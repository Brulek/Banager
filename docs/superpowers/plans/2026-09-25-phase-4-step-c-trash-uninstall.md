# Phase 4 Step C: Path-List Uninstall (Move to Trash) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a user uninstall the native Claude Code install from Canager safely: the uninstall dialog lists, in plain words, the three paths Anthropic's own instructions and installer name (`~/.local/share/claude`, `~/.claude/downloads`, `~/.local/bin/claude`) and the two it keeps (`~/.claude`, `~/.claude.json`); on confirmation Canager itself moves each listed path to the Trash with macOS's own `NSFileManager trashItemAtURL:` — no command, nothing deleted, the launcher last — after checks that run at preview time, again at the confirmation against what the preview saw, and once more immediately before each item moves; an uninstall stopped partway leaves exactly one state (program files in the Trash, a dangling launcher shown as a *launcher-only* row) that a second Uninstall finishes; and every sentence the row, the dialog, the trust file and the README say about it is true.

**Architecture:** `Plan` gains a two-arm `PlanAction { Command | TrashPaths }` in place of `program`/`args`/`env` (spec §6.2, Q16), and every construction site, the readers and the TypeScript mirror follow. A `Trasher` trait (`crates/banager-core/src/trash/`) is the seam, like `CommandRunner`/`HttpClient`: `RealTrasher` is `trashItemAtURL:` through `objc2-foundation` (macOS only), `MockTrasher` renames into a temporary directory and records every call. `Recipe` gains `uninstall: Option<Uninstall>` with `Uninstall::Paths`; `adapters/standalone/removal.rs` turns the list into a `TrashPaths` plan under the checks of spec §6.3 and two of this plan's — every folder between the home folder and a listed path is a real folder (Ruling 24), and nothing moved may take a kept path along (Ruling 25) — and records what the preview saw at each path (`ItemIdentity`: `st_dev`, `st_ino` and the kind), which travels with the plan in `PlanAction::TrashPaths.previewed`, a field serde skips so the window never sees it (Ruling 10). It executes the plan item by item: every check again and every identity against the preview's before anything moves; then, for each item on tokio's blocking pool, every check once more and the move with nothing in between (`Fault::PathChanged` otherwise; Rulings 26, 28). `route::probe` now accepts only a launcher one link away from its root, and `run_operation` verifies an uninstall through a new `Adapter::reconcile_after_uninstall` — the operation-aware reading B's plan hands to this step — which the standalone adapter answers with `route::probe_strict`, where "could not tell" is an error and never "gone" (Ruling 27).

**Tech Stack:** Rust (banager-core: `std::fs` lstat/canonicalize/`MetadataExt`, `objc2` 0.6 + `objc2-foundation` 0.3 under `cfg(target_os = "macos")` — both already in `Cargo.lock` through tauri, so no new crate is compiled; `tokio::task::spawn_blocking` for each item's last check and its move, `tokio::select!` for a cancellable pause after each item), TypeScript 5 `strict`, React 19, i18next, vitest.

**Spec:** `docs/superpowers/2026-09-24-phase-4-standalone-spec.md` (authoritative; Chinese). This plan implements §十 row C and argues from D8, §6.1–§6.3, §6.5 (`WillTrash`/`WillKeep`/`AlreadyGone`), §6.6 (the Claude Code dialog), §九 (9.1 wire, 9.2 copy, 9.4 tests, 9.5 trust file), 附录 A and 附录 B. Ground truth for the pre-merge verification: `~/dev/Canager/.superpowers/phase4/spike-trash-tcc.md`. This plan will live at `docs/superpowers/plans/2026-09-25-phase-4-step-c-trash-uninstall.md`.

## Baseline, and how to read the anchors below

Branch `feat/phase-4-standalone`, worktree `~/dev/Canager-phase4`, read at HEAD `3b5117a` on 2026-09-25: steps **A** (`e985eb8`…`a630222`) and **F** (`f1a246d`…`0c9ead8`, with the follow-up fixes `e67c627`, `d3890f4`, `2bc9371`) have landed, and B's plan is committed (`2a360c8`) with B's Tasks 1 and 2 (`31d756b`, `3b5117a`). **The rest of step B lands before this step executes** — at the time of writing its Task 3 is being built in that worktree (`b-task3-progress.md`). Everything B produces is treated here as existing **with the exact names and signatures of `docs/superpowers/plans/2026-09-24-phase-4-step-b-skeleton-claude.md`** (identical to `~/dev/Canager/.superpowers/phase4/plan-step-b-skeleton-claude.md`): `StandaloneAdapter { recipe, meta, runner, http, detected }` with `new(recipe, runner, http)`, `detect`, `read_version`, `artifact_key`, `inventory`, `search`, `check_updates`, `latest_version`, `plan`, `execute`, `reconcile`, `impl Adapter`, `all(runner, http)`; `Detected { home }`; `Recipe { id, meta_toml, route, version, latest, self_updates, upgrade }`, `Route`, `RouteKind::SymlinkIntoRoot`, `VersionCmd`, `VersionParse::FirstToken`, `Latest::ClaudeChannel`, `UpgradeCmd`; `recipes::{CLAUDE, RECIPES}`; `route::{expand, probe, lexical_join, shadow_note, Probe::{Absent, Present { real }, LauncherOnly}}`; `latest::*`; `#[cfg(test)] pub(super) mod testing { TempHome, ClaudeLayout, claude_layout }` in `standalone/mod.rs` (so reachable from `standalone`'s child modules' tests, **not** from `tests/`); the test helpers `exited_0`, `adapter`, `instance_for`, `request`, `detected_adapter` in `standalone/mod.rs`'s `mod tests`; `claude_home`/`claude_upgrade_outputs` in `tests/ops_upgrade_version_test.rs`; `InstanceNote::{NotOnPath, ShadowedByHomebrew, ShadowedByNpm, ShadowedByOther, LauncherOnly}`; `UninstallBlocked::NoSafeMethod` and `UNINSTALL_BLOCKED_KEYS.NoSafeMethod`; `ADAPTER_LABEL_KEYS["standalone-claude"]`; `STANDALONE_SUMMARY_KEYS`/`installedDescription`; `Session::new` extended with `standalone::all(runner, http)`; the `standalone-claude` row of `scan::owned_roots`; the recorded `adapters/fixtures/standalone-claude/<version>/`; the `## Claude Code` section of `docs/what-we-run.md`; the README row. **Where B's plan and the spec disagree, this plan follows B's plan** (it is newer and was reviewed twice) and says so in "Rulings" and "Deviations". All of step B has since landed (2026-09-25): Task 3 as `71eacd0`, Task 10 as `f61cd94`, Task 11 as `dcf0e7c`. Where the landed, rustfmt'd text differs from B's plan, the anchors below quote the landed text — the `reconcile` doc, the launcher-only inventory assertion, `Probe::LauncherOnly`'s doc, two multi-line `StandaloneAdapter::new(` calls in tests, and B's corrected install-test comment; the anchors into Tasks 10 and 11 (the README row and counts, the InstalledPage test comment, the summary sentence, the locale keys) were checked against the landed files and match.

Anchors: an edit to a file B creates or still changes (`adapters/standalone/*`, `adapters/mod.rs`, `model.rs`, `session/mod.rs`, `scan/mod.rs`, `lib.rs`, `http/real.rs`, `tests/ops_upgrade_version_test.rs`, `src/lib/{types,types.test,sources,sources.test}.ts`, `src/pages/{InstalledPage,UpdatesPage}.{tsx,test.tsx}`, `src/i18n/*.json`, `docs/what-we-run.md`, `README.md`) is anchored by symbol and quoted code, never by a line number alone. An edit to a file A or F touched and B does not (`UninstallDialog.{tsx,test.tsx}`, `warnings.{ts,test.ts}`, `format.{ts,test.ts}`, `completeness.test.ts`, `tests/what_we_run_test.rs`) is anchored by symbol. An edit to a file none of them touches cites `file:line` at `3b5117a`.

**`Plan {` literal constructions**, counted at `3b5117a` with `grep -rnE '(^|[^A-Za-z_])Plan\s*\{' crates src-tauri --include='*.rs'` minus `struct Plan` and the four `-> Plan {` function signatures: **23**, plus the one B adds (`StandaloneAdapter::plan`'s `Upgrade` arm) = **24** sites this plan converts. Spec §6.2 says "32 处"; that number is §3.2's count of `HostEnv {` literals, carried over (every site is listed in Task 1).

## Global Constraints

Copied verbatim from the spec's binding rules (spec lines 20–23):

> 产品规则一条不让（spec §1、§6）：每一步说人话；后台工作绝不问密码；执行前先看到确切命令；
> 结果诚实——版本没动是 `NeedsAttention(UnchangedAfterUpgrade)`，中途停止是 `Unconfirmed`，
> 没有证据绝不说成功；fixture 只收真机录制；Canager 不跑 shell、不把下载管进 `sh`；
> 界面绝不提供 Rust 会拒绝的操作；所有文案 en + zh-CN。

And from spec §十 ("每一步只带**该步有生产者**的变体与字段——「先定义、后面某步再用」正是本项目最常见的缺陷") and §2.2/§2.3 ("每个新字段点名生产读取方"), applied to this step:

- **Every new field, variant, constant or function names its production reader in the same task**, in its doc comment and in the task's Interfaces block, and that reader lands in the same task's commit. A wire variant whose *producer* is the core task (Task 6) lands with its front-end reader in its own task, as B's Tasks 1–2 did with `InstanceNote` and `NoSafeMethod`; anything whose only reader is Rust code of the core task lands in the core task, which is therefore one commit built in stages (B's Task 3 is the precedent: B's review finding 19). What this step does **not** define, because nothing in it produces it: `Uninstall::Command`/`Probe` (rustup, step E), `Expect::File`, `RemovedWhat::Backups`, `KeptWhat::{ToolState, ShellConfigLines, OutsideHome, NotOurs}`, `Recipe.backup_globs`/`Glob` and check 5 (step D), `Warning::{RemovesToolchains, DeletesCargoCaches, LeavesUnmanaged, EditsShellConfig, LeavesShellConfigLine}` and `operations.noCancelHint` (step E).
- **Honest outcomes.** `execute` for a `TrashPaths` plan returns `Succeeded` only when every listed path was moved; `Failed { exit_code: None, summary }` with macOS's own words when the system refused one; `Unconfirmed` when cancelled or out of time between items; `CanagerFailed(PathChanged)` when a path changed after the preview (a check now fails, the list differs, or a path is no longer the one the preview recorded — at the confirmation, or right before its turn); `CanagerFailed(Internal)` when Canager could not hand an item to the system at all (Ruling 9). `run_operation` then reads the disk again (`reconcile_after_uninstall`) and reports `Succeeded` only when the launcher is gone — `StillInstalledAfterUninstall` or `Cancelled` when it is still there, `Unconfirmed` when Canager cannot tell (Ruling 27) — never success on `execute`'s word alone.
- **Fixtures come from real machines only.** This step records nothing: every layout its tests need is synthetic, built by the test in a temporary directory (spec §9.3: removal tests never write under `adapters/fixtures/`). It appends provenance prose to B's fixture README (spec §3.1 puts the list's source there), leaving every recorded byte as it is.
- **No shell.** `PlanAction::Command.program` is only ever the instance's `exe_path`; a `TrashPaths` plan spawns nothing. The only file-system write in Canager's own process besides `settings.json` is `Trasher::trash`; the trait has no other method, and no code this step adds to a release build calls `remove_file`, `remove_dir_all`, `rename`, `create_dir` or `create_dir_all`. `MockTrasher` does all of those — it creates a temporary directory, renames items into it and removes it when dropped — so it is compiled only for tests: `#[cfg(any(test, feature = "test-support"))]`, a feature only dev-dependencies turn on (Ruling 20). A debug build — never a release one — also tries to list the Trash after each move and prints whether it may, for the author's pre-merge check (Ruling 30).
- **The UI never offers what Rust refuses.** Until Task 6 lands, Claude Code's artifact keeps `NoSafeMethod` and `plan(Uninstall)` keeps refusing it; Task 6 flips the recipe, the inventory, the plan, the `launcherOnly` sentence and every sentence of the trust file and the README that its behaviour makes true or false, in one commit (Ruling 23). The checks refuse at `plan()` with `UninstallUnsafe`, which the dialog words in the user's language (Task 5).
- **en + zh-CN for all copy.** Every new key in both `src/i18n/en.json` and `src/i18n/zh-CN.json`; `src/i18n/completeness.test.ts` requires each key to be looked up by a *literal* in non-test source (lookups go through `Record`s of literal keys, never assembled strings; the interpolated heads it knows are listed in `INTERPOLATED_SUBTREES`, which Task 4 extends); `src/i18n/no-literal-strings.test.ts` forbids English literals in JSX; zh-CN prose uses full-width `，：（）` between CJK characters; zh-CN carries only `_other` for a plural key, as `warnings.wouldBreak_other` does today.
- **No author-machine details in tests or source** beyond public tool names; every path in a test is under a temporary directory or a made-up `/Users/someone`.
- **The five gates**, from README.md "Tests — all five must pass before anything is committed" (the TypeScript gate is `pnpm typecheck`, two `tsc` programs):

  ```bash
  cargo fmt --all --check
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test --workspace
  pnpm test
  pnpm typecheck
  ```

  Run `cargo fmt --all` before the `--check` gate: the Rust below is written *for* rustfmt, and rustfmt decides line breaks.
- **Commits:** `git add <exact paths>` (never `-A`, never `.`), an imperative subject in sentence case, a body that says why, a blank line, then the attribution line. The commit blocks below end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`, the line this plan's author was told to use; an executing session told to use a different attribution replaces that whole line with its own and never adds a second.

## The pre-merge verification (spec §6.2, blocking): what the spike settled, and what is left

Spec §6.2 makes step C's merge conditional on a check from a Finder-launched development build **without** Full Disk Access (FDA), with the results written into `docs/what-we-run.md`: (1) `RealTrasher::trash` on a file and on a symbolic link succeeds, the items appear in the Trash, Finder's "Put Back" is available, and the link is moved as a link; (2) a `std::fs::rename` into `~/.Trash` from the same process fails with EPERM (expected). `~/dev/Canager/.superpowers/phase4/spike-trash-tcc.md` (2026-09-25, macOS 27.0 26A428, Apple silicon; objc2 0.6.4 / objc2-foundation 0.3.2 — the versions this repository's `Cargo.lock` already pins) ran an ad-hoc-signed app bundle through LaunchServices (`open -n`: parent `launchd`, its own bundle id — how Finder launches an app) 24 times without FDA, proven by `read_dir(~/.Trash)` answering `EPERM` in every LaunchServices run and `Ok` in every run of the same binary from the FDA shell.

| Spec item | Spike result | What this plan does with it |
|---|---|---|
| (1) `trashItemAtURL:` works without FDA | **Satisfied.** A file, a directory and a symlink moved into `~/.Trash` in 20/20 runs, no dialog, ~20 ms; a colliding name got the system's ` HH-MM-SS-mmm` suffix — so Claude Code's two paths both named `claude` are no problem (spike §3–§4). | `RealTrasher` makes exactly that call (Task 6, stage 6a). |
| (1) the link is moved as a link | **Satisfied for a link whose target exists** — `resulting_item_lstat: symlink`, the target still at its origin, every run, both contexts (the spike moved its link first, while the target was in place, spike §2). **Not tried: a dangling link** — and every Claude Code uninstall moves one: the launcher goes last, after the program directory it points into, so by its turn it dangles. See (c). | Kept, and hardened: the URL is built with `fileURLWithPath:isDirectory:` from the kind the item's last check saw (`lstat`), so no link ever gets a directory URL whose trailing slash could resolve through it, and no second look sits between that check and the call (Rulings 11, 26). |
| (1) "Put Back" available | **Verified by Finder's own record, with a finding.** Finder keeps Put Back as `ptbL`/`ptbN` entries in `~/.Trash/.DS_Store`; items trashed by the spike carried them, byte-for-byte as Finder writes them for its own trashings. **But** without FDA only the *first* item of a burst of calls ≤ 1.5 s apart got a record (15/15 runs); with ≥ 2 s between calls every item did (4/4 runs: 2 s ×3, 3 s ×1). Nobody clicked "Put Back" (headless); the mechanism is unknown (spike §5, §7). | **Author decision 1** (default taken): `execute` pauses `removal::PUT_BACK_SETTLE` = **3 s** after each item — before the next one, and after the last before it reports the run finished — the largest gap measured to work, a full second above the largest that failed; Cancel and the uninstall's time budget cut the pause short (Ruling 28); Claude Code's three items spend 9 s of a 120 s budget. The pause after the last item is there because every no-FDA run that recorded all items also stayed alive 3 s after its last call, and the record is written after the call returns (with FDA, a process that exited at once lost the later records, spike §5 reading 4): an immediate exit after the last call was never measured without FDA. It is an empirical knob from four runs on one Mac, and the trust file says so (Task 6, stage 6g): the pause makes Put Back likely for every item, not certain (Ruling 29). The dialog's sentence therefore promises what is certain — until the Trash is emptied, the items can be dragged back out of it — and says Finder's Put Back will likely work too. To drop the pause instead, set the constant to zero and change that sentence to say Put Back may be missing for all but the first item. |
| (2) `rename` into `~/.Trash` → EPERM | **Not reproduced: the rename succeeded** in 24/24 no-FDA runs — TCC protects *listing* `~/.Trash`, not adding to it (spike §3 (d)); the renamed files carry **no** Put Back record. | The spec's own sentence applies ("若意外成功，也不回到 `mv`"): the basename collision (`mv -n` silently skipping Claude Code's second `claude`, spec §0.1) and the missing Put Back both stand, so the design is unchanged. The trust file records the result (Task 6, stage 6g), and nothing in this plan says `mv` could not reach the Trash: that premise of spec §6.2 is disproved (Ruling 29). |

**What the spike could not reach**, so the blocking verification is satisfied **except for**:

- **(a) A click on "Put Back".** Only Finder's record was verified; the spike was headless.
- **(b) A Tauri build of Canager.** The spike bundle had no `NSApplication`; its `M1-runloop`/`W1`/`W2` runs (the trash call on a worker thread while the main thread spun a run loop, which is how Canager's tokio workers sit beside Tauri's main loop) behaved exactly like the plain runs, and an ad-hoc bundle is in the same TCC position as a Developer-ID-signed Canager (spike §7) — reasoning, not a run. Nor did the spike establish anything about Canager's own authorization: it proved "no Full Disk Access" for its own bundle, by that process being refused a listing of `~/.Trash`, and a Canager build the Mac had once been granted Full Disk Access would pass every other step below without testing the no-FDA case at all (Astra finding 8). The check's Step 3 establishes it for the build it runs.
- **(c) A dangling symbolic link, and a link to a directory.** The spike moved its link while the target was in place. Every Claude Code uninstall moves a *dangling* link — the launcher, last, after the program directory it points into has gone to the Trash — so every uninstall, not only a second one after a stop, depends on `trashItemAtURL:` accepting one. Task 7's `#[ignore]`d `RealTrasher` smoke test — gated on `CANAGER_LIVE=1` as well, like `brew_live`'s install test, because it changes the machine — moves both (plus a file, a directory and a link to a file). **Its dangling-link case is a merge blocker**: the author runs it once on their Mac (below; spec §9.4's "开发机手动跑一次") and CI runs it on every push (Task 7 adds the step). Neither run is the no-FDA context — a terminal with Full Disk Access, and a CI runner — so both verify the move itself, not Put Back without FDA; the author's Finder check below covers that, and the launcher it moves is a dangling link.
- **(d) Other macOS versions and Intel.** One Mac, macOS 27.0, Apple silicon. The deployment floor is macOS 13.3 (`tauri.conf.json`); `trashItemAtURL:` exists since 10.8 and nothing here needs a newer API, but TCC's behaviour on 13–26 and on Intel was not observed.

**The author's pre-merge check** — closes (a), (b) and (c); not for an agent: it needs a person's eyes on Finder, it changes the author's Trash, and it must never touch the author's real Claude Code install (this very session may be running on it). Its Finder part uses a throwaway home built inside a fresh private folder (`mktemp -d`, never a fixed path that could already exist as a link), confirms that Canager is really using it before anything is pressed, and establishes that the build it runs has no Full Disk Access.

**Step 1 — the smoke test**, in the author's own terminal: `CANAGER_LIVE=1 cargo test -p banager-core --test standalone_uninstall_test -- --ignored --nocapture`, then `sw_vers -productVersion` and `uname -m`. It leaves five `canager-trash-smoke-…` items in the Trash, and, being a debug build, prints `RealTrasher`'s `[canager] debug: read_dir(…/.Trash) -> …` line after each move. What that line says about a terminal (which often has Full Disk Access) does not count: Step 3 reads the Finder-launched build's own line. **If it fails**, stop: a link moved as its target, or a dangling link refused, means `RealTrasher` needs a different URL construction — a Task 6 fix and an author decision.

**Step 2 — a Finder-launched build**, against a throwaway home built inside a fresh private folder, whose fake Claude Code prints a version no real one has. From the worktree's root, in the author's own terminal. The block runs in a subshell, so a stop ends the block and not the terminal; `set -euC` stops it at the first failure and refuses to overwrite any existing file; `mktemp -d` makes a fresh, private (`0700`), unpredictable folder — never a fixed path, never reused — and every file below is created inside it and nowhere else. (No `#` comments inside the block: an interactive zsh would run them as words.)

```bash
(
set -euC
D=$(mktemp -d)
H="$D/home"
mkdir "$H"
mkdir -p "$H/.local/share/claude/versions" "$H/.local/bin" "$H/.claude/downloads" "$H/.claude/projects"
printf '#!/bin/sh\necho "0.0.1-canager-check (Claude Code)"\n' > "$H/.local/share/claude/versions/0.0.1-canager-check"
chmod 755 "$H/.local/share/claude/versions/0.0.1-canager-check"
ln -s "$H/.local/share/claude/versions/0.0.1-canager-check" "$H/.local/bin/claude"
printf '{}\n' > "$H/.claude.json"
pnpm tauri build --debug --bundles app --config '{"bundle":{"createUpdaterArtifacts":false}}'
open -n --stderr "$D/canager-stderr.log" --env HOME="$H" target/debug/bundle/macos/Canager.app
sleep 5
PID=$(pgrep -n -f 'Canager.app/Contents/MacOS/')
SEEN=$(ps eww -p "$PID" | tr ' ' '\n' | grep '^HOME=' || true)
if [ "$SEEN" != "HOME=$H" ]; then
  echo "STOP: Canager (pid $PID) runs with '${SEEN:-no HOME}', not HOME=$H. Quit it without pressing anything."
  exit 1
fi
PARENT=$(ps -o ppid= -p "$PID" | tr -d ' ')
if [ "$PARENT" != "1" ]; then
  echo "STOP: Canager (pid $PID) is a child of pid $PARENT, not of launchd: not a Finder-style launch. Quit it."
  exit 1
fi
echo "OK: Canager (pid $PID) runs under launchd with HOME=$H; its stderr goes to $D/canager-stderr.log"
)
```

It must end with the `OK:` line; after a `STOP:` line, quit Canager and stop. (`open` hands `--stderr` and `--env` to LaunchServices with the launch; the block checks that the app is still `launchd`'s child, the spike's proof of a Finder-style launch, and Step 3 establishes what that process may read.) In the window: Installed → *Claude Code*: **the row must show version `0.0.1-canager-check`**; if it shows any other version, quit Canager without pressing anything — it is looking at the real home. Then Uninstall → the dialog lists three moves and two kept paths → Uninstall → *Succeeded*. The operation log's three "Moved … (now at …)" lines say where each item went: under your real `~/.Trash`, spelled out in full because it is outside the throwaway home. If they went anywhere else (under the throwaway home's own `.Trash`, say), macOS chose the Trash by `HOME`, and Put Back cannot be checked in Finder this way — stop and report it.

**Step 3 — no Full Disk Access, for this very build.** (a) In System Settings → Privacy & Security → Full Disk Access, Canager is not listed, or its switch is off. If it is listed and on, turn it off (or remove it), quit Canager, and start again from Step 2 with a new block (a new folder). (b) Read the log the block named (`cat <that path>`): a debug build writes one line to stderr after each move, from `RealTrasher` (`cfg(debug_assertions)`; a release build does not contain it, Ruling 30), saying whether that process may list the Trash it has just used. All three must read `[canager] debug: read_dir(/Users/<you>/.Trash) -> Err: Operation not permitted (os error 1)` — the very process that moved the items was refused a listing of the Trash, which is how the spike proved its own runs had no Full Disk Access. A line ending `-> Ok: …` means this build has Full Disk Access: the check does not count; stop.

**Step 4 — Put Back.** In Finder's Trash, right-click each of the three items (`claude`, `downloads`, and the second `claude` — the launcher, a dangling link when it was moved, since the folder it points into had gone first) → **Put Back** → all three are back under the throwaway home. Refresh in Canager → the Claude Code row is back. Quit Canager; the throwaway folder can go to the Trash afterwards.

**Step 5 — the record.** The author adds one sentence to `docs/what-we-run.md`'s `## Moving files to the Trash`: the date, the macOS version and chip, that the smoke test passed there, that a Finder-launched debug build passed this check without Full Disk Access (not granted in System Settings, and refused a listing of the Trash by its own debug line), and on how many of the three items Put Back worked.

**If Put Back is missing for any item**, raise `PUT_BACK_SETTLE` and repeat; **if the move itself fails**, stop: the fallback spec §6.2 names (a `Trasher` that `create_dir`s a fresh `~/.Trash/Canager – <tool> <time>/` and `rename`s into it) fits the same trait, `Plan` and preview, but is a separate plan and an author decision.

## Rulings this plan makes

Where the spec leaves a choice to the step, or where this step's slice of the spec would define something nothing produces, the decision is made here so no task has to.

1. **`RemovedWhat`, `KeptWhat`, `Expect` and `Uninstall` are sliced to this step's producers** (spec §十's rule; B's ruling 1 applied again): `RemovedWhat { Launcher, Program, Cache }`, `KeptWhat { Settings, SettingsAndHistory }`, `Expect { SymlinkIntoRoot, Dir }`, `Uninstall { Paths }`. Claude Code produces exactly these; `Backups`, `ToolState`, `ShellConfigLines`, `OutsideHome`, `NotOurs`, `File`, `Command` arrive with agy and grok (D) and rustup (E), each with its `Record` row and copy. Consequences: check 5 (backup-file patterns) has no input until D adds `backup_globs`, so this step runs checks 1–4 and D adds 5; and an `optional` path whose shape does not match is **refused** here (`not_what_instructions_expect`) rather than skipped with `WillKeep { NotOurs }` — the skip is D's, with the variant, and D changes that one branch and its test. A test pins that refusal for `~/.claude/downloads` as a link and as a file (Task 6, `test_plan_removal_refuses_an_optional_path_of_the_wrong_shape`), so D's change to it is deliberate and visible.
2. **`Recipe.uninstall` is `Option<Uninstall>`**, as spec §3.1 writes it and as B's own `recipe.rs` module doc names it ("Step C adds the uninstall method (`uninstall: Option<Uninstall>`)"). B's ruling 1 ("non-optional where nothing produces `None`") is not applied: the `None` arm is the one production path of `UninstallBlocked::NoSafeMethod`, whose gate, copy record and page branch B shipped and which spec §十 keeps for the second batch's Ollama.app. A test-only recipe, `NO_UNINSTALL` in `standalone/mod.rs`'s tests, keeps that arm exercised.
3. **`Detected` gains `euid`** (written by `detect`, read by check 3) and derives `Clone, Debug`, so `plan`/`execute` copy it out of the mutex before awaiting. `launcher`/`real`/`cargo_home` are still not cached (B's ruling 2): `plan` and `execute` re-expand every path from `Detected.home` and the recipe, exactly as `detect` built `exe_path`.
4. **The launcher-only state is re-probed, not read from the instance's notes.** `removal::plan_removal` asks `route::probe` again; a `LauncherOnly` answer is what turns a missing non-optional path other than the launcher into `Warning::AlreadyGone`. The snapshot's note can be minutes old; the disk is not.
5. **Check 1 applies the spec's never-list as written: the path's canonical parent must start with the canonical home and must be neither the home itself nor one of `~/.local`, `~/.config`, `~/.cache`, `~/Library`, `~/.cargo`** (`recipe::SHARED_FOLDERS`; each compared both as the resolved home spells it and where it resolves, so a shared folder kept elsewhere in the home through a link still counts). Every Claude Code path passes (parents `~/.local/share`, `~/.claude`, `~/.local/bin`), and so does every Grok path spec §6.3 lists; a typo that lists `~/.local/bin` for `~/.local/bin/claude` does not — which is what the never-list is for ("防将来配方悄悄扩大爆炸半径"). The spec's other words, "之下至少两层", are not applied literally: read as "the parent at least two levels below home" they would refuse `~/.claude/downloads`, whose parent is one level down. The one listed path the never-list refuses is agy's `~/.cache/antigravity` (parent `~/.cache`), step D's: D decides it — an explicit, tested exception or a spec amendment — rather than this step widening the rule for a recipe it does not ship (backlog, Task 8). `recipes::tests` holds every `remove` path to the same rule as spelled, and the refusal has a reason of its own, `SharedFolder`, because "outside your home folder" would be false for it (Ruling 21).
6. **A `KeepSpec` is listed only when the path exists.** "Keeps: ~/.claude.json" on a Mac without that file would be a false sentence; the check is one `lstat`.
7. **Every path in a sentence (`WillTrash`, `WillKeep`, `AlreadyGone`, `PathChanged`, `UninstallUnsafe`, the two log notes) has `$HOME` abbreviated to `~`** by F's `scan::display_path`, made `pub(crate)` so the crate has one rule for `~` (spec §6.5: data for a sentence, not a path to act on). `PlanAction::TrashPaths.paths` and the trasher's calls stay absolute.
8. **Canager's two log lines are `LogNote`s, not `OperationEvent::Log`.** Spec §6.2 writes `Log { line: "Moved <path> to the Trash (<new location>)" }`; `events.rs`'s rule for `LogNote` ("travels as a key plus arguments, never as text") and the en + zh-CN constraint are stronger, so this step adds `LogNote::MovedToTrash { path, trashed_to }` and `LogNote::TrashFailed { path, error }` with their `LogDrawer.tsx` cases (Task 4).
9. **A refused item is `Outcome::Failed { exit_code: None, summary: <macOS's words> }`**, as spec §6.2 step 3 says, with the path in the log through `TrashFailed`. An item Canager cannot hand to the system at all — `TrashError::Unsupported`: not macOS, or a path that is not UTF-8, which `NSString` cannot carry — is `Outcome::CanagerFailed(Fault::Internal)` with no `TrashFailed` note: those are Canager's own words, never to be quoted as the Mac's (`Failed.summary` is another program's words, model.rs). `RealTrasher` answers it for every path alike, so it comes at the first item, before anything moved.
10. **`execute` reconstructs everything from `Detected` and the recipe, and compares it with what the preview saw** (spec §6.3; Astra finding 2): it re-runs `plan_removal` and refuses with `PathChanged` when a check now fails, when the fresh list differs from the plan's (naming the first path that appeared, else the first that disappeared), or when any path's identity — `st_dev`, `st_ino` and the kind, from `lstat` (`ItemIdentity`) — differs from the one the preview recorded; all of it before anything moves. Then each item's turn runs every check again and compares its identity with the preview's once more (Rulings 24–26). The preview's identities travel with the plan: `PlanAction::TrashPaths { paths, previewed }`, where `previewed` is `#[serde(skip)]` — the `IssuedPlan` the window receives never contains it, the TypeScript mirror does not change, and a plan read back from JSON has none, which `execute_removal` refuses as Canager's own bug (`Fault::Internal`). Checked in a scratch crate on serde 1.0.229, this `Cargo.lock`'s: the externally tagged variant with a skipped `Vec` field serialises as `{"TrashPaths":{"paths":[…]}}`, deserialises with the field empty, ignores the key when a payload names it, and keeps the field through `Clone` — so the stored plan `Session::submit` hands to `OperationManager::submit` still carries it, and `Adapter::execute`'s signature does not change. The alternative the controller named, a table keyed by plan id beside `session/plans.rs`'s `StoredPlan`, would need a new parameter or channel into `execute`: `submit` consumes the `StoredPlan` and passes only the `Plan` on. A Claude Code that updated itself between the preview and the click (its updater re-points the launcher, which makes a new link) therefore stops with `PathChanged`, and the user previews again; the trust file and `PathChanged`'s copy say so.
11. **`RealTrasher` compiles on every Unix target and moves only on macOS** (elsewhere `TrashError::Unsupported`, so a Linux build, which `lib.rs` says compiles, fails an uninstall honestly at run time). The URL is `NSURL::fileURLWithPath:isDirectory:` with `isDirectory` from the kind the item's last check saw — `Trasher::trash(path, kind)`, the kind from that check's `lstat`, so `RealTrasher` makes no look of its own between the check and the move (Ruling 26); a symlink is never a directory there — rather than the spike's `fileURLWithPath:`, which stats through a link to decide. A path that is not UTF-8 is `TrashError::Unsupported`, not a `Refused` carrying an English sentence of Canager's (Ruling 9).
12. **Tests set the pause to zero** with `StandaloneAdapter::with_trash_gap(Duration::ZERO)` (a public builder like `BrewAdapter::with_background_change`, reachable from `tests/`), and the `#[ignore]` smoke test calls `RealTrasher` directly, so no test waits seconds per item.
13. **Copy.** B's `NoSafeMethod` sentence stays (B's ruling 15): after this step it is produced only for a recipe without an uninstall method, for which it is still true. `sourceNotice.launcherOnly.description` gets back the three promises of spec §9.2 that B withheld (the files may be in the Trash after a stopped uninstall; Uninstall moves the link; put the folder back and refresh) while keeping B's reviewed correction that another installation may still run when the command is typed (B's Astra finding 9), so the sentence says "this link can't run", not "typing claude fails".
14. **`testing::{command_program, command_args, command_env}`** (Task 1) are how the ~40 existing test assertions read a `Command` plan's parts after the fields move; each panics on a `TrashPaths` plan, saying so. They live in the public `crate::testing` (reachable from `tests/` and `src-tauri`), return `&Path`/`&[String]`/`&[(String, String)]`, and no production code calls them.
15. **Operation-aware verification (B's deviation 15, "Step C handoff").** `run_operation` reads `Adapter::reconcile_after_uninstall` after an `Uninstall` and `reconcile` after everything else. The new trait method has a default body (`self.reconcile(inst, key).await`), so the seven package-manager adapters and every test fake are unchanged; `StandaloneAdapter` overrides it to answer presence alone, so a launcher-only launcher is "still there" after an uninstall while `reconcile` keeps B's strict refusal for upgrades — and a launcher it cannot look at is an error, never absence (`route::probe_strict`, Ruling 27). No state is kept between `execute` and the reading.
16. **The `uninstall_unsafe` refusal is shown as its own sentence**, without `uninstall.planError`'s "Couldn't check what this would affect:" frame — Canager did check; the six sentences already say nothing was changed. The same reason the pinned refusal skips the frame (`refusalText`).
17. **`operations.outcome.CanagerFailed.PathChanged` says what is true for a stop in the middle too**: spec §9.2's "so Canager didn't move anything" is true only when the first item changed; the sentence names the path, says Canager stopped without moving it, and points at the log for anything moved before.
18. **The end-to-end test runs through `Session`** (`refresh` → `issue_plan`'s gate → `submit` → `run_operation`), not only `OperationManager`, so it also proves the gate lets an unblocked Claude Code uninstall through and that the next refresh shows the launcher-only row.
19. **The pause is per operation.** Two uninstalls whose moves fall within ~2 s of each other could still lose a Put Back record for the second's first item; a second uninstall needs a fresh preview and a confirmation, which take longer than that. Recorded in the backlog (Task 8), with its shape.
20. **`MockTrasher` is test-only code.** It creates a temporary directory, renames into it and removes it on drop — a permanent delete of whatever was moved into it — so it is compiled only under `#[cfg(any(test, feature = "test-support"))]`, the crate's existing feature for test code that touches real state (`testing::expire_issued_plans`), and banager-core's own integration tests turn that feature on through a dev-dependency on the crate itself (`banager-core = { path = ".", features = ["test-support"] }`). Checked in a scratch workspace of the same shape on cargo 1.98.1 (a second member that also enables the feature under `[dev-dependencies]`): `cargo test --workspace` sees the type in unit and integration tests, `cargo clippy --workspace --all-targets -- -D warnings` is clean, and a release build does not have the feature.
21. **Six refusal reasons, not four.** Check 1 has two findings a sentence must keep apart: the folder leads out of the home folder (`OutsideHome`), and the folder is the home folder or a shared one (`SharedFolder`, Ruling 5) — "it's outside your home folder" would be false for `~/.local/bin`. `NotWhatInstructionsExpect`'s sentence says Canager *couldn't confirm* the path is what the instructions describe, naming the usual causes as possibilities — the path, or a folder it is in, may be a link to somewhere else (Ruling 24), or it may be a different kind of file — because it is also the answer for a path that could not be examined at all. The sixth, `OverlapsKept`, is Ruling 25's, and its sentence names the kept path, not a listed one.
22. **The preview's label follows the arm.** `CommandPreview` shows `commandPreview.label` ("This will run:") above an argv, and `commandPreview.trashLabel` ("What Canager will do:"; zh "将执行：", the label spec §6.6's dialog shows) above the `TrashPaths` sentence, so "no command runs" never sits under "This will run:".
23. **The documents change in the commit that makes them true.** Task 6's commit (stage 6g) carries every trust-file and README sentence its behaviour makes true or false — the Claude Code section's uninstall, "Files Canager writes", the new Trash section, the never-list, the README row and safety bullets — with the two `what_we_run_test` tests that hold them to the code. Task 7 adds what describes its own test (the smoke test's sentences, the ignored-tests paragraph, "plus 3 more"); Task 8 the backlog, the Language sections and the test counts, which only lag, never contradict.
24. **The ancestry rule** (Astra findings 1 and 3; the controller's ruling). Every folder between the resolved home folder and a listed path must be a real folder, not a link: for the folder the path is in, `canonicalize(folder)` must equal the resolved home joined with the recipe's own spelling of that folder (`removal::check_item`). The listed path itself may be a link only where the recipe says `Expect::SymlinkIntoRoot` (for `Dir` its `lstat` must say directory). One rule defeats `~/.claude -> ~/.local/share/claude`, `~/.claude -> ~/Documents` (an unrelated `~/Documents/downloads` would otherwise pass as Claude Code's cache) and an ancestor renamed away and replaced by a link between two moves while the item keeps its inode. The home folder itself may still be reached through a link (`HostEnv.home`): both sides are resolved. Check 1 runs first, so a folder that leads outside the home folder keeps its own sentence (`OutsideHome`) and a shared folder its own (`SharedFolder`); a link that stays inside the home folder is refused as `NotWhatInstructionsExpect`, whose sentence now says a folder the path is in may be a link (Ruling 21). Consequence, deliberate: a `~/.local/bin` kept as a link to a dotfiles folder now refuses the uninstall where spec §6.3's check 1 accepted it, and so does a `~/.claude` kept as a link while `~/.claude/downloads` exists inside it (without the cache nothing listed lies under it, and the uninstall goes ahead) — detection still lists the row (route.rs resolves the launcher's folder first), the dialog says why, and the backlog records the shape of a relaxation (Task 8). And because every listed path's folders are real folders, two listed paths can overlap on the disk only as they overlap as spelled, which `recipes::tests` already forbids.
25. **Kept paths stay kept** (Astra finding 1; the controller's ruling). With every link resolved, a path to be moved must not be a kept path, hold one, or hold what one leads to, and may lie inside a kept path only where the recipe lists it there — `~/.claude/downloads` inside `~/.claude`, and only while `~/.claude` is a real folder (`removal::disturbed`). The controller's words were "no path to be moved lies inside or equals a path to be kept and vice versa"; taken literally they refuse Claude Code's own list, whose cache lies by design inside the kept `~/.claude` (spec §6.3's claude row), so the one nesting the recipe spells is allowed and every other is refused. A kept path that exists but cannot be placed (its folder or its target unreadable) refuses too. The refusal is a sixth reason, `OverlapsKept`, whose sentence names the kept path and says Canager couldn't confirm the moves leave it where it is; none of the five existing sentences would be true of it (with `~/.claude -> ~/.local/share/claude`, no listed path is a link and no folder leads outside the home folder). At run time the same finding is `PathChanged`, naming the kept path. The check runs in `plan_removal` and in every item's turn.
26. **The last check is immediately before the move** (Astra finding 4; the controller's ruling: not a blocker). What the checks guard against is change by accident — the tool's own updater, the user, another app doing its ordinary work — not a hostile process running as the user, which already has every right Canager has. Each item's turn (`removal::take_turn`, on the blocking pool) runs every check from a fresh look, and the last of them is the item's own `lstat`, whose identity is compared with the preview's and whose kind is handed to `Trasher::trash`; nothing touches the disk between that `lstat` and `trashItemAtURL:`, because `RealTrasher` no longer looks at the path itself (Ruling 11) and building the URL reads nothing. What remains is the gap a pathname call cannot close — the system's move takes a path, and an item swapped between the last check and the call is the item it moves — and the trust file and the code say so in one sentence: "Canager checks each item immediately before moving it; a program running as you that swaps the item in that instant could still race it." Two tests pin the boundary with `MockTrasher`: a substitution before an item's turn is caught; one made inside the move itself is what gets moved (out of scope, documented).
27. **The launcher is one link into its root, and "could not tell" is not "gone"** (Astra finding 6; the controller's ruling). `route::probe` accepts only a launcher whose own text, taken from its resolved folder, names a place inside the resolved root (`one_hop`), besides resolving there: `claude -> ~/.local/bin/claude-current -> ~/.local/share/claude/versions/<v>` is not the installer's layout and is no instance — the Unknown page lists it — because once the root is in the Trash the middle link dangles, the launcher's own text no longer points into the root, and a stopped uninstall would read as a finished one. A link the tool keeps inside its root (a `current`) is its own business: B's two-hop test stands, and a launcher dangling through such a link is launcher-only. `route::probe_strict` is `probe` keeping its errors — `Ok(Absent)` only when the launcher's `lstat` says it is not there or what is there is not this route's, `Err` for any other error on the way — and `probe` is `probe_strict(..).unwrap_or(Absent)`, B's behaviour for detection and refresh. `StandaloneAdapter::reconcile_after_uninstall` reads presence with `probe_strict` and reads no version, so a permission error during the reading after an uninstall is an `Err`, which `run_operation` already reports as `Unconfirmed`, never `Succeeded`. B's `reconcile` (the upgrade's reading) keeps `probe`; that its `Absent` can hide an error is recorded in the backlog (Task 8), not changed here.
28. **Each move runs on the blocking pool; the budget stops Canager between items** (Astra finding 7; the controller's ruling). `trashItemAtURL:` blocks; each item's turn — its checks and its move — runs in `tokio::task::spawn_blocking` and is awaited to its end even when Cancel arrives meanwhile: a move already handed to the system finishes and is logged, and only then does the run stop, so nothing is still moving on another thread when `execute_removal` reports. Every pause is cut to what is left of the budget, and the budget is checked before each item. A turn that panics is `Unconfirmed` (whether that item moved is unknown; `run_operation` reads the disk). The trust file says "Canager stops between items once the budget is spent", not "120 s in all".
29. **Put Back is observed, not promised; `mv`'s real faults** (Astra finding 9; the controller's ruling). Every sentence about Put Back — code comments, the trust file, the README, the delivery note — says what was observed and that the 3-second pause makes it likely, not certain; an item without Finder's record can still be dragged back out of the Trash. The preview's sentence (`uninstall.trashPreview`, both locales) is reworded the same way, since "put them back" reads as Finder's Put Back: it promises what is certain (the items can be dragged back until the Trash is emptied) and says Finder's Put Back will likely work too. No sentence says `mv` could not reach a TCC-protected Trash — the spike disproved that premise (a rename into `~/.Trash` succeeded without Full Disk Access); the reasons against `mv` are the basename collision and the missing Put Back record.
30. **A debug-only line tells the pre-merge check what the build itself was allowed** (Astra finding 8; the controller's ruling). After each move, a debug build's `RealTrasher` tries to list the Trash it has just used and prints the answer to stderr (`#[cfg(all(target_os = "macos", debug_assertions))]`): without Full Disk Access macOS refuses with `Operation not permitted`, which is how the spike proved its runs had none. It runs after the move, never between a check and the move, and a release build does not contain it. The author's check launches the debug build with `open --stderr` and reads the line (Step 3 of "The author's pre-merge check").

## What already exists (do not rebuild)

- At `3b5117a`: `Plan`/`OpRequest`/`ResourceLock`/`CancelPolicy`/`Outcome`/`Fault`/`Attention`/`Warning`/`Reconciled` (`model.rs`); `run_plan` and the `Adapter` trait (`adapters/mod.rs`); `OperationManager::{new, register_adapter, register_instance, submit, wait, cancel, summaries}` and `run_operation`'s verification arms (`ops/mod.rs:435-808`, the reading after `execute` at `:668`); `execute_error_outcome` (`ops/mod.rs:43-65`); `plan_operation_error` (`src-tauri/src/ipc.rs:184-217`); `LogNote`/`OperationEvent`/`VecSink` (`events.rs`); `MockRunner`, `MockHttpClient`; `crate::testing::manager_instance` (`testing.rs:44-56`); `HostEnv { path_dirs, home, euid, cargo_home, ollama_host }` (`runner/path_env.rs:4-25`); `Session::{with_adapters, refresh, issue_plan, submit, cancel, operations, snapshot}`.
- Front end at `3b5117a`: `CommandPreview` (`src/components/CommandPreview.tsx`, props `{ program, args }`, rendered in `UninstallDialog.tsx` and `UpdatesPage.tsx`); `warningKey`/`warningArgs`/`warningTexts` with `never` defaults (A, `src/lib/warnings.ts`); `outcomeKey`/`outcomeArgs`/`faultKey`/`faultArgs` (`src/lib/format.ts`); `planErrorMessage` → `planFailureMessage`'s `switch` and `parseErrorPayload` (`src/lib/sources.ts`); `UninstallDialog`'s `refusalText`; `LogDrawer.tsx`'s `noteText` with its `never` default; `INTERPOLATED_SUBTREES["operations.outcome"]` (`src/i18n/completeness.test.ts`); `displayToken` (`format.ts`).
- From F: `scan::display_path` (private in `scan/mod.rs`, with `test_display_path_abbreviates_home_and_only_home`), `owned_roots`, the Unknown page.
- From B: everything listed under "Baseline" above.

## File Structure

```
crates/banager-core/Cargo.toml                                    MOD  objc2 + objc2-foundation under [target.'cfg(target_os = "macos")'.dependencies]; the crate itself under [dev-dependencies] with test-support, so tests/ see MockTrasher; the feature's comment (6a)
Cargo.lock                                                        MOD  banager-core's dependency list gains the two, and banager-core itself (6a)
crates/banager-core/src/lib.rs                                    MOD  `pub mod trash;`; one crate-doc sentence (6a)                     [B's file]
crates/banager-core/src/trash/mod.rs                              NEW  Trasher trait (`trash(path, kind)`), TrashError (6a)
crates/banager-core/src/trash/real.rs                             NEW  RealTrasher: trashItemAtURL: on macOS, the URL from the caller's kind; Unsupported elsewhere; the debug-only Full Disk Access line (6a)
crates/banager-core/src/trash/mock.rs                             NEW  MockTrasher, test-only (cfg(test) or feature test-support): rename into a temp dir, call and kind log, refuse-nth, cancel-after-nth (6a)
crates/banager-core/src/model.rs                                  MOD  PlanAction + Plan.action (1); RemovedWhat, KeptWhat, Warning ×3 (3); Fault::PathChanged (4); UninstallUnsafeReason ×6 (5); ItemKind (6a); ItemIdentity (6c); TrashPaths.previewed, serde-skipped, and its wire test (6e); doc comments (6f)   [B's file]
crates/banager-core/src/testing.rs                                MOD  command_program / command_args / command_env (1)
crates/banager-core/src/events.rs                                 MOD  LogNote::{MovedToTrash, TrashFailed} + shape test (4)
crates/banager-core/src/adapters/mod.rs                           MOD  run_plan matches PlanAction (1); Adapter::reconcile_after_uninstall (2); AdapterError::UninstallUnsafe (5)   [B's file]
crates/banager-core/src/ops/mod.rs                                MOD  argv_preview matches PlanAction (1); the reading after an uninstall (2); execute_error_outcome arm (5)
crates/banager-core/src/adapters/{brew/mod,cargo,npm,pip,pipx,uv,ollama/mod}.rs   MOD  Plan literals → action; test assertions via testing::command_* (1)
crates/banager-core/src/session/{test_support,plans}.rs           MOD  fake_plan literal; one assertion (1)
crates/banager-core/src/scan/mod.rs                               MOD  display_path becomes pub(crate) (6c)                             [B's + F's file]
crates/banager-core/src/adapters/standalone/recipe.rs             MOD  Uninstall, RemoveSpec, KeepSpec, Expect, SHARED_FOLDERS; Recipe.uninstall (6b)  [B's file]
crates/banager-core/src/adapters/standalone/recipes.rs            MOD  CLAUDE.uninstall; three invariants tests (6b)                   [B's file]
crates/banager-core/src/adapters/standalone/removal.rs            NEW  Job, Removal, the checks (check_item: check 1, ancestry, kept paths, check 4, the last lstat), plan_removal (6c); TIMEOUT_SECS, PUT_BACK_SETTLE, Confirmed, Pacing, take_turn, execute_removal (6d)
crates/banager-core/src/adapters/standalone/mod.rs                MOD  `pub mod removal;`, Detected.euid, testing::Unreadable (6c); trasher, trash_gap, new/with_trash_gap, inventory, plan, execute, reconcile_after_uninstall (probe_strict), all; B's tests updated; new tests (6e)   [B's file]
crates/banager-core/src/adapters/standalone/route.rs              MOD  the one-hop launcher: probe_strict, one_hop, probe over it, three tests (6c); Probe::LauncherOnly doc (6f)   [B's file]
crates/banager-core/src/session/mod.rs                            MOD  Session::new injects RealTrasher (6e)                           [B's file]
adapters/fixtures/standalone-claude/<the recorded version>/README.md   MOD  "Uninstall list" provenance section (6b)                  [B's file]
crates/banager-core/tests/ops_outcome_test.rs                     MOD  Plan literal (1); the split-reading adapter and its tests (2)
crates/banager-core/tests/ops_{cancel,fault,lock,panic,semaphore,summaries}_test.rs, brew_live.rs   MOD  Plan literals / assertions (1); one TrashPaths argv_preview test in ops_summaries_test (1)
crates/banager-core/tests/ops_upgrade_version_test.rs             MOD  the two StandaloneAdapter::new calls gain a trasher (6e)        [B's file]
crates/banager-core/tests/standalone_uninstall_test.rs            NEW  end to end through Session with MockTrasher; #[ignore] RealTrasher smoke (7)
crates/banager-core/tests/what_we_run_test.rs                     MOD  two tests: every listed path named; the call and the pause stated (6g)
.github/workflows/ci.yml                                          MOD  the RealTrasher smoke step (7)
src-tauri/src/ipc.rs                                              MOD  FakeAdapter's Plan literal + one assertion (1); uninstall_unsafe arm + tests (5)
src/lib/types.ts, types.test.ts                                   MOD  PlanAction, Plan.action (1); RemovedWhat, KeptWhat, Warning (3); Fault, LogNote (4); UninstallBlocked doc (6f)   [B's file]
src/components/CommandPreview.tsx, CommandPreview.test.tsx        MOD  props { action }, the TrashPaths branch and its own label (1)
src/components/UninstallDialog.tsx, UninstallDialog.test.tsx      MOD  the CommandPreview call, two plan fixtures (1); the item list test (3); the uninstall_unsafe refusal (5)
src/pages/UpdatesPage.tsx, UpdatesPage.test.tsx                   MOD  the CommandPreview call; the fixture's plan (1)               [B's file]
src/lib/{queries,api}.test.ts, src/pages/InstalledPage.test.tsx   MOD  plan fixtures (1); one positive Claude Code row test, one comment (6f)
src/lib/warnings.ts, warnings.test.ts                             MOD  REMOVED_WHAT_KEYS, KEPT_WHAT_KEYS, three branches (3)          [A's file]
src/lib/format.ts, format.test.ts                                 MOD  faultKey/faultArgs PathChanged (4)
src/components/LogDrawer.tsx, LogDrawer.test.tsx                  MOD  noteText MovedToTrash / TrashFailed (4)
src/lib/sources.ts, sources.test.ts                               MOD  UninstallUnsafeReason, UNINSTALL_UNSAFE_KEYS, parseUninstallUnsafe, the switch case (5); the launcherOnly comment and assertions (6f)   [B's file]
src/i18n/completeness.test.ts                                     MOD  INTERPOLATED_SUBTREES["operations.outcome"] += "CanagerFailed.PathChanged" (4)
src/i18n/en.json, zh-CN.json                                      MOD  uninstall.trashPreview, commandPreview.trashLabel (1); warnings.* (3); operations.* (4); planRefused.uninstallUnsafe.* (5); sourceNotice.launcherOnly.description (6f)   [B's file]
docs/what-we-run.md                                               MOD  the intro; when commands run; Claude Code's uninstall; files read/written; the Trash section; the never-list (6g); the smoke test's sentences (7)   [A's + B's + F's file]
docs/superpowers/backlog.md                                       MOD  four entries: the pause is per operation; check 1's never-list against agy's cache; a linked ~/.local/bin refuses the uninstall; the upgrade's reading still reads a probe error as absence (8)
README.md                                                         MOD  the Claude Code row; the exact-command bullet and two new safety bullets (6g); the ignored tests and "plus 3 more" (7); the Language section (en, zh); the counts (8)   [B's + F's file]
```

Single responsibility: `trash/` knows how to move one item and nothing about tools; `removal.rs` knows the checks and the order and nothing about the `Adapter` contract; `route.rs` knows what a launcher is; `recipe.rs`/`recipes.rs` hold the list as data; `standalone/mod.rs` wires the `Adapter` contract over them; `model.rs` holds what crosses IPC, plus the two small types a plan carries on this side only (`ItemKind`, `ItemIdentity`, in a field serde skips).

## Core Interfaces (authoritative — every task uses these names verbatim)

```rust
// crates/banager-core/src/model.rs
pub enum PlanAction {
    Command { program: PathBuf, args: Vec<String>, env: Vec<(String, String)> },
    TrashPaths { paths: Vec<PathBuf>, #[serde(skip)] previewed: Vec<ItemIdentity> },   // `previewed` arrives in Task 6 (6e); Task 1 has `{ paths }`
}
pub enum ItemKind { File, Dir, Symlink, Other }                                     // Clone, Copy, Debug, PartialEq, Eq; no serde (6a)
pub struct ItemIdentity { pub dev: u64, pub ino: u64, pub kind: ItemKind }          // same (6c)
pub struct Plan { pub request: OpRequest, pub action: PlanAction, pub needs_password: bool, pub locks: Vec<ResourceLock>,
                  pub cancel_policy: CancelPolicy, pub warnings: Vec<Warning>, pub affected: Vec<String>, pub timeout_secs: u64 }
pub enum RemovedWhat { Launcher, Program, Cache }                                   // Clone, Copy, Debug, PartialEq, Eq, serde
pub enum KeptWhat { Settings, SettingsAndHistory }                                   // same
pub enum Warning { …the six at HEAD…, WillTrash { path: String, what: RemovedWhat }, WillKeep { path: String, what: KeptWhat }, AlreadyGone { path: String } }
pub enum Fault { …the five at HEAD…, PathChanged { path: String } }
pub enum UninstallUnsafeReason { OutsideHome, SharedFolder, Missing, NotOwnedByYou, NotWhatInstructionsExpect, OverlapsKept }   // Clone, Copy, Debug, PartialEq, Eq; no serde

// crates/banager-core/src/events.rs
pub enum LogNote { WaitingForBrewUpdate { minutes: u64 }, ReadFailed { stream: Stream, error: String },
                   MovedToTrash { path: String, trashed_to: String }, TrashFailed { path: String, error: String } }

// crates/banager-core/src/adapters/mod.rs
pub enum AdapterError { …, UninstallUnsafe { path: String, reason: UninstallUnsafeReason }, … }
pub trait Adapter { …; async fn reconcile_after_uninstall(&self, inst: &ManagerInstance, key: &ArtifactKey)
                         -> Result<Reconciled, AdapterError> { self.reconcile(inst, key).await } }

// crates/banager-core/src/testing.rs
pub fn command_program(plan: &Plan) -> &Path;
pub fn command_args(plan: &Plan) -> &[String];
pub fn command_env(plan: &Plan) -> &[(String, String)];

// crates/banager-core/src/scan/mod.rs (F's; visibility widened)
pub(crate) fn display_path(path: &Path, home: &Path) -> PathBuf;

// crates/banager-core/src/trash/mod.rs, real.rs, mock.rs
pub trait Trasher: Send + Sync { fn trash(&self, path: &Path, kind: ItemKind) -> Result<PathBuf, TrashError>; }   // kind: what the last check's lstat saw
pub enum TrashError { Refused { detail: String }, Unsupported }   // Debug, thiserror; Refused = the system's words, Unsupported = Canager could not ask (not macOS, not UTF-8)
pub struct RealTrasher;  impl RealTrasher { pub fn new() -> RealTrasher }            // + Default
// `pub mod mock`, `pub use mock::MockTrasher` and everything below: #[cfg(any(test, feature = "test-support"))]
pub struct MockTrasher;  impl MockTrasher { pub fn new() -> MockTrasher; pub fn bin(&self) -> &Path; pub fn calls(&self) -> Vec<PathBuf>; pub fn kinds(&self) -> Vec<ItemKind>;
                                            pub fn refuse_call(&self, nth: usize, detail: &str); pub fn cancel_after_call(&self, nth: usize, token: CancellationToken) }   // + Default

// crates/banager-core/src/adapters/standalone/recipe.rs
pub struct Recipe { …B's seven fields…, pub uninstall: Option<Uninstall> }
pub enum Uninstall { Paths { remove: &'static [RemoveSpec], keep: &'static [KeepSpec] } }
pub struct RemoveSpec { pub path: &'static str, pub expect: Expect, pub what: RemovedWhat, pub optional: bool }
pub enum Expect { SymlinkIntoRoot, Dir }                                             // Clone, Copy, Debug, PartialEq, Eq
pub struct KeepSpec { pub path: &'static str, pub what: KeptWhat }
pub const SHARED_FOLDERS: [&str; 5] = [".local", ".config", ".cache", "Library", ".cargo"];   // check 1's never-list, besides home itself

// crates/banager-core/src/adapters/standalone/route.rs (B's; stage 6c adds)
pub fn probe_strict(kind: RouteKind, launcher: &Path, root: &Path) -> std::io::Result<Probe>;   // probe = probe_strict(..).unwrap_or(Absent)

// crates/banager-core/src/adapters/standalone/removal.rs
pub struct Job { pub recipe: &'static Recipe, pub detected: Detected, pub remove: &'static [RemoveSpec], pub keep: &'static [KeepSpec] }   // Clone, Debug; owned, for the blocking pool
pub struct Removal { pub paths: Vec<PathBuf>, pub identities: Vec<ItemIdentity>, pub warnings: Vec<Warning> }   // Debug, PartialEq, Eq
pub fn plan_removal(job: &Job) -> Result<Removal, AdapterError>;
pub const TIMEOUT_SECS: u64 = 120;
pub const PUT_BACK_SETTLE: Duration = Duration::from_secs(3);
pub struct Confirmed<'a> { pub paths: &'a [PathBuf], pub previewed: &'a [ItemIdentity] }   // Clone, Copy, Debug
pub struct Pacing { pub settle: Duration, pub budget: Duration }                     // Clone, Copy, Debug
pub async fn execute_removal(job: &Job, confirmed: Confirmed<'_>, trasher: &Arc<dyn Trasher>, pacing: Pacing,
                             sink: Arc<dyn EventSink>, op_id: OpId, cancel: CancellationToken) -> Result<Outcome, AdapterError>;

// crates/banager-core/src/adapters/standalone/mod.rs
pub struct Detected { pub home: PathBuf, pub euid: u32 }                              // Clone, Debug
pub struct StandaloneAdapter { recipe, meta, runner, http, trasher: Arc<dyn Trasher>, trash_gap: Duration, detected: Mutex<Option<Detected>> }
impl StandaloneAdapter {
    pub fn new(recipe: &'static Recipe, runner: Arc<dyn CommandRunner>, http: Arc<dyn HttpClient>, trasher: Arc<dyn Trasher>) -> StandaloneAdapter;
    pub fn with_trash_gap(self, gap: Duration) -> StandaloneAdapter;
    pub async fn reconcile_after_uninstall(&self, inst: &ManagerInstance, key: &ArtifactKey) -> Result<Reconciled, AdapterError>;
    // detect / inventory / check_updates / search / plan / execute / reconcile as B wrote them, changed as Task 6 says
}
pub fn all(runner: Arc<dyn CommandRunner>, http: Arc<dyn HttpClient>, trasher: Arc<dyn Trasher>) -> Vec<Arc<dyn Adapter>>;
```

```ts
// src/lib/types.ts
export type PlanAction = { Command: { program: string; args: string[]; env: [string, string][] } } | { TrashPaths: { paths: string[] } };
export interface Plan { request: OpRequest; action: PlanAction; needs_password: boolean; locks: string[]; cancel_policy: CancelPolicy; warnings: Warning[]; affected: string[]; timeout_secs: number }
export type RemovedWhat = "Launcher" | "Program" | "Cache";
export type KeptWhat = "Settings" | "SettingsAndHistory";
export type Warning = …the six… | { WillTrash: { path: string; what: RemovedWhat } } | { WillKeep: { path: string; what: KeptWhat } } | { AlreadyGone: { path: string } };
export type Fault = …the five… | { PathChanged: { path: string } };
export type LogNote = …the two… | { MovedToTrash: { path: string; trashed_to: string } } | { TrashFailed: { path: string; error: string } };
// src/components/CommandPreview.tsx
export interface CommandPreviewProps { action: PlanAction }
// src/lib/sources.ts
export type UninstallUnsafeReason = "outside_home" | "shared_folder" | "missing" | "not_owned_by_you" | "not_what_instructions_expect" | "overlaps_kept";
export const UNINSTALL_UNSAFE_KEYS: Record<UninstallUnsafeReason, string>;
export function parseUninstallUnsafe(message: string): { path: string; reason: UninstallUnsafeReason } | null;
```

IPC kind (Task 5): `{"kind":"uninstall_unsafe","path":"~/.local/bin/claude","reason":"not_what_instructions_expect"}`, the reason spelled by an exhaustive `match` in `plan_operation_error`, never by serde. The TypeScript `Plan`/`PlanAction` mirror is Task 1's and never changes for `previewed`, which does not cross the wire.

New locale keys, all in both files: `uninstall.trashPreview_one`/`_other`, `commandPreview.trashLabel` (Task 1); `warnings.willTrash.{Launcher,Program,Cache}`, `warnings.willKeep.{Settings,SettingsAndHistory}`, `warnings.alreadyGone` (Task 3); `operations.outcome.CanagerFailed.PathChanged`, `operations.logNote.{movedToTrash,trashFailed}` (Task 4); `planRefused.uninstallUnsafe.{outsideHome,sharedFolder,missing,notOwnedByYou,notWhatInstructionsExpect,overlapsKept}` (Task 5). Changed: `sourceNotice.launcherOnly.description` (Task 6).

## Task List

| # | Task | Deliverable |
|---|---|---|
| 1 | `PlanAction` in place of `program`/`args`/`env`, everywhere | the plan shape a no-command uninstall needs; `CommandPreview` says "Canager moves … itself" for it |
| 2 | Operation-aware uninstall verification: `Adapter::reconcile_after_uninstall` | `run_operation` asks only "is it still there?" after an uninstall |
| 3 | `Warning::{WillTrash, WillKeep, AlreadyGone}`, `RemovedWhat`, `KeptWhat`, their copy | the dialog's item list, in both languages |
| 4 | `Fault::PathChanged`, `LogNote::{MovedToTrash, TrashFailed}`, their copy | honest words for a changed path and for each move |
| 5 | `AdapterError::UninstallUnsafe`, `UninstallUnsafeReason`, the IPC kind, the dialog's wording | the six refusals reach the screen in the user's language |
| 6 | The path-list uninstall: the Trash seam, Claude Code's list, the one-hop launcher, `removal.rs`, the adapter, the row's words, the trust file and the README (one commit, stages 6a–6h) | Claude Code can be uninstalled from Canager, and the documents say so |
| 7 | End to end through `Session`; the `#[ignore]` `RealTrasher` smoke; the CI step; the smoke test's lines in the trust file and the README | the outcome the user sees, and the real call on every CI push |
| 8 | The backlog, the README's Language sections, the test counts | the documents that only lagged catch up |

Order: 1 → 2 → 3 → 4 → 5 → 6 → 7 → 8. Tasks 2–5 are independent of each other and could run in any order after 1; 6 needs 1–5; 7 needs 6; 8 is last.

## Review Focus

Eight inputs the spec implies, or its reviewers found, that a person is most likely to hit, most likely first. Each has its test in the task named.

1. **Two paths with the same basename** (`~/.local/share/claude` and `~/.local/bin/claude`) → both are moved and both are in the Trash as two items; `mv -n` would have skipped the second and reported success (Task 6 `test_execute_moves_every_listed_path_in_order_and_logs_each`; Task 7 `test_uninstalling_claude_code_moves_its_three_paths_and_keeps_its_settings`).
2. **Something changes between the preview and the click** — the launcher re-pointed at Homebrew's copy (a check fails), the updater creating `~/.claude/downloads` (the list grows), or Claude Code updating itself and re-pointing the launcher at a new version inside its root (every check still passes, but the link is not the one the preview saw) → `CanagerFailed(PathChanged)` naming that path, nothing moved, and the user previews again (Task 6 `test_execute_removal_refuses_when_a_check_now_fails`, `test_execute_removal_refuses_when_the_fresh_list_differs_from_the_preview`, `test_execute_removal_refuses_what_the_preview_did_not_see`, `test_execute_refuses_a_launcher_the_updater_re_pointed_after_the_preview`; Task 7 `test_a_path_changed_after_the_preview_stops_the_uninstall_before_it_moves_anything`, `test_a_self_update_between_the_preview_and_the_click_stops_the_uninstall_before_it_moves_anything`).
3. **Something changes during the pauses between moves** — the cache replaced by a folder of the same name, or `~/.claude` renamed out of the home folder and replaced by a link to where it went, the cache keeping its inode → that item's turn runs every check again and stops there; a swap inside the move itself is the documented edge (Task 6 `test_execute_removal_catches_a_substitution_before_an_items_check`, `test_execute_removal_checks_an_items_folders_again_after_the_pause`, `test_a_substitution_inside_the_move_itself_is_beyond_the_last_check`).
4. **macOS refuses an item after the first, or the user cancels between items** → the launcher stays, the outcome is `Failed`/`Cancelled`, the next refresh shows the launcher-only row with an Uninstall, and a second uninstall lists the program directory as already gone and finishes (a refusal of the first item, or a Cancel before it, moves nothing and leaves the ordinary row); a Cancel while a move is under way waits for that move (Task 2 `test_an_uninstall_is_verified_by_reconcile_after_uninstall_alone`; Task 6 `test_execute_stops_at_a_refused_item_and_leaves_the_launcher`, `test_execute_stops_between_items_when_cancelled`, `test_execute_removal_finishes_a_move_under_way_when_cancel_arrives`; Task 7 `test_an_uninstall_macos_refuses_partway_leaves_a_launcher_only_row_that_a_second_uninstall_finishes`, `test_an_uninstall_cancelled_between_items_is_reported_cancelled_and_a_second_uninstall_finishes`).
5. **A folder on the way that is a link** — `~/.local/share → /Volumes/Data/share` (`outside_home`), or `~/.claude → ~/Documents` and `~/.local/bin → ~/dotfiles/bin`, links that stay inside the home folder (`not_what_instructions_expect`), while a home folder that is itself reached through a link is accepted (Task 6 `test_plan_removal_refuses_a_parent_that_leads_outside_home`, `test_plan_removal_refuses_a_path_reached_through_a_linked_folder_inside_home`, `test_plan_removal_accepts_a_home_reached_through_a_symlink`).
6. **A kept path that leads into what would be moved** — `~/.claude → ~/.local/share/claude`, or a `~/.claude.json` that is a link into the program folder → `overlaps_kept` naming the kept path, at the preview and at the confirmation (Task 6 `test_plan_removal_refuses_when_a_kept_path_leads_into_what_it_would_move`, `test_execute_removal_refuses_a_settings_folder_linked_to_the_program_folder_after_the_preview`).
7. **The reading after an uninstall cannot see** — a permission error on the launcher's folder → `Unconfirmed`, never `Succeeded`; and a launcher that reaches its root through another link is no instance, so no uninstall can stop with that link dangling and read as finished (Task 2 `test_an_uninstall_whose_reading_cannot_tell_is_unconfirmed_never_succeeded`; Task 6 `test_probe_strict_says_it_cannot_tell_where_probe_says_absent`, `test_probe_refuses_a_launcher_that_reaches_the_root_through_a_link_outside_it`, `test_reconcile_after_uninstall_tells_there_gone_and_cannot_tell_apart`; Task 7 `test_an_uninstall_whose_last_reading_cannot_tell_is_unconfirmed_not_succeeded`).
8. **The real Trash call on the link kinds this step moves** — a dangling launcher (the last item of every uninstall, and the only item of a second one after a stop) and a link to a directory → the link itself lands in the Trash and its target stays put (Task 7 `test_real_trasher_moves_each_kind_of_item_and_links_as_links`, `#[ignore]` and `CANAGER_LIVE=1`, run once by the author on their Mac and on every CI push; a merge blocker).

(Ownership — check 3 — is not in the eight because it has its own test: Task 6 `test_plan_removal_refuses_a_path_the_user_does_not_own`, with an injected `euid`, since a test cannot make a file owned by someone else. Nor is check 1's never-list — a path directly in the home folder or in a folder many tools share — which only a recipe typo or an odd link can reach: Task 6 `test_plan_removal_refuses_a_path_whose_folder_is_home_or_shared` and `recipes::tests` pin it.)

---

### Task 1: `PlanAction` in place of `program`/`args`/`env`, everywhere

**Files:**
- Modify: `crates/banager-core/src/model.rs` — `Plan` and `test_plan_round_trips_through_json`  [B's file: anchor by symbol]
- Modify: `crates/banager-core/src/testing.rs:25` (the `use crate::model::{…}` line) and three helpers appended after `unavailable_instance` (ends `:82`)
- Modify: `crates/banager-core/src/adapters/mod.rs` — the `use crate::model::{…}` list, `run_plan`, the two test `Plan` literals, one test  [B's file: anchor by symbol]
- Modify: `crates/banager-core/src/ops/mod.rs:3-6` (imports), `:205` (`OpSummary.argv_preview`'s comment), `:284-285` (`summaries`)
- Modify: the 24 construction sites and the test assertions listed in Step 3
- Modify: `src/lib/types.ts`, `src/lib/types.test.ts`  [B's files]
- Modify: `src/components/CommandPreview.tsx`, `src/components/CommandPreview.test.tsx`
- Modify: `src/components/UninstallDialog.tsx` (the `<CommandPreview … />` element), `src/components/UninstallDialog.test.tsx` (`issuedPlanFor`, and the `plan_operation` reply in `ignores a submit that finishes after the dialog was retargeted`)
- Modify: `src/pages/UpdatesPage.tsx` (the `<CommandPreview … />` element), `src/pages/UpdatesPage.test.tsx` (`issuedPlanFor`)  [B's files]
- Modify: `src/lib/queries.test.ts:149-151`, `src/lib/api.test.ts:54-56`, `src/pages/InstalledPage.test.tsx` (two `plan_operation` fixtures)
- Modify: `src/i18n/en.json`, `src/i18n/zh-CN.json` — `uninstall.trashPreview`, `commandPreview.trashLabel`
- Test: `model.rs` shape tests, `adapters/mod.rs`'s `run_plan` test, `tests/ops_summaries_test.rs` (one new test), every existing Rust test (they compile against the new shape), `types.test.ts`, `CommandPreview.test.tsx`, `completeness.test.ts`.

**Interfaces:**
- Consumes: `Plan` and every adapter's `plan()`; `OpSummary.argv_preview` (`ops/mod.rs:205`); `CommandSpec` (`runner/mod.rs`); `displayToken` (`format.ts`).
- Produces (verbatim, from Core Interfaces): `PlanAction::{Command { program, args, env }, TrashPaths { paths }}` and `Plan.action`. Producers: every adapter's `plan()` builds `Command`; `StandaloneAdapter::plan` builds `TrashPaths` (Task 6). Production readers, each landing here: `run_plan` (`Command` only — refuses `TrashPaths`), `OperationManager::summaries` (empty `argv_preview` for `TrashPaths`), `CommandPreview.tsx` (one sentence for `TrashPaths`), the `types.ts` mirror. `testing::{command_program, command_args, command_env}` (readers: the test assertions below). `uninstall.trashPreview_one`/`_other` and `commandPreview.trashLabel` (reader: `CommandPreview.tsx`).

- [ ] **Step 1: Write the failing tests**

In `crates/banager-core/src/model.rs`, replace `test_plan_round_trips_through_json` (from its `#[test]` through its closing `}`) with:

```rust
    #[test]
    fn test_plan_round_trips_through_json() {
        let plan = Plan {
            request: OpRequest {
                kind: OpKind::Install,
                instance_id: "brew:/opt/homebrew".to_string(),
                artifact_kind: ArtifactKind::Formula,
                name: "jq".to_string(),
            },
            action: PlanAction::Command {
                program: PathBuf::from("/opt/homebrew/bin/brew"),
                args: vec![
                    "install".to_string(),
                    "--formula".to_string(),
                    "jq".to_string(),
                ],
                env: vec![("HOMEBREW_NO_AUTO_UPDATE".to_string(), "1".to_string())],
            },
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

        let trash = Plan {
            request: OpRequest {
                kind: OpKind::Uninstall,
                instance_id: "standalone-claude".to_string(),
                artifact_kind: ArtifactKind::Binary,
                name: "claude".to_string(),
            },
            action: PlanAction::TrashPaths {
                paths: vec![
                    PathBuf::from("/Users/someone/.local/share/claude"),
                    PathBuf::from("/Users/someone/.local/bin/claude"),
                ],
            },
            needs_password: false,
            locks: vec![ResourceLock("standalone-claude".to_string())],
            cancel_policy: CancelPolicy::KillThenReconcile,
            warnings: vec![],
            affected: vec![],
            timeout_secs: 120,
        };
        let json = serde_json::to_string(&trash).expect("serialize");
        let back: Plan = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(trash, back);
    }

    #[test]
    fn test_plan_action_is_externally_tagged_on_the_wire() {
        // `src/lib/types.ts` mirrors `PlanAction` as a union of two
        // single-key objects, and `CommandPreview.tsx` branches on
        // `"Command" in action`; the spellings below are the contract.
        assert_eq!(
            serde_json::to_string(&PlanAction::Command {
                program: PathBuf::from("/opt/homebrew/bin/brew"),
                args: vec!["install".to_string()],
                env: vec![("A".to_string(), "1".to_string())],
            })
            .unwrap(),
            r#"{"Command":{"program":"/opt/homebrew/bin/brew","args":["install"],"env":[["A","1"]]}}"#
        );
        assert_eq!(
            serde_json::to_string(&PlanAction::TrashPaths {
                paths: vec![PathBuf::from("/Users/someone/.local/bin/claude")],
            })
            .unwrap(),
            r#"{"TrashPaths":{"paths":["/Users/someone/.local/bin/claude"]}}"#
        );
    }
```

In `crates/banager-core/src/adapters/mod.rs`, inside `mod tests`, after the closing `}` of `test_run_plan_sends_a_runner_note_to_the_log_as_a_note_not_as_text` and before the line `use crate::model::ArtifactKind;` that precedes `fn installed(`, insert:

```rust

    #[tokio::test]
    async fn test_run_plan_refuses_a_plan_that_runs_no_command() {
        // A `TrashPaths` plan is carried out by `StandaloneAdapter::execute`
        // itself (adapters/standalone/removal.rs), never by a runner. Handing
        // one to `run_plan` is a bug in an adapter, refused before anything
        // could be spawned.
        use crate::events::VecSink;
        use crate::model::{CancelPolicy, OpKind, OpRequest, PlanAction, ResourceLock};
        use crate::runner::MockRunner;
        use std::path::PathBuf;
        use tokio_util::sync::CancellationToken;

        let plan = Plan {
            request: OpRequest {
                kind: OpKind::Uninstall,
                instance_id: "standalone-claude".to_string(),
                artifact_kind: ArtifactKind::Binary,
                name: "claude".to_string(),
            },
            action: PlanAction::TrashPaths {
                paths: vec![PathBuf::from("/Users/someone/.local/bin/claude")],
            },
            needs_password: false,
            locks: vec![ResourceLock("standalone-claude".to_string())],
            cancel_policy: CancelPolicy::KillThenReconcile,
            warnings: Vec::new(),
            affected: Vec::new(),
            timeout_secs: 120,
        };
        let runner_raw = Arc::new(MockRunner::new());
        let runner: Arc<dyn CommandRunner> = runner_raw.clone();
        let result = run_plan(
            &runner,
            &plan,
            Arc::new(VecSink::new()),
            3,
            CancellationToken::new(),
        )
        .await;
        assert!(matches!(result, Err(AdapterError::Refused(_))), "{result:?}");
        assert!(runner_raw.calls().is_empty(), "nothing was spawned");
    }
```

In `crates/banager-core/tests/ops_summaries_test.rs`, add `PlanAction` to the `use banager_core::model::{ … }` list (between `Plan,` and `Reconciled,`), add `use std::path::PathBuf;` after `use std::sync::Arc;`, and append at the end of the file:

```rust

#[tokio::test]
async fn test_summaries_gives_a_plan_that_runs_no_command_an_empty_argv_preview() {
    // A path-list uninstall (`PlanAction::TrashPaths`) spawns nothing, so
    // there is no argv to preview: an empty list, never an invented one.
    // (`src/` renders no `argv_preview` today; the uninstall dialog shows
    // the paths through the plan's `WillTrash` warnings instead.)
    let mut manager = OperationManager::new(Arc::new(VecSink::new()));
    let adapter = Arc::new(FakeAdapter::new());
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);
    let inst = make_instance("fake:1");
    manager.register_instance(inst.clone());

    let plan = Plan {
        request: OpRequest {
            kind: OpKind::Uninstall,
            instance_id: "fake:1".to_string(),
            artifact_kind: ArtifactKind::Binary,
            name: "claude".to_string(),
        },
        action: PlanAction::TrashPaths {
            paths: vec![PathBuf::from("/Users/someone/.local/bin/claude")],
        },
        needs_password: false,
        locks: vec![ResourceLock("fake:1".to_string())],
        cancel_policy: CancelPolicy::KillThenReconcile,
        warnings: vec![],
        affected: vec![],
        timeout_secs: 120,
    };
    let op_id = manager.submit(plan);
    manager.wait(op_id).await;

    let summary = manager.summaries().remove(0);
    assert_eq!(summary.id, op_id);
    assert_eq!(summary.kind, OpKind::Uninstall);
    assert!(summary.argv_preview.is_empty());
}
```

In `src/lib/types.test.ts`, add `PlanAction,` to the `import type { … } from "./types"` list (after `Plan,`); inside `it("round-trips a Plan, an OpSummary and Settings", …)`, replace the three lines

```ts
      program: "/opt/homebrew/bin/brew",
      args: ["install", "--formula", "jq"],
      env: [],
```

with

```ts
      action: {
        Command: { program: "/opt/homebrew/bin/brew", args: ["install", "--formula", "jq"], env: [] },
      },
```

and after that `it`'s closing `});` (before `it("spells the unknown-source scan's shapes as Rust sends them", …)`), insert:

```ts

  it("spells PlanAction as two externally tagged arms, as model.rs's shape test does", () => {
    // `test_plan_action_is_externally_tagged_on_the_wire` in
    // crates/banager-core/src/model.rs asserts these exact strings from the
    // Rust side. `CommandPreview.tsx` branches on `"Command" in action`.
    const command: PlanAction = {
      Command: { program: "/opt/homebrew/bin/brew", args: ["install"], env: [["A", "1"]] },
    };
    const trash: PlanAction = { TrashPaths: { paths: ["/Users/someone/.local/bin/claude"] } };
    expect(JSON.stringify(command)).toBe(
      '{"Command":{"program":"/opt/homebrew/bin/brew","args":["install"],"env":[["A","1"]]}}',
    );
    expect(JSON.stringify(trash)).toBe('{"TrashPaths":{"paths":["/Users/someone/.local/bin/claude"]}}');
    expect(roundTrip(command)).toEqual(command);
    expect(roundTrip(trash)).toEqual(trash);
  });
```

Replace the whole of `src/components/CommandPreview.test.tsx` with:

```tsx
import { describe, expect, it } from "vitest";
import { renderWithProviders } from "../test/setup";
import { CommandPreview } from "./CommandPreview";

describe("CommandPreview", () => {
  it("renders the joined program and arguments under a label", () => {
    const { getByText } = renderWithProviders(
      <CommandPreview
        action={{
          Command: { program: "/opt/homebrew/bin/brew", args: ["upgrade", "--cask", "onyx"], env: [] },
        }}
      />,
    );

    expect(getByText("This will run:")).toBeInTheDocument();
    expect(getByText("/opt/homebrew/bin/brew upgrade --cask onyx")).toBeInTheDocument();
  });

  it("quotes tokens that contain whitespace so argument boundaries stay visible", () => {
    const { getByText } = renderWithProviders(
      <CommandPreview
        action={{
          Command: { program: "/Users/Alice Smith/bin/brew", args: ["upgrade", "--cask", "onyx"], env: [] },
        }}
      />,
    );

    expect(getByText("'/Users/Alice Smith/bin/brew' upgrade --cask onyx")).toBeInTheDocument();
  });

  it("says Canager moves the listed items itself, counted, when the plan runs no command", () => {
    // A path-list uninstall (spec §6.2): no argv exists, so the honest
    // preview is a sentence -- what Canager will do, that no command
    // runs, and that nothing is deleted -- under a label of its own, never
    // "This will run:". The items themselves are the dialog's `WillTrash`
    // list above it. No <code>: there is nothing to copy into a terminal.
    const { getByText, queryByText, container } = renderWithProviders(
      <CommandPreview
        action={{
          TrashPaths: {
            paths: [
              "/Users/someone/.local/share/claude",
              "/Users/someone/.claude/downloads",
              "/Users/someone/.local/bin/claude",
            ],
          },
        }}
      />,
    );

    expect(getByText("What Canager will do:")).toBeInTheDocument();
    expect(queryByText("This will run:")).toBeNull();
    expect(
      getByText(
        "Canager moves the 3 items listed above to the Trash itself — no command runs, and nothing is deleted: until you empty the Trash you can drag them back out, and Finder's Put Back will likely work too.",
      ),
    ).toBeInTheDocument();
    expect(container.querySelector("code")).toBeNull();
  });

  it("uses the singular sentence for one item", () => {
    const { getByText } = renderWithProviders(
      <CommandPreview action={{ TrashPaths: { paths: ["/Users/someone/.local/bin/claude"] } }} />,
    );

    expect(
      getByText(
        "Canager moves the 1 item listed above to the Trash itself — no command runs, and nothing is deleted: until you empty the Trash you can drag it back out, and Finder's Put Back will likely work too.",
      ),
    ).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p banager-core --lib model::tests::test_plan_action_is_externally_tagged_on_the_wire`
Expected: FAIL to compile — `error[E0433]: failed to resolve: use of undeclared type \`PlanAction\`` and, in `test_plan_round_trips_through_json`, `error[E0560]: struct \`Plan\` has no field named \`action\``.

Run: `pnpm typecheck`
Expected: FAIL — `Module '"./types"' has no exported member 'PlanAction'` (`types.test.ts`); `Object literal may only specify known properties, and 'action' does not exist in type 'Plan'`; in `CommandPreview.test.tsx`, `Property 'program' is missing in type '{ action: … }'`.

- [ ] **Step 3: Move the three fields into `PlanAction`, at every site**

**3a. The type.** In `crates/banager-core/src/model.rs`, replace the `Plan` struct (its `#[derive(…)]` line through its closing `}`, which today reads `pub struct Plan { pub request: OpRequest, pub program: PathBuf, pub args: Vec<String>, // argv without program; preview = program + args` … `pub timeout_secs: u64, }`) with:

```rust
/// What a `Plan` does when it runs. Every plan an adapter built before
/// phase 4 was one program and one argv (`Command`), and `run_plan` is
/// still the only thing that spawns one. The path-list uninstall of a
/// tool installed by its own installer (`StandaloneAdapter`, phase 4
/// step C) runs no command at all: `execute` hands each path to the
/// system's "move to Trash" (`Trasher::trash`) in order -- `TrashPaths`.
/// Two arms rather than an invented argv, because a preview that names a
/// command that will not run is a lie about what is about to happen (spec
/// Q16), and because one `mv` cannot express two paths with the same
/// basename (`~/.local/bin/claude` and `~/.local/share/claude`: `mv -n`
/// skips the second and exits 0), and an item a rename puts in the Trash
/// gets no Finder "Put Back" record (spec §6.2; the Trash spike, which also
/// found that a rename does reach `~/.Trash` without Full Disk Access).
///
/// Readers, each matching both arms: `run_plan` (`adapters/mod.rs`;
/// `Command` only, it refuses the other), `OperationManager::summaries`'s
/// `argv_preview` (`ops/mod.rs`; empty for `TrashPaths`),
/// `CommandPreview.tsx` (one sentence for `TrashPaths`) and the
/// hand-written mirror in `src/lib/types.ts`. Externally tagged on the
/// wire like every other enum here: `{"Command":{"program":…,"args":[…],
/// "env":[…]}}` and `{"TrashPaths":{"paths":[…]}}`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PlanAction {
    /// One program, one argv, one environment: what `run_plan` spawns.
    /// `args` is the argv without the program; the preview is `program`
    /// followed by `args`.
    Command {
        program: PathBuf,
        args: Vec<String>,
        env: Vec<(String, String)>,
    },
    /// No command. `execute` moves each path to the Trash, in this order
    /// (the tool's launcher last, spec §6.2), after re-checking it. The
    /// paths are absolute; the same paths, `$HOME` abbreviated to `~`,
    /// are the plan's `Warning::WillTrash` items, which is what the dialog
    /// lists. Built only by `StandaloneAdapter::plan` for a recipe whose
    /// `uninstall` is `Uninstall::Paths`.
    TrashPaths { paths: Vec<PathBuf> },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Plan {
    pub request: OpRequest,
    pub action: PlanAction,
    pub needs_password: bool,
    pub locks: Vec<ResourceLock>,
    pub cancel_policy: CancelPolicy,
    pub warnings: Vec<Warning>,
    pub affected: Vec<String>, // dependents that would break on uninstall
    pub timeout_secs: u64,
}
```

**3b. The test helpers.** In `crates/banager-core/src/testing.rs`, change line 25,

```rust
use crate::model::{InstanceStatus, ManagerInstance, ReadOnlyReason, Scope, Unavailable};
use std::path::PathBuf;
```

to

```rust
use crate::model::{
    InstanceStatus, ManagerInstance, Plan, PlanAction, ReadOnlyReason, Scope, Unavailable,
};
use std::path::{Path, PathBuf};
```

and after the closing `}` of `unavailable_instance` (`:82`, before the doc comment of `expire_issued_plans`) insert:

```rust

/// The parts of a plan that runs a command, for a test that asserts what
/// argv an adapter built -- three accessors rather than a destructuring at
/// each of the ~40 assertions that read `plan.program`/`args`/`env` before
/// `PlanAction` existed. Each panics on a `TrashPaths` plan, saying so: a
/// test that expected a command and got a path list has found a bug, and
/// the message names it. Test-only by nature: a production reader matches
/// both arms (`run_plan`, `OperationManager::summaries`) and never calls
/// these.
fn command_parts(plan: &Plan) -> (&Path, &[String], &[(String, String)]) {
    match &plan.action {
        PlanAction::Command { program, args, env } => (program, args, env),
        PlanAction::TrashPaths { paths } => panic!(
            "this plan runs no command: it moves {} path(s) to the Trash",
            paths.len()
        ),
    }
}

/// The program a `Command` plan runs. See `command_parts`.
pub fn command_program(plan: &Plan) -> &Path {
    command_parts(plan).0
}

/// The argv (without the program) a `Command` plan runs. See `command_parts`.
pub fn command_args(plan: &Plan) -> &[String] {
    command_parts(plan).1
}

/// The environment a `Command` plan adds. See `command_parts`.
pub fn command_env(plan: &Plan) -> &[(String, String)] {
    command_parts(plan).2
}
```

**3c. The two production readers.** In `crates/banager-core/src/adapters/mod.rs`, add `PlanAction` to the `use crate::model::{…}` list at the top of the file (after `Plan,`; rustfmt re-wraps it), and in `run_plan` replace

```rust
    let spec = CommandSpec {
        program: plan.program.clone(),
        args: plan.args.clone(),
        env: plan.env.clone(),
```

with

```rust
    // The one thing this function spawns is a `Command`. A `TrashPaths`
    // plan is carried out by `StandaloneAdapter::execute` itself, item by
    // item (adapters/standalone/removal.rs); reaching here with one is a
    // bug in an adapter, refused before anything is started.
    let PlanAction::Command { program, args, env } = &plan.action else {
        return Err(AdapterError::Refused(
            "run_plan was handed a plan that runs no command (TrashPaths)".to_string(),
        ));
    };
    let spec = CommandSpec {
        program: program.clone(),
        args: args.clone(),
        env: env.clone(),
```

In `crates/banager-core/src/ops/mod.rs`, change the import at `:3-6`

```rust
use crate::model::{
    AdapterId, ArtifactKey, ArtifactKind, Attention, CancelPolicy, Fault, InstanceId,
    ManagerInstance, OpKind, OpStatus, Outcome, Plan, Reconciled, ResourceLock,
};
```

to

```rust
use crate::model::{
    AdapterId, ArtifactKey, ArtifactKind, Attention, CancelPolicy, Fault, InstanceId,
    ManagerInstance, OpKind, OpStatus, Outcome, Plan, PlanAction, Reconciled, ResourceLock,
};
```

change `:205` `    pub argv_preview: Vec<String>, // program followed by args` to `    pub argv_preview: Vec<String>, // program followed by args; empty for a plan that runs no command`, and in `summaries` replace `:284-285`

```rust
                let mut argv_preview = vec![r.plan.program.to_string_lossy().to_string()];
                argv_preview.extend(r.plan.args.iter().cloned());
```

with

```rust
                let argv_preview = match &r.plan.action {
                    PlanAction::Command { program, args, .. } => {
                        let mut argv = vec![program.to_string_lossy().to_string()];
                        argv.extend(args.iter().cloned());
                        argv
                    }
                    // No command runs, so there is no argv to preview: an
                    // empty list, never an invented one. (`src/` renders
                    // no argv_preview today; the dialog shows the paths
                    // through the plan's `WillTrash` warnings.)
                    PlanAction::TrashPaths { .. } => Vec::new(),
                };
```

**3d. The 24 construction sites.** The rule is mechanical and identical everywhere: the three consecutive fields `program: P, args: A, env: E,` become one field `action: PlanAction::Command { program: P, args: A, env: E },` — each expression kept exactly as it is, including the `program,`/`args,`/`env,` shorthands — and `PlanAction` is added to that file's `use crate::model::{…}` (or `use banager_core::model::{…}`) list in alphabetical position. The sites, with the exact three lines each has at `3b5117a`:

| File:line of `Plan {` | The three lines | Import list to extend |
|---|---|---|
| `adapters/npm.rs:382` | `program: inst.exe_path.clone(),` / `args,` / `env: self.env_vec(),` | `npm.rs:7` |
| `adapters/cargo.rs:331` | `program,` / `args,` / `env: Vec::new(),` | `cargo.rs:8` |
| `adapters/cargo.rs:344` | `program: inst.exe_path.clone(),` / `args: vec!["uninstall".to_string(), req.name.clone()],` / `env: Vec::new(),` | (same) |
| `adapters/pipx.rs:422` | `program: inst.exe_path.clone(),` / `args,` / `env: Vec::new(),` | `pipx.rs:8` |
| `adapters/uv.rs:266` | `program: inst.exe_path.clone(),` / `args,` / `env: Vec::new(),` | `uv.rs:7` |
| `adapters/ollama/mod.rs:536` | `program: inst.exe_path.clone(),` / `args,` / `env: Vec::new(),` | `ollama/mod.rs:9` |
| `adapters/brew/mod.rs:1208` | `program: inst.exe_path.clone(),` / `args: vec!["install".to_string(), flag.to_string(), req.name.clone()],` / `env,` | `brew/mod.rs:8` |
| `adapters/brew/mod.rs:1274` | `program: inst.exe_path.clone(),` / `args: vec!["uninstall".to_string(), flag.to_string(), req.name.clone()],` / `env: self.env_vec(),` | (same) |
| `adapters/brew/mod.rs:1297` | `program: inst.exe_path.clone(),` / `args: vec!["upgrade".to_string(), flag.to_string(), req.name.clone()],` / `env,` | (same) |
| `adapters/pip.rs:792` (test) | `program: inst.exe_path.clone(),` / `args: vec!["-m".to_string(), "pip".to_string()],` / `env: Vec::new(),` | the test module's `use crate::model::{OpKind, Warning};` at `pip.rs:467` → `use crate::model::{OpKind, PlanAction, Warning};` |
| `adapters/mod.rs`, test `plan_for` inside `test_run_plan_maps_a_cancelled_run_to_unconfirmed_and_a_failure_to_the_last_stderr_lines` | `program: PathBuf::from("/bin/fake"),` / `args: args.into_iter().map(\|a\| a.to_string()).collect(),` / `env: Vec::new(),` | covered by 3c's top-level import (`use super::*;`) |
| `adapters/mod.rs`, `let plan = Plan {` inside `test_run_plan_sends_a_runner_note_to_the_log_as_a_note_not_as_text` | `program: std::path::PathBuf::from("/bin/fake"),` / `args: Vec::new(),` / `env: Vec::new(),` | (same) |
| `session/test_support.rs:88` (`fake_plan`) | `program: inst.exe_path.clone(),` / `args: vec!["do".to_string(), req.name.clone()],` / `env: vec![],` | `test_support.rs:12` |
| `src-tauri/src/ipc.rs:677` (test `FakeAdapter::plan`) | `program: inst.exe_path.clone(),` / `args: vec!["do".to_string(), req.name.clone()],` / `env: vec![],` | the test module's `use banager_core::model::{…}` at `ipc.rs:599` |
| `tests/ops_fault_test.rs:85`, `ops_panic_test.rs:92`, `ops_lock_test.rs:74`, `ops_cancel_test.rs:131`, `ops_outcome_test.rs:98`, `ops_semaphore_test.rs:85` and `:283` | `program: inst.exe_path.clone(),` / `args: vec![],` / `env: vec![],` | each file's `use banager_core::model::{…}` (lines 15, 12, 4, 16, 15, 10) |
| `tests/ops_summaries_test.rs:78` | `program: inst.exe_path.clone(),` / `args: vec!["install".to_string(), req.name.clone()],` / `env: vec![],` | done in Step 1 |
| `model.rs` (test) | done in Step 1 | — |
| B's `StandaloneAdapter::plan`, `OpKind::Upgrade` arm (`crates/banager-core/src/adapters/standalone/mod.rs`) | `program: inst.exe_path.clone(),` (with its two comment lines `// The launcher, exactly as previewed: never a program` / `// the recipe could name (spec 附录 B).`) / `args: upgrade.args.iter().map(\|a\| a.to_string()).collect(),` / `env: Vec::new(),` (with `// Not the version read's environment: \`claude update\`` / `// must not be told to stop updating (spec §3.4).`) → `action: PlanAction::Command { program: inst.exe_path.clone(), args: upgrade.args.iter().map(\|a\| a.to_string()).collect(), env: Vec::new() },`, each comment kept above the field it explains, inside the braces | B's non-test `use crate::model::{…}` list in that file (it holds `OpKind, OpRequest, Outcome, Plan, …` since B's stage 8) → add `PlanAction` |

That is 23 `Plan {` literals at `3b5117a` (`grep -rnE '(^|[^A-Za-z_])Plan\s*\{' crates src-tauri --include='*.rs'`, minus `struct Plan` and the four `-> Plan {` signatures) plus B's one. The compiler is the completeness check: after 3a no `Plan` has a `program`, `args` or `env` field, so `cargo test --workspace --no-run` names every site still unconverted and builds once all 24 are done.

**3e. The test assertions that read the three fields.** Replace each expression per this table. Inside the crate the helpers are `crate::testing::…`; in `tests/` and `src-tauri` they are `banager_core::testing::…`:

| File:line(s) | Today | Becomes |
|---|---|---|
| `adapters/cargo.rs:697`, `:722`, `:747` (`assert_eq!(` / `plan.program,` / `PathBuf::from(…)` / `);`) | `plan.program,` | `command_program(&plan),` |
| `adapters/cargo.rs:700`, `:725`, `:750` | `assert_eq!(plan.args, vec![…]);` | `assert_eq!(command_args(&plan), vec![…]);` |
| `adapters/cargo.rs:897` | `assert_eq!(plan.program, binstall_path);` | `assert_eq!(command_program(&plan), binstall_path);` |
| `adapters/npm.rs:1138`, `:1156`, `:1171` | `assert_eq!(plan.args, vec![…]);` | `assert_eq!(command_args(&plan), vec![…]);` |
| `adapters/pipx.rs:911` | `assert_eq!(plan.args, expected);` | `assert_eq!(command_args(&plan), expected);` |
| `adapters/pipx.rs:1114` | `assert_eq!(issued.plan.args, vec!["upgrade", "cowsay"]);` | `assert_eq!(command_args(&issued.plan), vec!["upgrade", "cowsay"]);` |
| `adapters/uv.rs:532` | `assert_eq!(plan.args, expected);` | `assert_eq!(command_args(&plan), expected);` |
| `adapters/ollama/mod.rs:992`, `:1721`, `:1738` | `assert_eq!(plan.args, …);` | `assert_eq!(command_args(&plan), …);` |
| `adapters/brew/mod.rs:2300`, `:2321`, `:2347`, `:2416`, `:2508`, `:2523`, `:2541` | `assert_eq!(plan.args, vec![…]);` | `assert_eq!(command_args(&plan), vec![…]);` |
| `adapters/brew/mod.rs:2596` | `!plan.args.iter().any(\|arg\| arg.as_str() == forbidden),` | `!command_args(&plan).iter().any(\|arg\| arg.as_str() == forbidden),` |
| `adapters/brew/mod.rs:2598` | `plan.args` (the `{:?}` argument) | `command_args(&plan)` |
| `adapters/brew/mod.rs:2602` | `plan.args,` (first argument of `assert_eq!`) | `command_args(&plan),` |
| `adapters/brew/mod.rs:2910` | `assert!(plan.env.contains(&(` | `assert!(command_env(&plan).contains(&(` |
| `adapters/brew/mod.rs:2935`, `:2937` | `!plan.env.iter().any(\|(k, _)\| k == "SUDO_ASKPASS"),` / `plan.env` | `!command_env(&plan).iter().any(\|(k, _)\| k == "SUDO_ASKPASS"),` / `command_env(&plan)` |
| `session/plans.rs:498` | `assert_eq!(issued.plan.args, vec!["do".to_string(), "jq".to_string()]);` | `assert_eq!(crate::testing::command_args(&issued.plan), vec!["do".to_string(), "jq".to_string()]);` |
| `src-tauri/src/ipc.rs:1342` | the same line | `assert_eq!(banager_core::testing::command_args(&issued.plan), vec!["do".to_string(), "jq".to_string()]);` |
| `tests/brew_live.rs:112` | `install_plan.args,` | `banager_core::testing::command_args(&install_plan),` |
| `tests/brew_live.rs:165` | `uninstall_plan.args,` | `banager_core::testing::command_args(&uninstall_plan),` |
| B's `test_plan_upgrade_is_the_tools_own_update_command_without_the_version_env` in `standalone/mod.rs`'s `mod tests` | `assert_eq!(plan.program, layout.launcher);` / `assert_eq!(plan.args, vec!["update".to_string()]);` / `assert!(plan.env.is_empty(), "upgrade adds no environment override");` | `assert_eq!(command_program(&plan), layout.launcher);` / `assert_eq!(command_args(&plan), vec!["update".to_string()]);` / `assert!(command_env(&plan).is_empty(), "upgrade adds no environment override");` |

(`&[String] == Vec<&str>`, `&[String] == Vec<String>` and `&Path == PathBuf` all have `PartialEq` impls in std, so no assertion needs anything but the swap.) Imports for the helpers: add `use crate::testing::command_args;` right after `use super::*;` in `mod tests` of `npm.rs` (`:587`), `pipx.rs` (`:514`), `uv.rs` (`:358`) and `ollama/mod.rs` (`:628`); `use crate::testing::{command_args, command_program};` after `use super::*;` in `cargo.rs`'s `mod tests` (`:437`); `use crate::testing::{command_args, command_env};` after `use super::*;` in `brew/mod.rs`'s **`mod plan_execute_tests`** (`:2241` — every brew assertion above is in that module, none in `mod tests` at `:1437`); and `use crate::testing::{command_args, command_env, command_program};` beside the other `use` lines of B's `standalone/mod.rs` `mod tests`. `session/plans.rs`, `ipc.rs` and `brew_live.rs` use the qualified path in the table.

**3f. The TypeScript mirror and the preview.** In `src/lib/types.ts`, replace the `Plan` interface (`export interface Plan {` through its closing `}`) with:

```ts
/**
 * Mirrors `PlanAction` in crates/banager-core/src/model.rs: what a plan
 * does when it runs. Externally tagged single-key objects. `Command` is
 * one program and one argv, spawned by `run_plan`; `TrashPaths` is a
 * path-list uninstall of a tool installed by its own installer, which
 * Canager carries out itself by moving each path to the Trash (phase 4
 * step C) -- no argv exists, so `CommandPreview` shows a sentence for it.
 * `CommandPreview` branches on `"Command" in action` with a `never`
 * default, so a third arm fails `tsc` until it has a preview.
 */
export type PlanAction =
  | { Command: { program: string; args: string[]; env: [string, string][] } }
  | { TrashPaths: { paths: string[] } };
export interface Plan {
  request: OpRequest;
  action: PlanAction;
  needs_password: boolean;
  locks: string[];
  cancel_policy: CancelPolicy;
  warnings: Warning[];
  affected: string[];
  timeout_secs: number;
}
```

Replace the whole of `src/components/CommandPreview.tsx` with:

```tsx
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import { displayToken } from "../lib/format";
import type { PlanAction } from "../lib/types";

export interface CommandPreviewProps {
  action: PlanAction;
}

/**
 * What a Plan will do, exactly. For a `Command`: the argv under "This
 * will run:", one token per `displayToken`, since a plain `join(" ")`
 * cannot tell `/Users/Alice Smith/bin/brew` apart from a program called
 * `/Users/Alice` with an argument `Smith/bin/brew`; both destructive paths
 * (updates and uninstall) rely on this component as the operator's only
 * view of what is about to run. For a `TrashPaths` plan there is no
 * command to show: the honest preview is one sentence -- Canager moves the
 * listed items to the Trash itself, no command runs, nothing is deleted --
 * under a label of its own (`commandPreview.trashLabel`), never "This will
 * run:", and the items are the dialog's `WillTrash` warnings above it
 * (spec §6.2). Not in a <code> block: there is nothing to paste into a
 * terminal.
 */
export function CommandPreview({ action }: CommandPreviewProps) {
  const { t } = useTranslation();
  const { label, body } = preview(t, action);
  return (
    <div>
      <p className="text-xs font-medium uppercase text-[var(--color-muted)]">{label}</p>
      {body}
    </div>
  );
}

/** The label and the body for each arm; a third arm fails `tsc` here. */
function preview(t: TFunction, action: PlanAction): { label: string; body: ReactNode } {
  if ("Command" in action) {
    return {
      label: t("commandPreview.label"),
      body: (
        <code className="mt-1 block overflow-x-auto rounded-md bg-[var(--color-hover)] px-3 py-2 text-xs text-[var(--color-foreground)]">
          {[action.Command.program, ...action.Command.args].map(displayToken).join(" ")}
        </code>
      ),
    };
  }
  if ("TrashPaths" in action) {
    return {
      label: t("commandPreview.trashLabel"),
      body: (
        <p className="mt-1 rounded-md bg-[var(--color-hover)] px-3 py-2 text-sm text-[var(--color-foreground)]">
          {t("uninstall.trashPreview", { count: action.TrashPaths.paths.length })}
        </p>
      ),
    };
  }
  const unhandled: never = action;
  return unhandled;
}
```

In `src/components/UninstallDialog.tsx`, replace `<CommandPreview program={plan.program} args={plan.args} />` with `<CommandPreview action={plan.action} />`. In `src/pages/UpdatesPage.tsx`, replace `<CommandPreview program={item.issued.plan.program} args={item.issued.plan.args} />` with `<CommandPreview action={item.issued.plan.action} />`.

**3g. The TS fixtures.** In each of these, replace the three lines `program: "/opt/homebrew/bin/brew",` / `args: [...]` / `env: [],` with one `action: { Command: { program: "/opt/homebrew/bin/brew", args: [...], env: [] } },`, keeping the same `args` array: `src/components/UninstallDialog.test.tsx` (`issuedPlanFor`, args `["uninstall", "--formula", "jq"]`) — and, in the same file, the `plan_operation` reply of `it("ignores a submit that finishes after the dialog was retargeted", …)`, which spreads `issuedPlanFor()` and overrides the top-level `args` with the retargeted name: replace its `args: ["uninstall", "--formula", planned.name],` with `action: { Command: { program: "/opt/homebrew/bin/brew", args: ["uninstall", "--formula", planned.name], env: [] } },` (left as it is, the stray `args` would be ignored, the preview would still end in `jq`, and the test's `findByText("/opt/homebrew/bin/brew uninstall --formula yq")` would time out; `tsc` does not catch it, since that mock's reply is not checked against `Plan`); `src/lib/queries.test.ts:149-151` (`["uninstall", "--formula", "jq"]`); `src/lib/api.test.ts:54-56` (same); `src/pages/InstalledPage.test.tsx`, inside `it("opens the uninstall dialog and plans it when the row's primary button is clicked", …)` and `it("disables the dialog's confirm button when the plan reports dependents", …)` (same, in each `plan_operation` reply); `src/pages/UpdatesPage.test.tsx`, `issuedPlanFor` (`["upgrade", request.artifact_kind === "Cask" ? "--cask" : "--formula", request.name]`).

**3h. The copy.** In `src/i18n/en.json`, inside `"uninstall": { … }`, after `"affectedBlocksConfirm": "…"` add `,` and:

```json
    "trashPreview_one": "Canager moves the {{count}} item listed above to the Trash itself — no command runs, and nothing is deleted: until you empty the Trash you can drag it back out, and Finder's Put Back will likely work too.",
    "trashPreview_other": "Canager moves the {{count}} items listed above to the Trash itself — no command runs, and nothing is deleted: until you empty the Trash you can drag them back out, and Finder's Put Back will likely work too."
```

In `src/i18n/zh-CN.json`, the same position:

```json
    "trashPreview_other": "Canager 会自己把上面列出的 {{count}} 项移到废纸篓——不运行任何命令，也不删除任何东西：清空废纸篓之前都能把它们拖回来，访达的「放回原处」多半也能用。"
```

And the label above that sentence (Ruling 22): in `src/i18n/en.json`, inside `"commandPreview": { … }`, after `"label": "This will run:",` add `"trashLabel": "What Canager will do:",`; in `src/i18n/zh-CN.json`, after `"label": "将执行：",` add `"trashLabel": "将执行：",` — the label spec §6.6's dialog shows above this sentence, which in Chinese does not say that a command runs.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo fmt --all && cargo test --workspace` and `pnpm typecheck && pnpm test`
Expected: PASS — every existing test compiles against `action`; the three new Rust tests and the three new TS cases pass; `completeness.test.ts` passes because `uninstall.trashPreview` (it strips the plural suffix) and `commandPreview.trashLabel` are literals in `CommandPreview.tsx`.

- [ ] **Step 5: Run the gates**

Run all five from Global Constraints. Expected: all clean.

- [ ] **Step 6: Commit**

```bash
git add crates/banager-core/src/model.rs crates/banager-core/src/testing.rs crates/banager-core/src/adapters/mod.rs crates/banager-core/src/ops/mod.rs crates/banager-core/src/adapters/brew/mod.rs crates/banager-core/src/adapters/cargo.rs crates/banager-core/src/adapters/npm.rs crates/banager-core/src/adapters/pip.rs crates/banager-core/src/adapters/pipx.rs crates/banager-core/src/adapters/uv.rs crates/banager-core/src/adapters/ollama/mod.rs crates/banager-core/src/adapters/standalone/mod.rs crates/banager-core/src/session/test_support.rs crates/banager-core/src/session/plans.rs crates/banager-core/tests/ops_cancel_test.rs crates/banager-core/tests/ops_fault_test.rs crates/banager-core/tests/ops_lock_test.rs crates/banager-core/tests/ops_outcome_test.rs crates/banager-core/tests/ops_panic_test.rs crates/banager-core/tests/ops_semaphore_test.rs crates/banager-core/tests/ops_summaries_test.rs crates/banager-core/tests/brew_live.rs src-tauri/src/ipc.rs src/lib/types.ts src/lib/types.test.ts src/components/CommandPreview.tsx src/components/CommandPreview.test.tsx src/components/UninstallDialog.tsx src/components/UninstallDialog.test.tsx src/pages/UpdatesPage.tsx src/pages/UpdatesPage.test.tsx src/lib/queries.test.ts src/lib/api.test.ts src/pages/InstalledPage.test.tsx src/i18n/en.json src/i18n/zh-CN.json
git commit -m "$(cat <<'EOF'
Give a plan an action: one command, or a list of paths to move to the Trash

Every plan used to be one program and one argv. A path-list uninstall of
a tool installed by its own installer runs no command: Canager moves each
path to the Trash itself, and one mv could not express two paths with
the same basename anyway. Plan.program/args/env become
PlanAction::Command, beside a TrashPaths arm the standalone adapter's
uninstall will build; run_plan refuses that arm, the argv preview is
empty for it, and CommandPreview says in one sentence, under a label of
its own, what Canager will do instead of showing a command that will not
run.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: Operation-aware uninstall verification: `Adapter::reconcile_after_uninstall`

B's plan hands this to step C in its deviation 15: B's `StandaloneAdapter::reconcile` refuses (`AdapterError::Parse`) an owned launcher it cannot read a version from, because after an *upgrade* that exits 0 such a launcher is no evidence of success; but after an *uninstall* that same launcher — the launcher-only state a stopped path-list uninstall leaves — is exactly the evidence that the tool is still there. "Before enabling uninstall, make verification operation-aware so uninstall reads that presence even without a version, while upgrades retain this strict check." This task adds the operation-aware reading with a default that changes nothing for any existing adapter; Task 6 gives `StandaloneAdapter` its override.

**Files:**
- Modify: `crates/banager-core/src/adapters/mod.rs` — the `Adapter` trait, after `async fn reconcile(…) -> Result<Reconciled, AdapterError>;`  [B's file: anchor by symbol]
- Modify: `crates/banager-core/src/ops/mod.rs:668` (`let reconciled = adapter.reconcile(&instance, &key).await;` in `run_operation`)
- Modify: `crates/banager-core/tests/ops_outcome_test.rs` — a second fake adapter and its tests, appended at the end of the file
- Test: `tests/ops_outcome_test.rs`.

**Interfaces:**
- Consumes: `Adapter::reconcile`, `Reconciled`, `run_operation`'s `Ok(Outcome::Succeeded)` and `Ok(Outcome::Unconfirmed)` arms (`ops/mod.rs:670-802`); `PlanAction` (Task 1).
- Produces (verbatim): `async fn reconcile_after_uninstall(&self, inst: &ManagerInstance, key: &ArtifactKey) -> Result<Reconciled, AdapterError>` on `Adapter`, with a default body `self.reconcile(inst, key).await`. Production reader, landing here: `OperationManager::run_operation`, for `OpKind::Uninstall` only. Production override: `StandaloneAdapter` (Task 6, stage 6e).

- [ ] **Step 1: Write the failing tests**

Append to `crates/banager-core/tests/ops_outcome_test.rs`:

```rust

// --- The reading after an uninstall (phase 4 step C) -----------------------

/// An adapter whose `reconcile` never answers -- the way
/// `StandaloneAdapter`'s refuses a launcher it cannot read a version from
/// -- while its `reconcile_after_uninstall` answers presence, which is all
/// an uninstall's verification reads: `Some(present)`, or `None` for a
/// reading that cannot tell (an `Err`, as a permission error on the
/// launcher's folder is for the standalone adapter). `execute` reports
/// `outcome`, first firing the operation's own token when
/// `cancel_in_execute` is set: a user's Cancel landing while the uninstall
/// ran.
struct SplitReadingAdapter {
    meta: AdapterMeta,
    still_there: Option<bool>,
    outcome: Outcome,
    cancel_in_execute: bool,
    calls: Mutex<Vec<&'static str>>,
}

impl SplitReadingAdapter {
    fn new(
        still_there: Option<bool>,
        outcome: Outcome,
        cancel_in_execute: bool,
    ) -> SplitReadingAdapter {
        SplitReadingAdapter {
            meta: AdapterMeta {
                id: "fake".to_string(),
                name: "fake".to_string(),
                kind: "fake".to_string(),
                platforms: vec!["macos".to_string()],
                homepage: "https://example.invalid".to_string(),
                schema_version: 1,
                verified_versions: vec![],
            },
            still_there,
            outcome,
            cancel_in_execute,
            calls: Mutex::new(Vec::new()),
        }
    }
}

#[async_trait]
impl Adapter for SplitReadingAdapter {
    fn meta(&self) -> &AdapterMeta {
        &self.meta
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

    async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError> {
        Ok(Plan {
            request: req.clone(),
            action: PlanAction::Command {
                program: inst.exe_path.clone(),
                args: vec![],
                env: vec![],
            },
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
        self.calls.lock().unwrap().push("execute");
        if self.cancel_in_execute {
            cancel.cancel();
        }
        Ok(self.outcome.clone())
    }

    async fn reconcile(
        &self,
        _inst: &ManagerInstance,
        _key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        self.calls.lock().unwrap().push("reconcile");
        Err(AdapterError::Parse("no version to read".to_string()))
    }

    async fn reconcile_after_uninstall(
        &self,
        _inst: &ManagerInstance,
        _key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        self.calls.lock().unwrap().push("reconcile_after_uninstall");
        match self.still_there {
            Some(present) => Ok(Reconciled {
                present,
                version: None,
            }),
            None => Err(AdapterError::Parse(
                "cannot tell whether it is still there".to_string(),
            )),
        }
    }
}

/// Submits one op of `kind` to a fresh manager over `adapter` and returns
/// the outcome and the order the adapter was called in.
async fn run_split(kind: OpKind, adapter: SplitReadingAdapter) -> (Outcome, Vec<&'static str>) {
    let mut manager = OperationManager::new(Arc::new(VecSink::new()));
    let adapter = Arc::new(adapter);
    manager.register_adapter(adapter.clone());
    let manager = Arc::new(manager);
    let inst = make_instance("fake:/split-reading");
    manager.register_instance(inst.clone());
    let req = OpRequest {
        kind,
        instance_id: inst.id.clone(),
        artifact_kind: ArtifactKind::Binary,
        name: "tool".to_string(),
    };
    let plan = adapter.plan(&inst, &req).await.expect("plan");
    let op_id = manager.submit(plan);
    let outcome = manager.wait(op_id).await.expect("an outcome");
    let calls = adapter.calls.lock().unwrap().clone();
    (outcome, calls)
}

#[tokio::test]
async fn test_an_uninstall_is_verified_by_reconcile_after_uninstall_alone() {
    // After an uninstall the only question is "is it still there?". An
    // adapter whose version reading cannot answer must still be able to
    // say "yes, the launcher is there" -- otherwise a stopped uninstall
    // would read as `Unconfirmed` instead of what it is.
    let (outcome, calls) = run_split(
        OpKind::Uninstall,
        SplitReadingAdapter::new(Some(true), Outcome::Succeeded, false),
    )
    .await;
    assert_eq!(
        outcome,
        Outcome::NeedsAttention(Attention::StillInstalledAfterUninstall)
    );
    assert_eq!(calls, vec!["execute", "reconcile_after_uninstall"]);

    let (outcome, _) = run_split(
        OpKind::Uninstall,
        SplitReadingAdapter::new(Some(false), Outcome::Succeeded, false),
    )
    .await;
    assert_eq!(outcome, Outcome::Succeeded);

    // Stopped by the user's Cancel partway: still there means the cancel
    // is what happened; gone means the work finished anyway.
    let (outcome, _) = run_split(
        OpKind::Uninstall,
        SplitReadingAdapter::new(Some(true), Outcome::Unconfirmed, true),
    )
    .await;
    assert_eq!(outcome, Outcome::Cancelled);
    let (outcome, _) = run_split(
        OpKind::Uninstall,
        SplitReadingAdapter::new(Some(false), Outcome::Unconfirmed, true),
    )
    .await;
    assert_eq!(outcome, Outcome::Succeeded);
}

#[tokio::test]
async fn test_an_uninstall_whose_reading_cannot_tell_is_unconfirmed_never_succeeded() {
    // "Could not tell" is not "gone" (phase 4 step C, Astra finding 6): a
    // reading that fails -- a permission error hiding the launcher, say --
    // leaves an uninstall `Unconfirmed` whatever `execute` reported, and
    // whether or not the user pressed Cancel. Never `Succeeded`, never
    // `Cancelled`: either would claim to know what is on the disk.
    let (outcome, calls) = run_split(
        OpKind::Uninstall,
        SplitReadingAdapter::new(None, Outcome::Succeeded, false),
    )
    .await;
    assert_eq!(outcome, Outcome::Unconfirmed);
    assert_eq!(calls, vec!["execute", "reconcile_after_uninstall"]);

    let (outcome, _) = run_split(
        OpKind::Uninstall,
        SplitReadingAdapter::new(None, Outcome::Unconfirmed, true),
    )
    .await;
    assert_eq!(outcome, Outcome::Unconfirmed);
}

#[tokio::test]
async fn test_an_upgrade_and_an_install_keep_the_strict_reading() {
    // B's rule for a standalone upgrade stands: a reading that cannot say
    // what is installed makes an exit-0 upgrade `Unconfirmed`, never
    // success. The uninstall reading is never asked for them.
    let (outcome, calls) = run_split(
        OpKind::Upgrade,
        SplitReadingAdapter::new(Some(true), Outcome::Succeeded, false),
    )
    .await;
    assert_eq!(outcome, Outcome::Unconfirmed);
    assert_eq!(calls, vec!["reconcile", "execute", "reconcile"]);

    let (outcome, calls) = run_split(
        OpKind::Install,
        SplitReadingAdapter::new(Some(true), Outcome::Succeeded, false),
    )
    .await;
    assert_eq!(outcome, Outcome::Unconfirmed);
    assert_eq!(calls, vec!["execute", "reconcile"]);
}

#[tokio::test]
async fn test_an_adapter_that_does_not_override_it_verifies_an_uninstall_with_reconcile() {
    // The trait's default: every source but the standalone one keeps
    // verifying an uninstall exactly as before this method existed.
    let (outcome, calls) =
        run_case_with_calls(OpKind::Uninstall, ReconcileBehavior::Present(false)).await;
    assert_eq!(outcome, Outcome::Succeeded);
    assert_eq!(calls, vec!["execute", "reconcile"]);
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p banager-core --test ops_outcome_test`
Expected: FAIL to compile — `error[E0407]: method \`reconcile_after_uninstall\` is not a member of trait \`Adapter\``.

- [ ] **Step 3: Add the method and read it**

In `crates/banager-core/src/adapters/mod.rs`, inside `pub trait Adapter`, after

```rust
    async fn reconcile(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError>;
```

insert:

```rust
    /// The reading `run_operation` (ops/mod.rs) takes after an uninstall,
    /// which asks one question only: is the artifact still there
    /// (`Reconciled::present`)? Every other operation is verified with
    /// `reconcile`. Defaults to `reconcile`, which is what every source's
    /// uninstall was verified with before this existed.
    ///
    /// `StandaloneAdapter` overrides it (adapters/standalone/mod.rs): its
    /// `reconcile` refuses a launcher it cannot read a version from --
    /// after an upgrade that exits 0, such a launcher is no evidence of
    /// success -- while after an uninstall that launcher, the dangling
    /// link a stopped path-list uninstall leaves, is exactly the evidence
    /// that the tool is still there.
    ///
    /// An adapter that cannot tell -- a permission error where the
    /// launcher should be, say -- answers `Err`, never `present: false`:
    /// `run_operation` turns an `Err` into `Unconfirmed`, so "could not
    /// tell" is never reported as a finished uninstall.
    async fn reconcile_after_uninstall(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        self.reconcile(inst, key).await
    }
```

In `crates/banager-core/src/ops/mod.rs`, replace `:668`

```rust
        let reconciled = adapter.reconcile(&instance, &key).await;
```

with

```rust
        // After an uninstall only presence decides anything below, and an
        // adapter may answer that when it cannot answer what version is
        // installed (`Adapter::reconcile_after_uninstall`); everything
        // else keeps the full reading.
        let reconciled = match plan.request.kind {
            OpKind::Uninstall => adapter.reconcile_after_uninstall(&instance, &key).await,
            OpKind::Install | OpKind::Upgrade => adapter.reconcile(&instance, &key).await,
        };
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p banager-core --test ops_outcome_test` and `cargo test --workspace`
Expected: PASS — the four new tests and every existing one (no adapter implements the new method, so each keeps its current verification). `test_an_uninstall_whose_reading_cannot_tell_is_unconfirmed_never_succeeded` pins what `run_operation` already does with an `Err` reading, now that the uninstall's reading is a method an adapter can answer on its own.

- [ ] **Step 5: Run the gates**

Run all five from Global Constraints. Expected: all clean.

- [ ] **Step 6: Commit**

```bash
git add crates/banager-core/src/adapters/mod.rs crates/banager-core/src/ops/mod.rs crates/banager-core/tests/ops_outcome_test.rs
git commit -m "$(cat <<'EOF'
Verify an uninstall by asking only whether the item is still there

After an uninstall run_operation reads nothing but presence, yet it
asked each adapter for the full reading, version included. The
standalone adapter refuses that reading for a launcher whose program
files are gone, which is right after an upgrade and wrong after an
uninstall that stopped partway, where that launcher is the proof the
tool is still installed. The Adapter trait gains a reading for exactly
that question, defaulting to reconcile, so every existing source is
verified as before; the standalone adapter's override follows with its
uninstall. A reading that cannot tell is an error, which run_operation
reports as unconfirmed, never as a finished uninstall; a test pins it.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: `Warning::{WillTrash, WillKeep, AlreadyGone}`, `RemovedWhat`, `KeptWhat`, their copy

**Files:**
- Modify: `crates/banager-core/src/model.rs` — before `Warning`, inside `Warning`, `test_warning_wire_shapes_match_the_hand_written_ts_mirror`  [B's file: anchor by symbol]
- Modify: `src/lib/types.ts` — before `Warning`, the `Warning` union and its doc  [B's file]
- Modify: `src/lib/types.test.ts` — the import list; `spells Warning's bare-string variants as bare strings and WouldBreak/Message as externally tagged`  [B's file]
- Modify: `src/lib/warnings.ts` (`warningKey`, `warningArgs`; two `Record`s)  [A's file]
- Modify: `src/lib/warnings.test.ts` (`describe("warningKey")`, `describe("warningArgs")`)
- Modify: `src/components/UninstallDialog.test.tsx` — two tests after `pluralises WouldBreak's copy and interpolates every name`
- Modify: `src/i18n/en.json`, `src/i18n/zh-CN.json` — `warnings`
- Test: the Rust shape test, `types.test.ts`, `warnings.test.ts`, `UninstallDialog.test.tsx`, `completeness.test.ts`.

**Interfaces:**
- Consumes: `Warning` and its shape test; `warningKey`'s `never` defaults (A); `warningTexts` → the uninstall dialog's "Before you continue:" list.
- Produces (verbatim): `RemovedWhat::{Launcher, Program, Cache}`, `KeptWhat::{Settings, SettingsAndHistory}`, `Warning::{WillTrash { path, what }, WillKeep { path, what }, AlreadyGone { path }}`. Producer: `removal::plan_removal` (Task 6, stage 6c) — one `WillTrash` per path in execution order, one `WillKeep` per kept path that exists, one `AlreadyGone` per missing program path of a launcher-only row. Production readers, landing here: `warningKey`/`warningArgs` → `warningTexts` → `UninstallDialog`'s list. The TS mirrors; `REMOVED_WHAT_KEYS: Record<RemovedWhat, string>`, `KEPT_WHAT_KEYS: Record<KeptWhat, string>` (reader: `warningKey`); keys `warnings.willTrash.{Launcher,Program,Cache}`, `warnings.willKeep.{Settings,SettingsAndHistory}`, `warnings.alreadyGone`.

- [ ] **Step 1: Write the failing tests**

In `crates/banager-core/src/model.rs`, inside `test_warning_wire_shapes_match_the_hand_written_ts_mirror`, after the statement `let round_tripped: Warning = serde_json::from_str(r#"{"WouldBreak":{"names":["a","b"]}}"#).unwrap();` and its `assert_eq!(round_tripped, Warning::WouldBreak { … });`, before the test's closing `}`, insert:

```rust

        // Phase 4 step C: what a path-list uninstall moves, keeps, and
        // finds already gone. Struct variants carrying a unit enum,
        // spelled as `REMOVED_WHAT_KEYS`/`KEPT_WHAT_KEYS` in
        // src/lib/warnings.ts index them.
        assert_eq!(
            serde_json::to_string(&Warning::WillTrash {
                path: "~/.local/bin/claude".to_string(),
                what: RemovedWhat::Launcher,
            })
            .unwrap(),
            r#"{"WillTrash":{"path":"~/.local/bin/claude","what":"Launcher"}}"#
        );
        assert_eq!(
            serde_json::to_string(&Warning::WillKeep {
                path: "~/.claude".to_string(),
                what: KeptWhat::SettingsAndHistory,
            })
            .unwrap(),
            r#"{"WillKeep":{"path":"~/.claude","what":"SettingsAndHistory"}}"#
        );
        assert_eq!(
            serde_json::to_string(&Warning::AlreadyGone {
                path: "~/.local/share/claude".to_string(),
            })
            .unwrap(),
            r#"{"AlreadyGone":{"path":"~/.local/share/claude"}}"#
        );
        for what in [RemovedWhat::Launcher, RemovedWhat::Program, RemovedWhat::Cache] {
            assert_eq!(serde_json::to_string(&what).unwrap(), format!("\"{what:?}\""));
        }
        for what in [KeptWhat::Settings, KeptWhat::SettingsAndHistory] {
            assert_eq!(serde_json::to_string(&what).unwrap(), format!("\"{what:?}\""));
        }
```

In `src/lib/types.test.ts`, add `RemovedWhat,` and `KeptWhat,` to the `import type { … } from "./types"` list (after `Warning,`), and inside `it("spells Warning's bare-string variants as bare strings and WouldBreak/Message as externally tagged", …)`, after `expect(roundTrip(message)).toEqual({ Message: "boom" });` and before the `});` closing the `it`, insert:

```ts

    // Phase 4 step C: the three struct variants a path-list uninstall
    // carries, and the two nested unit enums, spelled as
    // `test_warning_wire_shapes_match_the_hand_written_ts_mirror` in
    // crates/banager-core/src/model.rs asserts serde emits them.
    const willTrash: Warning = { WillTrash: { path: "~/.local/bin/claude", what: "Launcher" } };
    const willKeep: Warning = { WillKeep: { path: "~/.claude", what: "SettingsAndHistory" } };
    const alreadyGone: Warning = { AlreadyGone: { path: "~/.local/share/claude" } };
    expect(JSON.stringify(willTrash)).toBe(
      '{"WillTrash":{"path":"~/.local/bin/claude","what":"Launcher"}}',
    );
    expect(JSON.stringify(willKeep)).toBe(
      '{"WillKeep":{"path":"~/.claude","what":"SettingsAndHistory"}}',
    );
    expect(JSON.stringify(alreadyGone)).toBe('{"AlreadyGone":{"path":"~/.local/share/claude"}}');
    expect(roundTrip(willTrash)).toEqual(willTrash);
    const removed: RemovedWhat[] = ["Launcher", "Program", "Cache"];
    const kept: KeptWhat[] = ["Settings", "SettingsAndHistory"];
    expect(JSON.stringify(removed)).toBe('["Launcher","Program","Cache"]');
    expect(JSON.stringify(kept)).toBe('["Settings","SettingsAndHistory"]');
```

In `src/lib/warnings.test.ts`, inside `describe("warningKey", …)`, in `it("gives each fixed warning its own key", …)`, after the last `expect` (the one whose value is `"warnings.thirdPartyRegistry"`, ending `);`) insert:

```ts
    // A path-list uninstall's items: the key is chosen by what the path
    // is, so each kind can have its own parenthesis.
    expect(warningKey({ WillTrash: { path: "~/.local/bin/claude", what: "Launcher" } })).toBe(
      "warnings.willTrash.Launcher",
    );
    expect(warningKey({ WillTrash: { path: "~/.local/share/claude", what: "Program" } })).toBe(
      "warnings.willTrash.Program",
    );
    expect(warningKey({ WillTrash: { path: "~/.claude/downloads", what: "Cache" } })).toBe(
      "warnings.willTrash.Cache",
    );
    expect(warningKey({ WillKeep: { path: "~/.claude.json", what: "Settings" } })).toBe(
      "warnings.willKeep.Settings",
    );
    expect(warningKey({ WillKeep: { path: "~/.claude", what: "SettingsAndHistory" } })).toBe(
      "warnings.willKeep.SettingsAndHistory",
    );
    expect(warningKey({ AlreadyGone: { path: "~/.local/share/claude" } })).toBe(
      "warnings.alreadyGone",
    );
```

In `it("is null for Message and for nothing else", …)`, change the comment's "every variant of `Warning` is one of these six" to "every variant of `Warning` is one of these nine", and replace the `all` array with:

```ts
    const all: Warning[] = [
      "DependentsUnknown",
      "CompilesLocally",
      "NonRegistrySource",
      { WouldBreak: { names: ["a"] } },
      { ThirdPartyRegistry: { host: "modelscope.cn" } },
      { WillTrash: { path: "~/.local/bin/claude", what: "Launcher" } },
      { WillKeep: { path: "~/.claude", what: "SettingsAndHistory" } },
      { AlreadyGone: { path: "~/.local/share/claude" } },
      { Message: "boom" },
    ];
```

Inside `describe("warningArgs", …)`, before `it("is empty for every other variant", …)`, insert:

```ts
  it("interpolates the path a trash, keep or already-gone item names", () => {
    // The path arrives with `$HOME` already abbreviated to `~` on the Rust
    // side (`scan::display_path`): data for the sentence, not a path to
    // act on.
    expect(warningArgs({ WillTrash: { path: "~/.local/bin/claude", what: "Launcher" } })).toEqual({
      path: "~/.local/bin/claude",
    });
    expect(warningArgs({ WillKeep: { path: "~/.claude", what: "SettingsAndHistory" } })).toEqual({
      path: "~/.claude",
    });
    expect(warningArgs({ AlreadyGone: { path: "~/.local/share/claude" } })).toEqual({
      path: "~/.local/share/claude",
    });
  });

```

In `src/components/UninstallDialog.test.tsx`, after the closing `});` of `it("pluralises WouldBreak's copy and interpolates every name", …)` and before `it("renders a warning variant the mirror lacks as its raw key rather than dropping it", …)`, insert:

```tsx
  it("lists what a path-list uninstall moves, keeps and finds gone, and says Canager does the moving", async () => {
    // Spec §6.6, the Claude Code dialog: every item is a sentence in the
    // user's language, in the order the paths will be moved, the kept
    // paths after them; the preview below is one sentence with the
    // count, since no command runs.
    const claudeRequest: OpRequest = {
      ...request,
      instance_id: "standalone-claude",
      artifact_kind: "Binary",
      name: "claude",
    };
    vi.mocked(invoke).mockResolvedValue(
      issuedPlanFor({
        request: claudeRequest,
        action: {
          TrashPaths: {
            paths: [
              "/Users/someone/.local/share/claude",
              "/Users/someone/.claude/downloads",
              "/Users/someone/.local/bin/claude",
            ],
          },
        },
        warnings: [
          { WillTrash: { path: "~/.local/share/claude", what: "Program" } },
          { WillTrash: { path: "~/.claude/downloads", what: "Cache" } },
          { WillTrash: { path: "~/.local/bin/claude", what: "Launcher" } },
          { WillKeep: { path: "~/.claude", what: "SettingsAndHistory" } },
          { WillKeep: { path: "~/.claude.json", what: "Settings" } },
        ],
        locks: ["standalone-claude"],
        timeout_secs: 120,
      }),
    );

    renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={claudeRequest} displayName="Claude Code" />,
    );

    expect(await screen.findByText("Before you continue:")).toBeInTheDocument();
    const items = screen.getAllByRole("listitem").map((li) => li.textContent);
    expect(items).toEqual([
      "Moves to the Trash: ~/.local/share/claude (the program's files)",
      "Moves to the Trash: ~/.claude/downloads (downloaded files it can re-create)",
      "Moves to the Trash: ~/.local/bin/claude (the command itself)",
      "Keeps: ~/.claude (your settings, login, history and working files — other apps may use it too)",
      "Keeps: ~/.claude.json (your settings)",
    ]);
    expect(
      screen.getByText(
        "Canager moves the 3 items listed above to the Trash itself — no command runs, and nothing is deleted: until you empty the Trash you can drag them back out, and Finder's Put Back will likely work too.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Uninstall" })).toBeEnabled();
  });

  it("says a program directory an earlier uninstall already moved is already gone", async () => {
    // The launcher-only row's second uninstall (spec §6.3 check 2): the
    // list adds up -- one item to move, one already in the Trash.
    vi.mocked(invoke).mockResolvedValue(
      issuedPlanFor({
        action: { TrashPaths: { paths: ["/Users/someone/.local/bin/claude"] } },
        warnings: [
          { AlreadyGone: { path: "~/.local/share/claude" } },
          { WillTrash: { path: "~/.local/bin/claude", what: "Launcher" } },
        ],
      }),
    );

    renderWithProviders(
      <UninstallDialog open onOpenChange={() => {}} request={request} displayName="Claude Code" />,
    );

    expect(
      await screen.findByText("Already gone: ~/.local/share/claude (nothing left to move)"),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Canager moves the 1 item listed above to the Trash itself — no command runs, and nothing is deleted: until you empty the Trash you can drag it back out, and Finder's Put Back will likely work too.",
      ),
    ).toBeInTheDocument();
  });

```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p banager-core --lib model::tests::test_warning_wire_shapes_match_the_hand_written_ts_mirror`
Expected: FAIL to compile — `no variant named \`WillTrash\` found for enum \`Warning\`` (and `WillKeep`, `AlreadyGone`); `cannot find type \`RemovedWhat\``, `\`KeptWhat\``.

Run: `pnpm typecheck`
Expected: FAIL — `Module '"./types"' has no exported member 'RemovedWhat'`; in `warnings.test.ts` and `UninstallDialog.test.tsx`, `Object literal may only specify known properties, and 'WillTrash' does not exist in type 'Warning'`.

- [ ] **Step 3: Add the variants, the mirror, the branches and the copy**

In `crates/banager-core/src/model.rs`, directly above the doc comment that begins `/// A specific warning \`Plan\` or \`UpdateCandidate\` carries` (the `Warning` enum's), insert:

```rust
/// What one path a path-list uninstall moves to the Trash is, for the
/// sentence that lists it. Payload of `Warning::WillTrash`; produced by
/// `removal::plan_removal` from the recipe's `RemoveSpec.what`, read by
/// `REMOVED_WHAT_KEYS` in src/lib/warnings.ts, a `Record` over the
/// mirror, so a variant added here without copy fails `tsc`. Only the
/// kinds Claude Code's list produces exist in this step; `Backups`
/// (Antigravity's `agy.<time>.old`) arrives with step D.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemovedWhat {
    /// The launcher: the command itself (`~/.local/bin/claude`).
    Launcher,
    /// The program's files (`~/.local/share/claude`).
    Program,
    /// Downloaded files the tool re-creates (`~/.claude/downloads`).
    Cache,
}

/// What one path a path-list uninstall leaves where it is, for the
/// sentence that lists it. Payload of `Warning::WillKeep`; produced by
/// `removal::plan_removal` from the recipe's `KeepSpec.what`, read by
/// `KEPT_WHAT_KEYS` in src/lib/warnings.ts. `ToolState`,
/// `ShellConfigLines`, `OutsideHome` and `NotOurs` arrive with the
/// recipes that produce them (agy and grok, step D).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeptWhat {
    /// A settings file (`~/.claude.json`).
    Settings,
    /// Settings, login, history and working files, shared with other
    /// apps (`~/.claude`, which the tool's editor extensions and desktop
    /// app use too).
    SettingsAndHistory,
}

```

and inside `Warning`, after the `ThirdPartyRegistry { host: String },` variant and before `/// Not yet localised -- see this type's doc comment.`, insert:

```rust
    /// A path-list uninstall will move this to the Trash: one per path, in
    /// the order they will be moved (the launcher last). `path` has `$HOME`
    /// abbreviated to `~` (`scan::display_path`): data for a sentence, not
    /// a path to act on -- the plan's `PlanAction::TrashPaths.paths` keep
    /// the absolute ones. Produced by `removal::plan_removal`
    /// (`StandaloneAdapter::plan`); read by `warningKey`/`warningArgs` in
    /// src/lib/warnings.ts for the uninstall dialog's list.
    WillTrash { path: String, what: RemovedWhat },
    /// A path-list uninstall will leave this where it is. Same producer and
    /// reader as `WillTrash`; listed only when the path exists.
    WillKeep { path: String, what: KeptWhat },
    /// A path the list names is already gone -- an earlier uninstall
    /// stopped after moving it and before moving the launcher (the
    /// launcher-only state) -- so there is nothing to move there. Said, so
    /// the list adds up. Same producer and reader as `WillTrash`.
    AlreadyGone { path: String },
```

In `src/lib/types.ts`, directly above the doc comment of `Warning` (`/**` followed by ` * A specific warning \`Plan\` or \`UpdateCandidate\` carries. …`), insert:

```ts
/**
 * What one path a path-list uninstall moves to the Trash is. Mirrors
 * `RemovedWhat` in crates/banager-core/src/model.rs: bare-string unit
 * variants, the payload of `Warning.WillTrash`. Read through
 * `REMOVED_WHAT_KEYS` in src/lib/warnings.ts, a `Record` over this union,
 * so a variant added here without copy fails `tsc`.
 */
export type RemovedWhat = "Launcher" | "Program" | "Cache";
/**
 * What one path a path-list uninstall leaves alone is. Mirrors `KeptWhat`;
 * read through `KEPT_WHAT_KEYS` in src/lib/warnings.ts.
 */
export type KeptWhat = "Settings" | "SettingsAndHistory";
```

replace the `Warning` union with:

```ts
export type Warning =
  | "DependentsUnknown"
  | { WouldBreak: { names: string[] } }
  | "CompilesLocally"
  | "NonRegistrySource"
  | { ThirdPartyRegistry: { host: string } }
  | { WillTrash: { path: string; what: RemovedWhat } }
  | { WillKeep: { path: string; what: KeptWhat } }
  | { AlreadyGone: { path: string } }
  | { Message: string };
```

and in its doc comment replace `\`ThirdPartyRegistry\`, whose \`host\` interpolates it), and a \`Message\`` with `\`ThirdPartyRegistry\`, whose \`host\` interpolates it, and a path-list uninstall's \`WillTrash\`, \`WillKeep\` and \`AlreadyGone\`, whose \`path\` interpolates it and whose \`what\` picks the key), and a \`Message\``.

In `src/lib/warnings.ts`, change `import type { Warning } from "./types";` to `import type { KeptWhat, RemovedWhat, Warning } from "./types";`, insert before `warningKey`'s doc comment:

```ts
/** The sentence for each kind of path a path-list uninstall moves; a
 *  `Record` over `RemovedWhat`, so a kind without copy fails `tsc`. */
const REMOVED_WHAT_KEYS: Record<RemovedWhat, string> = {
  Launcher: "warnings.willTrash.Launcher",
  Program: "warnings.willTrash.Program",
  Cache: "warnings.willTrash.Cache",
};

/** The sentence for each kind of path a path-list uninstall keeps. */
const KEPT_WHAT_KEYS: Record<KeptWhat, string> = {
  Settings: "warnings.willKeep.Settings",
  SettingsAndHistory: "warnings.willKeep.SettingsAndHistory",
};

```

in `warningKey`, after `if ("ThirdPartyRegistry" in warning) return "warnings.thirdPartyRegistry";` insert:

```ts
  if ("WillTrash" in warning) return REMOVED_WHAT_KEYS[warning.WillTrash.what];
  if ("WillKeep" in warning) return KEPT_WHAT_KEYS[warning.WillKeep.what];
  if ("AlreadyGone" in warning) return "warnings.alreadyGone";
```

and in `warningArgs`, after `if ("ThirdPartyRegistry" in warning) return { host: warning.ThirdPartyRegistry.host };` insert:

```ts
  if ("WillTrash" in warning) return { path: warning.WillTrash.path };
  if ("WillKeep" in warning) return { path: warning.WillKeep.path };
  if ("AlreadyGone" in warning) return { path: warning.AlreadyGone.path };
```

In `src/i18n/en.json`, inside `"warnings": { … }`, after `"thirdPartyRegistry": "…"` add `,` and:

```json
    "willTrash": {
      "Launcher": "Moves to the Trash: {{path}} (the command itself)",
      "Program": "Moves to the Trash: {{path}} (the program's files)",
      "Cache": "Moves to the Trash: {{path}} (downloaded files it can re-create)"
    },
    "willKeep": {
      "Settings": "Keeps: {{path}} (your settings)",
      "SettingsAndHistory": "Keeps: {{path}} (your settings, login, history and working files — other apps may use it too)"
    },
    "alreadyGone": "Already gone: {{path}} (nothing left to move)"
```

In `src/i18n/zh-CN.json`, the same position:

```json
    "willTrash": {
      "Launcher": "移到废纸篓：{{path}}（命令本身）",
      "Program": "移到废纸篓：{{path}}（程序文件）",
      "Cache": "移到废纸篓：{{path}}（可重新下载的缓存）"
    },
    "willKeep": {
      "Settings": "保留：{{path}}（你的设置）",
      "SettingsAndHistory": "保留：{{path}}（你的设置、登录信息、历史记录和工作文件，其它应用也可能在用）"
    },
    "alreadyGone": "已经不在了：{{path}}（没有东西要移）"
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p banager-core --lib model::tests` and `pnpm typecheck && pnpm exec vitest run src/lib src/components/UninstallDialog.test.tsx src/i18n`
Expected: PASS. `completeness.test.ts` passes because every new key is a string literal in `warnings.ts`.

- [ ] **Step 5: Run the gates**

Run all five from Global Constraints. Expected: all clean (a `pub` enum variant whose producer lands in Task 6 is not a dead-code warning; its reader, `warningKey`, is here).

- [ ] **Step 6: Commit**

```bash
git add crates/banager-core/src/model.rs src/lib/types.ts src/lib/types.test.ts src/lib/warnings.ts src/lib/warnings.test.ts src/components/UninstallDialog.test.tsx src/i18n/en.json src/i18n/zh-CN.json
git commit -m "$(cat <<'EOF'
Add the warnings a path-list uninstall lists: moved, kept, already gone

The dialog's item list is the only preview a no-command uninstall has,
so each path it will move gets a sentence saying what it is (the
command, the program's files, a re-creatable cache), each path it keeps
says why it matters, and a program directory an earlier, stopped
uninstall already moved is listed as already gone so the list adds up.
The path arrives with the home folder abbreviated; the sentence is
chosen by the kind, through a Record the compiler holds complete. The
producer is the standalone adapter's uninstall plan, which follows.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: `Fault::PathChanged`, `LogNote::{MovedToTrash, TrashFailed}`, their copy

**Files:**
- Modify: `crates/banager-core/src/model.rs` — inside `Fault`; `test_canager_failed_is_externally_tagged_on_the_wire`  [B's file: anchor by symbol]
- Modify: `crates/banager-core/src/events.rs:26-42` (`LogNote`), `:115-139` (`test_note_wire_shape_is_what_the_typescript_mirror_expects`)
- Modify: `src/lib/types.ts` — the `Fault` and `LogNote` unions  [B's file]
- Modify: `src/lib/types.test.ts` — `keeps Outcome's externally tagged variants intact on the wire`, `keeps OperationEvent and UiEvent wire shapes intact`  [B's file]
- Modify: `src/lib/format.ts` — `faultKey`, `faultArgs`  [F's file: anchor by symbol]
- Modify: `src/lib/format.test.ts` — the `faults` array in `describe("outcomeKey for Canager's own failures")` and `passes a fault's data, never a sentence, to its translation`
- Modify: `src/components/LogDrawer.tsx` — `noteText`; `src/components/LogDrawer.test.tsx` — one test after `says which stream a failed read cut short`
- Modify: `src/i18n/completeness.test.ts` — `INTERPOLATED_SUBTREES["operations.outcome"]`  [anchor: the `"CanagerFailed.HomebrewStillUpdating",` line]
- Modify: `src/i18n/en.json`, `src/i18n/zh-CN.json` — `operations.outcome.CanagerFailed.PathChanged`, `operations.logNote.movedToTrash`, `operations.logNote.trashFailed`
- Test: the two Rust shape tests, `types.test.ts`, `format.test.ts`, `LogDrawer.test.tsx`, `completeness.test.ts`.

**Interfaces:**
- Consumes: `Fault`/`Outcome::CanagerFailed` (`model.rs`), `LogNote`/`OperationEvent::Note` (`events.rs`), `faultKey`/`faultArgs` → `outcomeKey`/`outcomeArgs` (`format.ts`) → `OperationBar.tsx` and `LogDrawer.tsx`'s outcome line; `noteText` (`LogDrawer.tsx`); `INTERPOLATED_SUBTREES` (`completeness.test.ts`).
- Produces (verbatim): `Fault::PathChanged { path: String }` (producer: `removal::execute_removal`, Task 6, stage 6d, when a path is not what the preview saw, `$HOME` abbreviated; readers landing here: `faultKey`/`faultArgs`, the `types.ts` mirror, `INTERPOLATED_SUBTREES`); `LogNote::MovedToTrash { path: String, trashed_to: String }` and `LogNote::TrashFailed { path: String, error: String }` (producer: `execute_removal`, one per item; reader landing here: `noteText`); keys `operations.outcome.CanagerFailed.PathChanged`, `operations.logNote.movedToTrash`, `operations.logNote.trashFailed`.

- [ ] **Step 1: Write the failing tests**

In `crates/banager-core/src/model.rs`, inside `test_canager_failed_is_externally_tagged_on_the_wire`, after the `assert_eq!` whose expected string is `r#"{"CanagerFailed":{"HomebrewStillUpdating":{"minutes":10}}}"#` (and its closing `);`), insert:

```rust
        // Phase 4 step C: a path-list uninstall found a path changed
        // between the preview and the run. `path` has `$HOME` abbreviated.
        assert_eq!(
            serde_json::to_string(&Outcome::CanagerFailed(Fault::PathChanged {
                path: "~/.local/bin/claude".to_string()
            }))
            .unwrap(),
            r#"{"CanagerFailed":{"PathChanged":{"path":"~/.local/bin/claude"}}}"#
        );
```

In `crates/banager-core/src/events.rs`, inside `test_note_wire_shape_is_what_the_typescript_mirror_expects`, after the `assert_eq!` on `failed` (its expected string ends `"error":"Input/output error (os error 5)"}}}}"#`) and its closing `);`, insert:

```rust
        // Phase 4 step C: the two lines a path-list uninstall writes, one
        // per path moved and one for a path macOS refused. Data only (a
        // path with `$HOME` abbreviated, the Trash location, the system's
        // own words); `LogDrawer.tsx` words them.
        let moved = OperationEvent::Note {
            op_id: 7,
            note: LogNote::MovedToTrash {
                path: "~/.local/share/claude".to_string(),
                trashed_to: "~/.Trash/claude".to_string(),
            },
        };
        assert_eq!(
            serde_json::to_string(&moved).unwrap(),
            r#"{"Note":{"op_id":7,"note":{"MovedToTrash":{"path":"~/.local/share/claude","trashed_to":"~/.Trash/claude"}}}}"#
        );
        let refused = OperationEvent::Note {
            op_id: 7,
            note: LogNote::TrashFailed {
                path: "~/.local/bin/claude".to_string(),
                error: "Operation not permitted".to_string(),
            },
        };
        assert_eq!(
            serde_json::to_string(&refused).unwrap(),
            r#"{"Note":{"op_id":7,"note":{"TrashFailed":{"path":"~/.local/bin/claude","error":"Operation not permitted"}}}}"#
        );
```

In `src/lib/types.test.ts`, inside `it("keeps Outcome's externally tagged variants intact on the wire", …)`, after `expect(roundTrip(missing)).toEqual(missing);` and before the `});` that closes the `it`, insert:

```ts
    // Phase 4 step C: a path changed between the preview and the run.
    const changed: Outcome = { CanagerFailed: { PathChanged: { path: "~/.local/bin/claude" } } };
    expect(JSON.stringify(changed)).toBe(
      '{"CanagerFailed":{"PathChanged":{"path":"~/.local/bin/claude"}}}',
    );
    expect(roundTrip(changed)).toEqual(changed);
```

and inside `it("keeps OperationEvent and UiEvent wire shapes intact", …)`, after the `expect(JSON.stringify(readFailed)).toBe(…);` statement (the one ending `"error":"Input/output error (os error 5)"}}}}',` and `);`), insert:

```ts
    // Phase 4 step C: what `events.rs`'s shape test asserts for the two
    // notes a path-list uninstall writes.
    const moved: OperationEvent = {
      Note: {
        op_id: 7,
        note: { MovedToTrash: { path: "~/.local/share/claude", trashed_to: "~/.Trash/claude" } },
      },
    };
    expect(JSON.stringify(moved)).toBe(
      '{"Note":{"op_id":7,"note":{"MovedToTrash":{"path":"~/.local/share/claude","trashed_to":"~/.Trash/claude"}}}}',
    );
    const trashFailed: OperationEvent = {
      Note: { op_id: 7, note: { TrashFailed: { path: "~/.local/bin/claude", error: "Operation not permitted" } } },
    };
    expect(JSON.stringify(trashFailed)).toBe(
      '{"Note":{"op_id":7,"note":{"TrashFailed":{"path":"~/.local/bin/claude","error":"Operation not permitted"}}}}',
    );
```

In `src/lib/format.test.ts`, inside `describe("outcomeKey for Canager's own failures", …)`, replace the `faults` array with:

```ts
  const faults: Fault[] = [
    "Panicked",
    { ProgramMissing: { program: "/opt/homebrew/bin/brew" } },
    { SpawnFailed: { detail: "Permission denied (os error 13)" } },
    { HomebrewStillUpdating: { minutes: 10 } },
    { PathChanged: { path: "~/.local/bin/claude" } },
    "Internal",
  ];
```

and in `it("passes a fault's data, never a sentence, to its translation", …)`, after the two `expect(…CanagerFailed.HomebrewStillUpdating).toContain("{{minutes}}");` lines (for `en` and `zhCN`) and before `expect(en.operations.logNote.waitingForBrewUpdate).toContain("{{minutes}}");`, insert:

```ts
    // Phase 4 step C: the path a path-list uninstall stopped at, and the
    // two lines it writes in the log.
    expect(outcomeKey({ CanagerFailed: { PathChanged: { path: "~/.local/bin/claude" } } })).toBe(
      "CanagerFailed.PathChanged",
    );
    expect(outcomeArgs({ CanagerFailed: { PathChanged: { path: "~/.local/bin/claude" } } })).toEqual({
      path: "~/.local/bin/claude",
    });
    expect(en.operations.outcome.CanagerFailed.PathChanged).toContain("{{path}}");
    expect(zhCN.operations.outcome.CanagerFailed.PathChanged).toContain("{{path}}");
    expect(en.operations.logNote.movedToTrash).toContain("{{trashedTo}}");
    expect(zhCN.operations.logNote.movedToTrash).toContain("{{trashedTo}}");
    expect(en.operations.logNote.trashFailed).toContain("{{error}}");
    expect(zhCN.operations.logNote.trashFailed).toContain("{{error}}");
```

In `src/components/LogDrawer.test.tsx`, after the closing `});` of `it("says which stream a failed read cut short", …)` and before `it("only shows log lines for the focused operation", …)`, insert:

```tsx
  it("words each path a path-list uninstall moved, and the one macOS refused", async () => {
    // Canager's own two lines in an uninstall that runs no command: where
    // each path went, and the system's words for one it would not move.
    const { findByText } = renderWithProviders(<LogDrawer />);

    act(() => {
      useUiStore.getState().appendLog({
        opId: 1,
        note: { MovedToTrash: { path: "~/.local/share/claude", trashed_to: "~/.Trash/claude" } },
      });
      useUiStore.getState().appendLog({
        opId: 1,
        note: { TrashFailed: { path: "~/.local/bin/claude", error: "Operation not permitted" } },
      });
    });

    await findByText("Moved ~/.local/share/claude to the Trash (now at ~/.Trash/claude).");
    await findByText(
      "Couldn't move ~/.local/bin/claude to the Trash, so the uninstall stopped here. Your Mac gave this reason: Operation not permitted",
    );
  });

```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p banager-core --lib model::tests::test_canager_failed_is_externally_tagged_on_the_wire`
Expected: FAIL to compile — `no variant or associated item named \`PathChanged\` found for enum \`Fault\``.

Run: `cargo test -p banager-core --lib events::tests::test_note_wire_shape_is_what_the_typescript_mirror_expects`
Expected: FAIL to compile — `no variant or associated item named \`MovedToTrash\` found for enum \`LogNote\`` (and `TrashFailed`).

Run: `pnpm typecheck`
Expected: FAIL — in `types.test.ts`, `Object literal may only specify known properties, and 'PathChanged' does not exist in type …` (and `MovedToTrash`); in `format.test.ts` the same for the `faults` element, and `Property 'PathChanged' does not exist` on the `en.operations.outcome.CanagerFailed` lookup; in `LogDrawer.test.tsx` the same for `MovedToTrash`.

- [ ] **Step 3: Add the variants, the mirror, the branches and the copy**

In `crates/banager-core/src/model.rs`, inside `Fault`, after `HomebrewStillUpdating { minutes: u64 },` and before the doc comment `/// Something on Canager's side did not add up`, insert:

```rust
    /// A path a path-list uninstall was about to move is not what the
    /// preview showed: at the confirmation, or when its turn came after the
    /// moves before it, it fails one of the preview's checks (a folder on
    /// its way became a link, say, or a kept path now leads into it), the
    /// list itself changed (a path that was absent is there now, or one the
    /// preview listed is gone), or it is no longer the file the preview
    /// recorded (`st_dev`, `st_ino` and the kind): re-pointed -- as a tool
    /// that updates itself re-points its launcher -- or replaced by another
    /// of the same name. Canager stopped without moving that path; whatever
    /// it moved before is in the Trash, one `LogNote::MovedToTrash` each in
    /// the log. `path` has the home folder abbreviated to `~`; it is the
    /// kept path when a kept path is what changed. Built only by
    /// `removal::execute_removal` (`adapters/standalone/removal.rs`); read
    /// by `faultKey`/`faultArgs` in src/lib/format.ts.
    PathChanged { path: String },
```

In `crates/banager-core/src/events.rs`, inside `LogNote`, after `ReadFailed { stream: Stream, error: String },` insert:

```rust
    /// A path-list uninstall moved `path` (home folder abbreviated to `~`)
    /// to the Trash; `trashed_to` is where the system put it, as
    /// `trashItemAtURL:` reported it and abbreviated the same way (a
    /// colliding name gets a suffix), so someone who wants it back knows
    /// what to look for. One per item, from `removal::execute_removal`;
    /// worded by `LogDrawer.tsx`.
    MovedToTrash { path: String, trashed_to: String },
    /// The system refused to move `path` to the Trash; `error` is its own
    /// description, shown as-is like a tool's stderr. The run stops there,
    /// and `Outcome::Failed` carries the same words as its summary. From
    /// `removal::execute_removal`; worded by `LogDrawer.tsx`.
    TrashFailed { path: String, error: String },
```

In `src/lib/types.ts`, replace the `Fault` union with:

```ts
export type Fault =
  | "Panicked"
  | { ProgramMissing: { program: string } }
  | { SpawnFailed: { detail: string } }
  | { HomebrewStillUpdating: { minutes: number } }
  | { PathChanged: { path: string } }
  | "Internal";
```

and the `LogNote` union with:

```ts
export type LogNote =
  | { WaitingForBrewUpdate: { minutes: number } }
  | { ReadFailed: { stream: Stream; error: string } }
  | { MovedToTrash: { path: string; trashed_to: string } }
  | { TrashFailed: { path: string; error: string } };
```

In `src/lib/format.ts`, in `faultKey`, after `if ("HomebrewStillUpdating" in fault) return "HomebrewStillUpdating";` insert:

```ts
  if ("PathChanged" in fault) return "PathChanged";
```

and in `faultArgs`, after `if ("HomebrewStillUpdating" in fault) return { minutes: fault.HomebrewStillUpdating.minutes };` insert:

```ts
  if ("PathChanged" in fault) return { path: fault.PathChanged.path };
```

In `src/components/LogDrawer.tsx`, in `noteText`, after the `if ("ReadFailed" in note) { … }` block and before `const unhandled: never = note;`, insert:

```ts
  if ("MovedToTrash" in note) {
    const { path, trashed_to } = note.MovedToTrash;
    return t("operations.logNote.movedToTrash", { path, trashedTo: trashed_to });
  }
  if ("TrashFailed" in note) {
    const { path, error } = note.TrashFailed;
    return t("operations.logNote.trashFailed", { path, error });
  }
```

In `src/i18n/completeness.test.ts`, inside `INTERPOLATED_SUBTREES["operations.outcome"]`, after the line `"CanagerFailed.HomebrewStillUpdating",` insert:

```ts
    "CanagerFailed.PathChanged",
```

In `src/i18n/en.json`, inside `"operations": { "logNote": { … } }`, after `"readFailedStderr": "…"` add `,` and:

```json
    "movedToTrash": "Moved {{path}} to the Trash (now at {{trashedTo}}).",
    "trashFailed": "Couldn't move {{path}} to the Trash, so the uninstall stopped here. Your Mac gave this reason: {{error}}"
```

and inside `"operations": { "outcome": { "CanagerFailed": { … } } }`, after `"HomebrewStillUpdating": "…"` add `,` and (before `"Internal"`):

```json
      "PathChanged": "Failed: {{path}} changed between the preview and now (a tool that updates itself can do that), so Canager stopped without moving it. The operation log says what, if anything, was moved before that. Look at the preview again."
```

In `src/i18n/zh-CN.json`, the same two positions:

```json
    "movedToTrash": "已把 {{path}} 移到废纸篓（现在在 {{trashedTo}}）。",
    "trashFailed": "没能把 {{path}} 移到废纸篓，卸载在这里停下了。系统给出的原因是：{{error}}"
```

```json
      "PathChanged": "失败：{{path}} 在预览之后有了变化（会自己更新的工具就可能这样），所以 Canager 停下了，没有移动它。在此之前是否移动过什么，操作日志里能看到。请重新查看预览。"
```

(Spec §9.2's sentence for `PathChanged` says "so Canager didn't move anything"; that is true only when the first item changed. Spec §6.3 has the run stop *at* the changed item with earlier items already in the Trash, so this sentence says what is true in both cases and points at the log, which lists each move — Ruling 17. The parenthesis names the likeliest cause a user can do nothing about: Claude Code re-points its launcher when it updates itself, and a launcher the preview did not see stops the run — Ruling 10.)

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p banager-core --lib model::tests` and `cargo test -p banager-core --lib events::tests` and `pnpm typecheck && pnpm exec vitest run src/lib src/components/LogDrawer.test.tsx src/i18n`
Expected: PASS. `completeness.test.ts` passes because both `operations.logNote.*` keys are literals in `LogDrawer.tsx` and `CanagerFailed.PathChanged` is enumerated in `INTERPOLATED_SUBTREES`.

- [ ] **Step 5: Run the gates**

Run all five from Global Constraints. Expected: all clean.

- [ ] **Step 6: Commit**

```bash
git add crates/banager-core/src/model.rs crates/banager-core/src/events.rs src/lib/types.ts src/lib/types.test.ts src/lib/format.ts src/lib/format.test.ts src/components/LogDrawer.tsx src/components/LogDrawer.test.tsx src/i18n/completeness.test.ts src/i18n/en.json src/i18n/zh-CN.json
git commit -m "$(cat <<'EOF'
Word a path that changed after its preview, and each move to the Trash

A path-list uninstall re-checks every path between the confirmation and
the move; a path that is no longer what the preview showed stops the run
with a fault of its own, so the operation bar can say which path and
send the user back to the preview. Each item moved gets a line in the
log saying where the Trash put it, and a refused item gets the system's
own reason, both as data the drawer words in the user's language rather
than English among the tool's lines. The producer is the standalone
adapter's uninstall, which follows.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: `AdapterError::UninstallUnsafe`, `UninstallUnsafeReason`, the IPC kind, the dialog's wording

**Files:**
- Modify: `crates/banager-core/src/model.rs` — after the closing `}` of `pub enum UninstallBlocked`  [B's file: anchor by symbol]
- Modify: `crates/banager-core/src/adapters/mod.rs` — the `use crate::model::{…}` list; inside `AdapterError`, after `UninstallBlocked { reason: UninstallBlocked },`  [B's file: anchor by symbol]
- Modify: `crates/banager-core/src/ops/mod.rs:36-62` (`execute_error_outcome`'s doc comment and `match`)
- Modify: `src-tauri/src/ipc.rs` — `plan_operation_error` (`:184-217`; anchor: the arm `AdapterError::UninstallBlocked { reason } => uninstall_blocked_json(reason),`), `test_plan_operation_error_never_sends_canagers_own_english` (`:1363-1443`), one test after it
- Modify: `src/lib/sources.ts` — after `parseUninstallBlocked`; `planFailureMessage`'s doc and `switch`  [B's file]
- Modify: `src/lib/sources.test.ts` — the import list, `describe("planErrorMessage")`, one `describe` after `describe("parseUninstallBlocked")`  [B's file]
- Modify: `src/components/UninstallDialog.tsx` — the import list, `refusalText` and its comment  [A's file: anchor by symbol]
- Modify: `src/components/UninstallDialog.test.tsx` — one test after `says a pinned package was not uninstalled and gives the unpin command as code`
- Modify: `src/i18n/en.json`, `src/i18n/zh-CN.json` — `planRefused.uninstallUnsafe.*`
- Test: the ipc tests, `sources.test.ts`, `UninstallDialog.test.tsx`, `completeness.test.ts`.

**Interfaces:**
- Consumes: `AdapterError` and its `thiserror` derive; `plan_operation_error`, with `not_actionable_json` as the precedent for spelling a wire value by hand; `planErrorMessage` → `planFailureMessage` → `parseErrorPayload` (`sources.ts`); `UninstallDialog`'s `refusalText`; `execute_error_outcome` (`ops/mod.rs`).
- Produces (verbatim): `UninstallUnsafeReason::{OutsideHome, SharedFolder, Missing, NotOwnedByYou, NotWhatInstructionsExpect, OverlapsKept}` and `AdapterError::UninstallUnsafe { path: String, reason: UninstallUnsafeReason }` (producer: `removal::plan_removal`, Task 6, stage 6c, a check failing at preview time; production readers landing here: `plan_operation_error` → the IPC kind `{"kind":"uninstall_unsafe","path":…,"reason":<snake_case>}` → `parseUninstallUnsafe`/`UNINSTALL_UNSAFE_KEYS` in `planFailureMessage` → `refusalText`; and `execute_error_outcome`, which maps it to `Fault::Internal` because no `execute` returns it); the TS `UninstallUnsafeReason` union, `UNINSTALL_UNSAFE_KEYS: Record<UninstallUnsafeReason, string>`, `parseUninstallUnsafe(message: string): { path: string; reason: UninstallUnsafeReason } | null`; keys `planRefused.uninstallUnsafe.{outsideHome,sharedFolder,missing,notOwnedByYou,notWhatInstructionsExpect,overlapsKept}`.

- [ ] **Step 1: Write the failing tests**

In `src-tauri/src/ipc.rs`, inside `test_plan_operation_error_never_sends_canagers_own_english`, after the `assert_eq!` for `uninstall_blocked` (its expected value is `serde_json::json!({ "kind": "uninstall_blocked", "reason": "Pinned" })`) and its closing `);`, and before the comment `// Errors no \`plan()\` returns:`, insert:

```rust
        // A path-list uninstall's preview refused one of its checks (phase
        // 4 step C): the path (home folder abbreviated) and the reason, as
        // snake_case data for `parseUninstallUnsafe` in src/lib/sources.ts.
        let v = parse(AdapterError::UninstallUnsafe {
            path: "~/.local/bin/claude".to_string(),
            reason: banager_core::model::UninstallUnsafeReason::NotWhatInstructionsExpect,
        });
        assert_eq!(
            v,
            serde_json::json!({
                "kind": "uninstall_unsafe",
                "path": "~/.local/bin/claude",
                "reason": "not_what_instructions_expect"
            })
        );
```

and after that test's closing `}` (before the next `#[test]`), insert:

```rust

    #[test]
    fn test_plan_operation_error_spells_each_uninstall_unsafe_reason_in_snake_case() {
        use banager_core::model::UninstallUnsafeReason;
        // Written out by hand in `plan_operation_error`, not derived:
        // `UNINSTALL_UNSAFE_KEYS` in src/lib/sources.ts indexes its copy by
        // these exact strings, and the `match` is exhaustive, so a reason
        // added in Rust fails to compile there until it has a spelling --
        // and the TS `Record` fails `tsc` until it has copy.
        for (reason, spelling) in [
            (UninstallUnsafeReason::OutsideHome, "outside_home"),
            (UninstallUnsafeReason::SharedFolder, "shared_folder"),
            (UninstallUnsafeReason::Missing, "missing"),
            (UninstallUnsafeReason::NotOwnedByYou, "not_owned_by_you"),
            (
                UninstallUnsafeReason::NotWhatInstructionsExpect,
                "not_what_instructions_expect",
            ),
            (UninstallUnsafeReason::OverlapsKept, "overlaps_kept"),
        ] {
            let raw = plan_operation_error(AdapterError::UninstallUnsafe {
                path: "~/.claude/downloads".to_string(),
                reason,
            });
            let v: serde_json::Value = serde_json::from_str(&raw).expect("JSON");
            assert_eq!(v["kind"], "uninstall_unsafe");
            assert_eq!(v["path"], "~/.claude/downloads");
            assert_eq!(v["reason"], spelling, "{reason:?}");
        }
    }
```

In `src/lib/sources.test.ts`, add `parseUninstallUnsafe,` to the `import { … } from "./sources"` list (after `parseUninstallBlocked,`). Inside `describe("planErrorMessage", …)`, after the closing `});` of `it("quotes the system's reason a tool could not start inside a translated sentence", …)`, insert:

```ts

  it("words each reason a path-list uninstall preview was refused, naming the path", () => {
    // `plan_operation_error` (src-tauri/src/ipc.rs) spells the reason in
    // snake_case by hand; these six are the whole set, and each has its
    // own sentence. The path arrives with `$HOME` already abbreviated.
    for (const [reason, key] of [
      ["outside_home", "planRefused.uninstallUnsafe.outsideHome"],
      ["shared_folder", "planRefused.uninstallUnsafe.sharedFolder"],
      ["missing", "planRefused.uninstallUnsafe.missing"],
      ["not_owned_by_you", "planRefused.uninstallUnsafe.notOwnedByYou"],
      ["not_what_instructions_expect", "planRefused.uninstallUnsafe.notWhatInstructionsExpect"],
      ["overlaps_kept", "planRefused.uninstallUnsafe.overlapsKept"],
    ]) {
      expect(
        planErrorMessage(
          fakeT,
          JSON.stringify({ kind: "uninstall_unsafe", path: "~/.local/bin/claude", reason }),
          "Claude Code",
        ),
      ).toBe(`${key}({"path":"~/.local/bin/claude"})`);
    }
    // A reason this build has no copy for, or a payload without its path,
    // is shown verbatim rather than guessed at.
    const unknown = '{"kind":"uninstall_unsafe","path":"~/x","reason":"cursed"}';
    expect(planErrorMessage(fakeT, unknown, "Claude Code")).toBe(unknown);
    const pathless = '{"kind":"uninstall_unsafe","reason":"missing"}';
    expect(planErrorMessage(fakeT, pathless, "Claude Code")).toBe(pathless);
  });
```

and after the closing `});` of `describe("parseUninstallBlocked", …)`, append:

```ts

describe("parseUninstallUnsafe", () => {
  it("reads the path and the reason out of the preview's refusal and nothing else", () => {
    expect(
      parseUninstallUnsafe(
        '{"kind":"uninstall_unsafe","path":"~/.claude/downloads","reason":"not_owned_by_you"}',
      ),
    ).toEqual({ path: "~/.claude/downloads", reason: "not_owned_by_you" });
    // The gate's own refusal is a different payload with different copy.
    expect(parseUninstallUnsafe('{"kind":"uninstall_blocked","reason":"Pinned"}')).toBeNull();
    // A reason this build has no copy for is not guessed at; nor is a
    // prototype property, nor a path that is not a string.
    expect(parseUninstallUnsafe('{"kind":"uninstall_unsafe","path":"~/x","reason":"toString"}')).toBeNull();
    expect(parseUninstallUnsafe('{"kind":"uninstall_unsafe","path":7,"reason":"missing"}')).toBeNull();
    expect(parseUninstallUnsafe("not json")).toBeNull();
  });
});
```

In `src/components/UninstallDialog.test.tsx`, after the closing `});` of `it("says a pinned package was not uninstalled and gives the unpin command as code", …)` and before `it("localises the same refusal when it comes back from submit, not from plan", …)`, insert:

```tsx
  it("words a refused path-list preview with the path and the reason, never the payload", async () => {
    // One of the checks a path-list uninstall runs at preview time refused
    // a path (`removal::plan_removal` in
    // crates/banager-core/src/adapters/standalone/removal.rs);
    // `plan_operation_error` in src-tauri/src/ipc.rs sends the path and
    // the reason as data, and the dialog words them. Canager did check,
    // so the sentence is shown on its own, not inside "Couldn't check
    // what this would affect" -- the same reason the pin above skips it.
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_snapshot") {
        return {
          generation: 1,
          detect: "Found",
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
          artifacts: [],
          updates: [],
          refreshed_at: 1,
          stale: false,
          errors: [],
        };
      }
      if (cmd === "plan_operation") {
        throw '{"kind":"uninstall_unsafe","path":"~/.local/bin/claude","reason":"not_what_instructions_expect"}';
      }
      return undefined;
    });

    renderWithProviders(
      <UninstallDialog
        open
        onOpenChange={() => {}}
        request={{ ...request, instance_id: "standalone-claude", artifact_kind: "Binary", name: "claude" }}
        displayName="Claude Code"
      />,
    );

    const alert = await screen.findByRole("alert");
    await waitFor(() =>
      expect(alert).toHaveTextContent(
        "Canager won't remove ~/.local/bin/claude: it couldn't confirm this is what the official instructions describe — it, or a folder it is in, may be a link to somewhere else, or it may be a different kind of file — so removing it could hit the wrong thing. Nothing was changed.",
      ),
    );
    expect(alert.textContent).not.toMatch(/uninstall_unsafe|not_what_instructions_expect|Couldn't check/);
    expect(screen.getByRole("button", { name: "Uninstall" })).toBeDisabled();
  });

```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p canager --lib ipc::tests::test_plan_operation_error_spells_each_uninstall_unsafe_reason_in_snake_case`
Expected: FAIL to compile — `no variant or associated item named \`UninstallUnsafe\` found for enum \`AdapterError\``; `cannot find type \`UninstallUnsafeReason\` in module \`banager_core::model\``.

Run: `pnpm typecheck`
Expected: FAIL — `Module '"./sources"' has no exported member 'parseUninstallUnsafe'`.

- [ ] **Step 3: Add the reason, the error, the IPC arm, the decoder, the dialog branch and the copy**

In `crates/banager-core/src/model.rs`, after the closing `}` of `pub enum UninstallBlocked { … }`, insert:

```rust

/// Why a path-list uninstall's preview refused one of the paths its
/// recipe names: which of the checks in `removal::plan_removal`
/// (`adapters/standalone/removal.rs`; phase 4 spec §6.3) failed. Payload
/// of `AdapterError::UninstallUnsafe`, beside the path (home folder
/// abbreviated). Not serialised by serde: `plan_operation_error` in
/// src-tauri/src/ipc.rs spells each reason by hand, in snake_case, and
/// `UNINSTALL_UNSAFE_KEYS` in src/lib/sources.ts indexes the copy by that
/// spelling -- one producer of the wire form, and an exhaustive `match`
/// there, so a reason added here without a spelling fails to compile. The
/// user sees one of six sentences (`planRefused.uninstallUnsafe.*`);
/// nothing was moved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UninstallUnsafeReason {
    /// Check 1: the path's parent directory, fully resolved, is not inside
    /// the home folder (a `~/.local/bin` that is a link to another volume).
    OutsideHome,
    /// Check 1's never-list: the path's parent directory, fully resolved,
    /// is the home folder itself or one of the folders directly inside it
    /// that many tools share (`recipe::SHARED_FOLDERS`: `~/.local`,
    /// `~/.config`, `~/.cache`, `~/Library`, `~/.cargo`) -- moving a path
    /// there, `~/.local/bin` say, could take other tools' files with it. A
    /// recipe cannot list such a path (`recipes::tests`); the resolved
    /// check also catches a folder that leads into one through a link.
    SharedFolder,
    /// Check 2: the path is not there, and the list needs it -- it is not
    /// optional, and it is not the already-gone program directory of a
    /// launcher-only install.
    Missing,
    /// Check 3: the path belongs to another user.
    NotOwnedByYou,
    /// Check 4: the path is not the kind of thing the tool's own uninstall
    /// instructions describe -- a launcher that is not one link into the
    /// tool's root, a program directory that is a link, a file where a
    /// directory is expected -- or a folder on its way from the home folder
    /// is a link (the ancestry rule, ruling 24 of the step C plan), or it
    /// could not be examined at all.
    NotWhatInstructionsExpect,
    /// With every link resolved, moving the listed paths might take a path
    /// the preview says is kept: a listed path is a kept path, holds one or
    /// what one leads to, or lies inside one other than where the recipe
    /// lists it (`~/.claude -> ~/.local/share/claude`) -- or a kept path
    /// that is there could not be placed. `path` is the kept path (ruling
    /// 25 of the step C plan).
    OverlapsKept,
}
```

In `crates/banager-core/src/adapters/mod.rs`, add `UninstallUnsafeReason` to the `use crate::model::{…}` list (after `UninstallBlocked,`; rustfmt re-wraps it), and inside `AdapterError`, after the `UninstallBlocked { reason: UninstallBlocked },` variant and before the doc comment `/// \`Session::issue_plan\` was asked to plan against an instance that is`, insert:

```rust
    /// A path-list uninstall's `plan()` (`StandaloneAdapter`, through
    /// `removal::plan_removal`) refused one of the paths the recipe names:
    /// `reason` is which check failed, `path` the path with the home folder
    /// abbreviated to `~` (for `OverlapsKept`, the kept path it concerns).
    /// Nothing was moved. Sent to the front end by
    /// `plan_operation_error` (src-tauri/src/ipc.rs) as
    /// `{"kind":"uninstall_unsafe","path":…,"reason":<snake_case>}`, which
    /// `parseUninstallUnsafe` in src/lib/sources.ts words as one of six
    /// sentences -- never as this `Display`, which is for logs. No
    /// `execute` returns it: the same checks failing at run time are
    /// `Fault::PathChanged` (`removal::execute_removal`).
    #[error("unsafe to remove {path}: {reason:?}")]
    UninstallUnsafe {
        path: String,
        reason: UninstallUnsafeReason,
    },
```

In `crates/banager-core/src/ops/mod.rs`, in `execute_error_outcome`'s doc comment, replace the two lines

```rust
/// `InvalidName`, `NotActionable`, `UpdateBlocked`, `UninstallBlocked`
/// (only `issue_plan` builds those two) or `IndexUpdating` (brew's `execute`
```

with

```rust
/// `InvalidName`, `NotActionable`, `UpdateBlocked`, `UninstallBlocked`
/// (only `issue_plan` builds those two), `UninstallUnsafe` (a `plan()`
/// refusal; at run time the same finding is `Fault::PathChanged`) or
/// `IndexUpdating` (brew's `execute`
```

and in its `match`, after `| AdapterError::UninstallBlocked { .. }` insert the line `| AdapterError::UninstallUnsafe { .. }`.

In `src-tauri/src/ipc.rs`, in `plan_operation_error`, after the arm `AdapterError::UninstallBlocked { reason } => uninstall_blocked_json(reason),` insert:

```rust
        // A path-list uninstall's preview refused one of its checks (phase
        // 4 step C): the path, home folder abbreviated, and the reason
        // spelled by hand -- the one producer of these strings, which
        // `UNINSTALL_UNSAFE_KEYS` in src/lib/sources.ts indexes by.
        AdapterError::UninstallUnsafe { path, reason } => {
            use banager_core::model::UninstallUnsafeReason;
            let reason = match reason {
                UninstallUnsafeReason::OutsideHome => "outside_home",
                UninstallUnsafeReason::SharedFolder => "shared_folder",
                UninstallUnsafeReason::Missing => "missing",
                UninstallUnsafeReason::NotOwnedByYou => "not_owned_by_you",
                UninstallUnsafeReason::NotWhatInstructionsExpect => "not_what_instructions_expect",
                UninstallUnsafeReason::OverlapsKept => "overlaps_kept",
            };
            serde_json::json!({ "kind": "uninstall_unsafe", "path": path, "reason": reason })
                .to_string()
        }
```

and in `plan_operation_error`'s doc comment, in the first bullet (`**Canager's own words** go out with no prose at all, …`), after the phrase `` `output_too_large`, `` add `` `uninstall_unsafe` (the path a path-list uninstall preview refused, and which check refused it), ``.

In `src/lib/sources.ts`, after the closing `}` of `parseUninstallBlocked`, insert:

```ts

/**
 * The reasons a path-list uninstall preview can be refused by one of its
 * checks (`removal::plan_removal` in
 * crates/banager-core/src/adapters/standalone/removal.rs), as
 * `plan_operation_error` in src-tauri/src/ipc.rs spells them -- by hand,
 * in snake_case, one `match` arm each. Mirrored here as a union so the
 * copy table below is a `Record` over it: a reason without a sentence
 * fails `tsc`.
 */
export type UninstallUnsafeReason =
  | "outside_home"
  | "shared_folder"
  | "missing"
  | "not_owned_by_you"
  | "not_what_instructions_expect"
  | "overlaps_kept";

/** The `planRefused.uninstallUnsafe.*` sentence for each reason; each
 *  interpolates `{{path}}` (home folder abbreviated on the Rust side). */
export const UNINSTALL_UNSAFE_KEYS: Record<UninstallUnsafeReason, string> = {
  outside_home: "planRefused.uninstallUnsafe.outsideHome",
  shared_folder: "planRefused.uninstallUnsafe.sharedFolder",
  missing: "planRefused.uninstallUnsafe.missing",
  not_owned_by_you: "planRefused.uninstallUnsafe.notOwnedByYou",
  not_what_instructions_expect: "planRefused.uninstallUnsafe.notWhatInstructionsExpect",
  overlaps_kept: "planRefused.uninstallUnsafe.overlapsKept",
};

/**
 * Reads the `{"kind":"uninstall_unsafe","path":…,"reason":…}` payload
 * `plan_operation_error` sends when a path-list uninstall preview refused
 * one of its checks. `null` for anything else, including a reason this
 * build has no copy for or a payload without its path, which
 * `planErrorMessage` then shows verbatim rather than guessing at.
 */
export function parseUninstallUnsafe(
  message: string,
): { path: string; reason: UninstallUnsafeReason } | null {
  const p = parseErrorPayload(message);
  if (!p || p.kind !== "uninstall_unsafe") return null;
  if (typeof p.path !== "string" || typeof p.reason !== "string") return null;
  if (!Object.prototype.hasOwnProperty.call(UNINSTALL_UNSAFE_KEYS, p.reason)) return null;
  return { path: p.path, reason: p.reason as UninstallUnsafeReason };
}
```

In `planFailureMessage`'s doc comment, after the sentence ending `carry data (the name, the path), not prose.` add ` \`uninstall_unsafe\` carries the path a path-list uninstall preview refused and which check refused it (\`parseUninstallUnsafe\`).`, and in its `switch (p.kind) { … }`, before `default:`, insert:

```ts
    case "uninstall_unsafe": {
      const refused = parseUninstallUnsafe(raw);
      return refused ? t(UNINSTALL_UNSAFE_KEYS[refused.reason], { path: refused.path }) : null;
    }
```

In `src/components/UninstallDialog.tsx`, add `parseUninstallUnsafe,` to the `import { … } from "../lib/sources"` list (after `parseUninstallBlocked,`), and replace `refusalText`'s comment and body — from the comment line `// The one refusal this dialog words itself rather than through` through the function's closing `}` — with:

```tsx
  // Two refusals are shown as sentences of their own rather than inside
  // `uninstall.planError`'s "Couldn't check what this would affect", because
  // Canager did check: the tool will not uninstall this package (a pinned
  // Homebrew formula or cask, `uninstall_blocked` in
  // crates/banager-core/src/session/plans.rs), which only a stale Installed
  // page can reach and whose sentence carries the unpin command, set apart
  // as code as on the Installed page's row; and a path-list uninstall whose
  // preview refused one of its paths (`uninstall_unsafe`,
  // `removal::plan_removal`), whose sentence names the path and already
  // says nothing was changed.
  function refusalText(raw: string, frame: "uninstall.planError" | "uninstall.submitError") {
    if (parseUninstallUnsafe(raw) !== null) {
      return planErrorMessage(t, raw, sourceLabel);
    }
    const blocked = parseUninstallBlocked(raw);
    if (blocked === null) {
      return t(frame, { message: planErrorMessage(t, raw, sourceLabel) });
    }
    const copy = UNINSTALL_BLOCKED_KEYS[blocked];
    return withCommand(
      t(copy.refused, { command: COMMAND_SLOT, source: sourceLabel }),
      copy.command(
        { instance_id: request.instance_id, kind: request.artifact_kind, name: request.name },
        instance,
      ),
    );
  }
```

In `src/i18n/en.json`, inside `"planRefused": { … }`, after `"refused": "…"` add `,` and:

```json
    "uninstallUnsafe": {
      "outsideHome": "Canager won't remove {{path}}: the folder it is in leads outside your home folder. Nothing was changed.",
      "sharedFolder": "Canager won't remove {{path}}: it sits directly in your home folder or in a folder other apps share, so moving it could take their files with it. Nothing was changed.",
      "missing": "{{path}} isn't there any more, so Canager stopped. Nothing was changed.",
      "notOwnedByYou": "Canager won't remove {{path}}: it belongs to another user on this Mac. Nothing was changed.",
      "notWhatInstructionsExpect": "Canager won't remove {{path}}: it couldn't confirm this is what the official instructions describe — it, or a folder it is in, may be a link to somewhere else, or it may be a different kind of file — so removing it could hit the wrong thing. Nothing was changed.",
      "overlapsKept": "Canager won't uninstall this: it couldn't confirm that moving the listed paths leaves {{path}}, which it keeps, where it is — through a link, one of them may be the same thing, hold it, or lie inside it. Nothing was changed."
    }
```

In `src/i18n/zh-CN.json`, the same position:

```json
    "uninstallUnsafe": {
      "outsideHome": "Canager 不会移除 {{path}}：它所在的文件夹通向你的个人文件夹之外。什么都没有改动。",
      "sharedFolder": "Canager 不会移除 {{path}}：它直接放在你的个人文件夹里，或者放在其它应用共用的文件夹里，移走它可能连带别的应用的文件。什么都没有改动。",
      "missing": "{{path}} 已经不在了，Canager 停下了。什么都没有改动。",
      "notOwnedByYou": "Canager 不会移除 {{path}}：它属于这台 Mac 上的另一个用户。什么都没有改动。",
      "notWhatInstructionsExpect": "Canager 不会移除 {{path}}：它无法确认这就是官方说明描述的东西（它本身或它所在的某个文件夹可能链到了别处，或者它不是同一类文件），移除可能误伤别的东西。什么都没有改动。",
      "overlapsKept": "Canager 不会卸载：它无法确认移走清单上的路径不会动到它要保留的 {{path}}——通过链接，其中某一条可能就是它、装着它，或者在它里面。什么都没有改动。"
    }
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p canager --lib ipc::tests` and `pnpm typecheck && pnpm exec vitest run src/lib/sources.test.ts src/components/UninstallDialog.test.tsx src/i18n`
Expected: PASS. `completeness.test.ts` passes because the six keys are literals in `UNINSTALL_UNSAFE_KEYS`.

- [ ] **Step 5: Run the gates**

Run all five from Global Constraints. Expected: all clean.

- [ ] **Step 6: Commit**

```bash
git add crates/banager-core/src/model.rs crates/banager-core/src/adapters/mod.rs crates/banager-core/src/ops/mod.rs src-tauri/src/ipc.rs src/lib/sources.ts src/lib/sources.test.ts src/components/UninstallDialog.tsx src/components/UninstallDialog.test.tsx src/i18n/en.json src/i18n/zh-CN.json
git commit -m "$(cat <<'EOF'
Send a refused path-list uninstall preview as the path and the reason

The checks a path-list uninstall runs before it is previewed can refuse
a path: outside the home folder, directly in it or in a folder other
apps share, missing, owned by someone else, not what the tool's
instructions describe, or tangled through a link with a path it keeps.
A Refused(String) would reach the dialog as one generic sentence; this
reason travels as its own error, is spelled on the wire by hand in one
place, and the dialog words each of the six with the path, in the
user's language, on its own rather than
after "Couldn't check what this would affect" -- Canager did check. The
producer is the standalone adapter's uninstall, which follows.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 6: The path-list uninstall — the Trash seam, Claude Code's list, the one-hop launcher, `removal.rs`, the adapter, the row's words, the trust file and the README

One task, seven stages and a commit stage, **one commit** at the end of stage 6h: every type, field and function below has its production reader in this commit — `Trasher` → `removal::execute_removal` → `StandaloneAdapter::execute`; `RealTrasher` → `Session::new`; `ItemKind` → `Trasher::trash`; `Uninstall::Paths`/`RemoveSpec`/`KeepSpec`/`Expect`/`SHARED_FOLDERS` → `removal::plan_removal` → `StandaloneAdapter::plan`; `ItemIdentity` and `PlanAction::TrashPaths.previewed` → `removal::execute_removal`; `Detected.euid` → check 3; `route::probe_strict` → `route::probe` and `StandaloneAdapter::reconcile_after_uninstall` → `run_operation` (Task 2). A commit after any earlier stage would ship one of these with no reader, which is the defect spec §十 names and B's review refused (B's finding 19). The same commit carries every trust-file and README sentence this behaviour makes true or false (stage 6g, Ruling 23), so no commit says "no uninstall" or "never moves a file" while Canager moves files. Each stage still has its own red → green cycle; the five gates run once, at 6h.

**Files (whole task):**
- Modify: `crates/banager-core/Cargo.toml` (a target-specific dependency section after `getrandom = "0.4"`; the `[features]` comment; one `[dev-dependencies]` line), `Cargo.lock` (cargo rewrites banager-core's entry)
- Modify: `crates/banager-core/src/lib.rs` — the crate doc's `[`scan`]` sentence; one module line after `pub mod testing;`  [B's file]
- Create: `crates/banager-core/src/trash/mod.rs`, `crates/banager-core/src/trash/real.rs`, `crates/banager-core/src/trash/mock.rs`
- Modify: `crates/banager-core/src/adapters/standalone/recipe.rs` — the module doc's last paragraph, the `use` line, `Recipe` (a field after `upgrade`), five items appended (`Uninstall`, `RemoveSpec`, `SHARED_FOLDERS`, `Expect`, `KeepSpec`)  [B's file]
- Modify: `crates/banager-core/src/adapters/standalone/recipes.rs` — the two `use` lines, `CLAUDE`'s doc comment and body, `mod tests`  [B's file]
- Modify: `adapters/fixtures/standalone-claude/<the one version directory B recorded>/README.md` — a section appended  [B's file]
- Modify: `crates/banager-core/src/scan/mod.rs` — `display_path`'s visibility and doc  [B's and F's file]
- Modify: `crates/banager-core/src/adapters/standalone/route.rs` — `probe` becomes `probe_strict` plus a one-line `probe` over it, `one_hop` added, three tests (stage 6c); `Probe::LauncherOnly`'s doc comment (6f)  [B's file]
- Create: `crates/banager-core/src/adapters/standalone/removal.rs`
- Modify: `crates/banager-core/src/adapters/standalone/mod.rs` — `pub mod removal;`, `Detected`, `detect`, `testing::Unreadable` (6c); `StandaloneAdapter` and `new`, `with_trash_gap`, `inventory`, `reconcile`'s doc, `reconcile_after_uninstall`, `plan`, `execute`, `all`, `impl Adapter`, `mod tests` (6e)  [B's file]
- Modify: `crates/banager-core/src/model.rs` — `ItemKind` (6a); `ItemIdentity` (6c); `PlanAction::TrashPaths.previewed`, its doc, and the two Task 1 tests that build a `TrashPaths` (6e); the doc comments of `InstanceNote::LauncherOnly`, `UninstallBlocked::NoSafeMethod`, `CancelPolicy::KillThenReconcile` (6f)  [B's file]
- Modify: `crates/banager-core/src/adapters/mod.rs` — Task 1's `test_run_plan_refuses_a_plan_that_runs_no_command` builds a `TrashPaths` (6e)  [B's file]
- Modify: `crates/banager-core/tests/ops_summaries_test.rs` — Task 1's `test_summaries_gives_a_plan_that_runs_no_command_an_empty_argv_preview` builds a `TrashPaths` (6e)
- Modify: `crates/banager-core/src/session/mod.rs` — imports, `Session::new` and its doc  [B's file]
- Modify: `crates/banager-core/tests/ops_upgrade_version_test.rs` — imports; the two `StandaloneAdapter::new(` calls in `claude_upgrade_outputs`  [B's file]
- Modify: `src/lib/types.ts` (`UninstallBlocked`'s doc), `src/lib/sources.ts` (the `LauncherOnly` branch's comment), `src/lib/sources.test.ts` (two `it`s), `src/pages/InstalledPage.test.tsx` (one test added, one comment), `src/i18n/en.json`, `src/i18n/zh-CN.json` (`sourceNotice.launcherOnly.description`)  [B's files]
- Modify: `crates/banager-core/tests/what_we_run_test.rs` — the module doc, the `use` lines, two tests appended  [A's file: anchor by symbol]
- Modify: `docs/what-we-run.md` — the intro, `## When commands run`, B's `## Claude Code`, `## Files Canager reads`, `## Files Canager writes`, a new `## Moving files to the Trash`, `## What Canager never does`  [A's + B's + F's file: anchor by quoted text; A hard-wraps the prose, so match a quoted sentence by its words, not as one line, and keep the file's wrapping when rewriting]
- Modify: `README.md` — B's Claude Code row, the exact-command bullet, two new safety bullets  [B's + F's file: anchor by quoted text]
- Test: `trash/mock.rs`'s, `recipes.rs`'s, `route.rs`'s, `removal.rs`'s, `model.rs`'s and `standalone/mod.rs`'s `mod tests`; `tests/ops_upgrade_version_test.rs`; `tests/ops_summaries_test.rs`; `session/mod.rs`'s `test_new_registers_all_eight_adapters` and `tests/fixtures_layout_test.rs` (unchanged, still green); `tests/what_we_run_test.rs` (two new tests, stage 6g); `sources.test.ts`; `InstalledPage.test.tsx`.

**Interfaces:**
- Consumes: B's `Recipe`, `Route`, `RouteKind::SymlinkIntoRoot`, `route::{expand(home: &Path, spec: &str) -> PathBuf, probe(kind: RouteKind, launcher: &Path, root: &Path) -> Probe, Probe::{Absent, Present { real }, LauncherOnly}}` and its private `canonicalize_existing_prefix`, `CLAUDE`/`RECIPES`, `Detected`, `StandaloneAdapter` and its methods as B wrote them (quoted below where changed), B's test helpers (`exited_0`, `adapter`, `instance_for`, `request`, `detected_adapter`) and `standalone::testing::{TempHome, ClaudeLayout, claude_layout}`; `PlanAction::TrashPaths` (Task 1); `Adapter::reconcile_after_uninstall` (Task 2); `Warning::{WillTrash, WillKeep, AlreadyGone}`, `RemovedWhat`, `KeptWhat` (Task 3); `Fault::PathChanged`, `LogNote::{MovedToTrash, TrashFailed}` (Task 4); `AdapterError::UninstallUnsafe`, `UninstallUnsafeReason` with its six reasons (Task 5); `reconcile_from`, `ensure_instance_match`, `validate_package_name`, `run_plan`; `EventSink`, `OpId`, `OperationEvent`; `std::os::unix::fs::MetadataExt` (`dev`, `ino`, `uid`); `tokio::task::spawn_blocking` (tokio's `rt` feature, which banager-core already has through `tokio-util`'s `rt`, its `Cargo.toml` says).
- Produces (verbatim, from Core Interfaces): the `trash` module (`Trasher` with `trash(path, kind)`, `TrashError`, `RealTrasher`, `MockTrasher` with `kinds()`); `ItemKind`, `ItemIdentity`, `PlanAction::TrashPaths.previewed` (`#[serde(skip)]`); `Recipe.uninstall`, `Uninstall::Paths`, `RemoveSpec`, `Expect`, `KeepSpec`; `CLAUDE.uninstall`; `scan::display_path` as `pub(crate)`; `route::probe_strict`; `removal::{Job, Removal, plan_removal, TIMEOUT_SECS, PUT_BACK_SETTLE, Confirmed, Pacing, execute_removal}`; `Detected { home, euid }`; `testing::Unreadable` (test-only); `StandaloneAdapter::{new(recipe, runner, http, trasher), with_trash_gap, reconcile_after_uninstall}`; `all(runner, http, trasher)`; `Session::new` injecting `RealTrasher`; the restored `sourceNotice.launcherOnly.description`; `recipe::SHARED_FOLDERS`; the trust file's and the README's sentences about all of it, with two `what_we_run_test` tests. Readers of each are named in their doc comments and land in this commit; this task's `what_we_run_test` tests and Task 7's tests read `removal::{PUT_BACK_SETTLE, TIMEOUT_SECS}`, `Uninstall::Paths`, `RECIPES`, `StandaloneAdapter::with_trash_gap`, `MockTrasher`, `RealTrasher` and `ItemKind` too.

#### Stage 6a: the `trash/` module — `Trasher`, `RealTrasher`, `MockTrasher` — and `ItemKind`

- [ ] **Step 1: Write the failing tests**

In `crates/banager-core/src/lib.rs`, after the line `pub mod testing;` insert:

```rust
/// Moving an item to the Trash -- the one change Canager makes to a file
/// in its own process besides its settings, behind a seam like `runner`
/// and `http`.
pub mod trash;
```

Create `crates/banager-core/src/trash/mod.rs` containing only:

```rust
pub mod mock;
```

Create `crates/banager-core/src/trash/mock.rs` with the test module only (Step 3 adds the type above it):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// A throwaway directory beside the mock's own, removed on drop.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Scratch {
            let dir = std::env::temp_dir().join(format!(
                "canager-trash-mock-{tag}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir_all(&dir).expect("create scratch dir");
            Scratch(std::fs::canonicalize(&dir).expect("canonical scratch dir"))
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn test_mock_trasher_moves_a_file_a_directory_and_a_link_as_a_link_and_records_each() {
        let scratch = Scratch::new("moves");
        let file = scratch.0.join("file.txt");
        std::fs::write(&file, b"x").expect("write file");
        let dir = scratch.0.join("dir");
        std::fs::create_dir(&dir).expect("create dir");
        std::fs::write(dir.join("inner"), b"y").expect("write inner");
        let link = scratch.0.join("link");
        std::os::unix::fs::symlink(&file, &link).expect("symlink");

        let trasher = MockTrasher::new();
        let moved_link = trasher
            .trash(&link, ItemKind::Symlink)
            .expect("trash the link");
        let moved_dir = trasher.trash(&dir, ItemKind::Dir).expect("trash the dir");
        let moved_file = trasher
            .trash(&file, ItemKind::File)
            .expect("trash the file");

        // Each landed in the mock's bin under its own name.
        assert_eq!(moved_link, trasher.bin().join("link"));
        assert_eq!(moved_dir, trasher.bin().join("dir"));
        assert_eq!(moved_file, trasher.bin().join("file.txt"));
        // The link was moved as a link, exactly as `trashItemAtURL:` moves
        // one; its target was still in place when it moved.
        assert!(std::fs::symlink_metadata(&moved_link)
            .expect("moved link")
            .file_type()
            .is_symlink());
        assert!(moved_dir.join("inner").is_file());
        assert!(moved_file.is_file());
        assert!(std::fs::symlink_metadata(&link).is_err());
        assert!(std::fs::symlink_metadata(&dir).is_err());
        assert!(std::fs::symlink_metadata(&file).is_err());
        assert_eq!(trasher.calls(), vec![link, dir, file]);
        assert_eq!(
            trasher.kinds(),
            vec![ItemKind::Symlink, ItemKind::Dir, ItemKind::File]
        );
    }

    #[test]
    fn test_mock_trasher_gives_a_second_item_of_the_same_name_a_suffixed_name() {
        // Claude Code's launcher and program directory are both named
        // `claude`. The system suffixes the second with the time of day;
        // the mock suffixes it with the call index, so a test can name it.
        let scratch = Scratch::new("collision");
        let dir = scratch.0.join("share/claude");
        std::fs::create_dir_all(&dir).expect("create dir");
        let link = scratch.0.join("bin/claude");
        std::fs::create_dir_all(link.parent().unwrap()).expect("create bin");
        std::os::unix::fs::symlink(&dir, &link).expect("symlink");

        let trasher = MockTrasher::new();
        assert_eq!(
            trasher.trash(&dir, ItemKind::Dir).expect("dir"),
            trasher.bin().join("claude")
        );
        assert_eq!(
            trasher.trash(&link, ItemKind::Symlink).expect("link"),
            trasher.bin().join("claude 1")
        );
        assert!(trasher.bin().join("claude").is_dir());
        assert!(std::fs::symlink_metadata(trasher.bin().join("claude 1"))
            .expect("moved link")
            .file_type()
            .is_symlink());
    }

    #[test]
    fn test_mock_trasher_refuses_the_nth_call_and_moves_nothing_for_it() {
        let scratch = Scratch::new("refuse");
        let first = scratch.0.join("first");
        let second = scratch.0.join("second");
        std::fs::write(&first, b"1").expect("first");
        std::fs::write(&second, b"2").expect("second");

        let trasher = MockTrasher::new();
        trasher.refuse_call(1, "“second” couldn’t be moved to the Trash.");
        trasher.trash(&first, ItemKind::File).expect("first moves");
        let err = trasher
            .trash(&second, ItemKind::File)
            .expect_err("second is refused");
        assert_eq!(err.to_string(), "“second” couldn’t be moved to the Trash.");
        assert!(second.is_file(), "a refused item stays where it was");
        // Refused calls are recorded too: the adapter's tests assert the
        // whole sequence of what was attempted.
        assert_eq!(trasher.calls(), vec![first, second]);
    }

    #[test]
    fn test_mock_trasher_fires_a_token_after_the_nth_call() {
        let scratch = Scratch::new("cancel");
        let first = scratch.0.join("first");
        std::fs::write(&first, b"1").expect("first");
        let token = CancellationToken::new();

        let trasher = MockTrasher::new();
        trasher.cancel_after_call(0, token.clone());
        assert!(!token.is_cancelled());
        trasher.trash(&first, ItemKind::File).expect("first moves");
        assert!(
            token.is_cancelled(),
            "the user pressed Cancel after the first item"
        );
    }

    #[test]
    fn test_mock_trasher_removes_its_bin_on_drop() {
        let bin = {
            let trasher = MockTrasher::new();
            trasher.bin().to_path_buf()
        };
        assert!(!bin.exists());
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p banager-core --lib trash`
Expected: FAIL to compile — `cannot find type \`MockTrasher\` in this scope`, `cannot find type \`PathBuf\``, `cannot find type \`CancellationToken\``, `cannot find type \`ItemKind\`` (the test module's `use super::*;` has nothing to bring in yet), and `no method named \`trash\``.

- [ ] **Step 3: Write the dependency, the kind, the trait and the two implementations**

In `crates/banager-core/Cargo.toml`, after the line `getrandom = "0.4"` (the last entry of `[dependencies]`) and before `[features]`, insert:

```toml

# `RealTrasher` (src/trash/real.rs) is one call: `NSFileManager
# trashItemAtURL:resultingItemURL:error:`, the call Finder makes to move an
# item to the Trash, which needs no Full Disk Access (docs/what-we-run.md,
# "Moving files to the Trash", says what was observed, Finder's "Put Back"
# included). Both crates are already in Cargo.lock at these versions as
# dependencies of tauri, so no new crate enters the build -- only the
# features named here are switched on. macOS only: there is no Trash to
# call on another target, and `RealTrasher::trash` answers `Unsupported`
# there.
[target.'cfg(target_os = "macos")'.dependencies]
objc2 = { version = "0.6", default-features = false, features = ["std"] }
objc2-foundation = { version = "0.3", default-features = false, features = [
    "std",
    "NSError",
    "NSFileManager",
    "NSString",
    "NSURL",
] }
```

In the same file, under `[dev-dependencies]`, after the line `tokio = { version = "1", features = ["rt-multi-thread"] }`, add:

```toml
# The crate itself with `test-support` on, so its own integration tests
# (`tests/`) can use `trash::MockTrasher`, which that feature keeps out of
# every release build (see `[features]`). Resolver v2 scopes the feature to
# test targets.
banager-core = { path = ".", features = ["test-support"] }
```

and replace the whole comment above `test-support = []` (from ``# Gates `testing::expire_issued_plans`, the one item in `testing` that`` through ``# `testing.rs` for the item this gates and why.``) with:

```toml
# Gates the test code that touches real state rather than building a
# disconnected fixture: `testing::expire_issued_plans`, which mutates a live
# `Session`, and `trash::MockTrasher`, which creates a directory, renames
# into it and deletes it when dropped. Left off by default so a release
# build of this library contains neither; `src-tauri`'s own tests enable it
# via a `[dev-dependencies]` entry on this crate with this feature turned
# on, and this crate's own integration tests via its `[dev-dependencies]`
# entry on itself -- resolver v2 keeps both scoped to test targets and out
# of the release binary's dependency graph. See `testing.rs` and
# `trash/mod.rs` for the items this gates and why.
```

In `crates/banager-core/src/lib.rs`, in the crate doc, replace the line

```rust
//! directories none of those sources put there.
```

with

```rust
//! directories none of those sources put there. [`trash`] is the one place
//! it changes a file itself: macOS's own move-to-Trash, for a confirmed
//! uninstall of a tool that has no uninstall command.
```

In `crates/banager-core/src/model.rs`, directly after the closing `}` of `pub enum PlanAction` (Task 1), insert:

```rust

/// What kind of file `lstat` found at a path -- a symbolic link is itself,
/// never what it points at. Produced by the path-list uninstall's checks
/// (`removal::identity_of`, adapters/standalone/removal.rs) from the item's
/// last `lstat`; part of an `ItemIdentity`, and what `Trasher::trash` is
/// told about the item it moves, so `RealTrasher` builds its URL from the
/// check made immediately before the call instead of looking again
/// (trash/real.rs). No serde: it never crosses IPC.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    File,
    Dir,
    Symlink,
    /// A socket, a pipe, a device: nothing a recipe lists.
    Other,
}
```

Replace the whole of `crates/banager-core/src/trash/mod.rs` with:

```rust
//! Moving files to the Trash: the one change Canager makes to a file in
//! its own process besides its settings (phase 4 spec §6.2, 附录 B).
//! `Trasher` is the seam, like `CommandRunner` and `HttpClient`:
//! `RealTrasher` is macOS's own `NSFileManager trashItemAtURL:` -- the call
//! Finder makes, which needs no Full Disk Access -- and `MockTrasher`
//! renames into a temporary directory and records every call, so an
//! uninstall can be tested end to end without touching anyone's Trash.
//! Whether Finder can later "Put Back" an item this call moved is Finder's
//! own record, observed but not promised (`removal::PUT_BACK_SETTLE`, and
//! docs/what-we-run.md, "Moving files to the Trash").
//!
//! The trait has one method on purpose: in a release build the only thing
//! this module can do to a file is move it to the Trash (`RealTrasher`).
//! `MockTrasher` -- which creates a temporary directory, renames items into
//! it and deletes it when dropped -- is compiled only for tests
//! (`cfg(test)`, or the `test-support` feature, which only dev-dependencies
//! turn on: this crate's own `tests/` through its dev-dependency on
//! itself), so no release build contains a delete, a rename or a directory
//! creation here.

use crate::model::ItemKind;
use std::path::{Path, PathBuf};

#[cfg(any(test, feature = "test-support"))]
pub mod mock;
pub mod real;

#[cfg(any(test, feature = "test-support"))]
pub use mock::MockTrasher;
pub use real::RealTrasher;

/// Why an item was not moved.
#[derive(Debug, thiserror::Error)]
pub enum TrashError {
    /// The system refused to move this item; `detail` is its own words (an
    /// `NSError`'s localized description; `MockTrasher` stands in for it
    /// with a detail of its own), shown as-is like a tool's stderr --
    /// `removal::execute_removal` puts it in `Outcome::Failed`'s summary
    /// and in a `LogNote::TrashFailed`.
    #[error("{detail}")]
    Refused { detail: String },
    /// Canager could not hand the item to the system at all: not macOS,
    /// where nothing implements the move (Canager v0.1 ships for macOS
    /// only, the crate doc in lib.rs; a build for another Unix reaches this
    /// at run time, honestly, rather than failing to compile), or a path
    /// that is not valid UTF-8, which `NSString` cannot carry (macOS's file
    /// systems do not create such names). Canager's own limitation, with no
    /// words of the Mac's to quote: `execute_removal` reports it as
    /// `Fault::Internal`, never as `Failed` or `TrashFailed`. Produced by
    /// `RealTrasher`: off macOS for every path, and on macOS only for a
    /// path whose home folder's own name is not UTF-8 -- so, either way,
    /// for every path of one uninstall alike, from the first on.
    #[error("Canager could not hand this item to the system's Trash")]
    Unsupported,
}

/// The seam. Implemented by `RealTrasher` (macOS's Trash) and
/// `MockTrasher` (a temporary directory, for tests); read by
/// `removal::execute_removal`, which calls it once per path of a
/// `PlanAction::TrashPaths` plan, in order, each immediately after that
/// item's last check.
pub trait Trasher: Send + Sync {
    /// Moves the item at `path` -- a file, a directory, or a symbolic link
    /// (the link itself, never its target) -- to the Trash, and returns
    /// where it now is (for the operation log). `kind` is what the check
    /// made immediately before this call found at `path` (its last
    /// `lstat`): the caller's, so an implementation never needs a second
    /// look of its own between that check and the move.
    fn trash(&self, path: &Path, kind: ItemKind) -> Result<PathBuf, TrashError>;
}
```

Create `crates/banager-core/src/trash/real.rs`:

```rust
//! `RealTrasher`: macOS's own "move to Trash".
//!
//! `NSFileManager trashItemAtURL:resultingItemURL:error:` is what Finder
//! calls. Observed on 2026-09-25 from an ad-hoc-signed bundle launched
//! through LaunchServices *without* Full Disk Access (the same process
//! could not list `~/.Trash`): a file, a directory and a symbolic link
//! whose target existed each moved into `~/.Trash` in every run, the link
//! as a link with its target untouched (a dangling link -- the launcher,
//! moved last in every uninstall -- is `tests/standalone_uninstall_test.rs`'s
//! smoke test), and a name collision suffixed by the system. Finder's
//! "Put Back" record was written for every item when the calls were at
//! least two seconds apart, and for only the first when they came back to
//! back -- an observation, not a documented behaviour
//! (`removal::PUT_BACK_SETTLE`). `docs/what-we-run.md` ("Moving files to
//! the Trash") carries the results.

use super::{TrashError, Trasher};
use crate::model::ItemKind;
use std::path::{Path, PathBuf};

/// The system's Trash. Stateless: `NSFileManager`'s shared manager is safe
/// to use from any thread, and there is nothing to configure. Built once,
/// by `Session::new`, for every standalone adapter.
#[derive(Debug, Default)]
pub struct RealTrasher;

impl RealTrasher {
    pub fn new() -> RealTrasher {
        RealTrasher
    }
}

#[cfg(target_os = "macos")]
impl Trasher for RealTrasher {
    fn trash(&self, path: &Path, kind: ItemKind) -> Result<PathBuf, TrashError> {
        use objc2::rc::{autoreleasepool, Retained};
        use objc2_foundation::{NSFileManager, NSString, NSURL};

        // `NSString` carries UTF-8, and macOS's file systems do not create
        // a name that is not: Canager's limitation, not the system's
        // answer, so `Unsupported` rather than a `Refused` in its own words.
        let Some(utf8) = path.to_str() else {
            return Err(TrashError::Unsupported);
        };
        // What the item is comes from the caller's last check, the `lstat`
        // made immediately before this call -- not from a look of this
        // function's own, which would sit between that check and the move.
        // A symbolic link is never a directory here, so its URL gets no
        // trailing slash that could resolve through it: the link itself is
        // what moves, never what it points at. (`fileURLWithPath:` alone
        // would `stat` through the link to decide.) Building the URL reads
        // nothing from the disk; the next thing that touches this path is
        // the system's move.
        let is_dir = kind == ItemKind::Dir;
        // A pool of its own: this runs on a thread of tokio's blocking pool,
        // which has none, and the URL, the path and the error description
        // come back autoreleased.
        let moved: Result<PathBuf, TrashError> = autoreleasepool(|_| {
            let url = NSURL::fileURLWithPath_isDirectory(&NSString::from_str(utf8), is_dir);
            let mut resulting: Option<Retained<NSURL>> = None;
            NSFileManager::defaultManager()
                .trashItemAtURL_resultingItemURL_error(&url, Some(&mut resulting))
                .map_err(|error| TrashError::Refused {
                    detail: error.localizedDescription().to_string(),
                })?;
            Ok(resulting
                .and_then(|url| url.path())
                .map(|trashed| PathBuf::from(trashed.to_string()))
                // The API fills the URL on success (Apple's contract, and
                // every spike run); the fallback names the home volume's
                // Trash so the log line still says where to look.
                .unwrap_or_else(|| PathBuf::from("~/.Trash")))
        });
        let trashed = moved?;
        #[cfg(debug_assertions)]
        report_trash_access(&trashed);
        Ok(trashed)
    }
}

/// Debug builds only (`cfg(debug_assertions)`: `pnpm tauri build --debug`,
/// `cargo test`; never a release build): whether this process may list the
/// Trash it has just moved an item into, printed to stderr. Without Full
/// Disk Access macOS refuses that listing with `Operation not permitted` --
/// how the Trash spike proved a run had no Full Disk Access -- so the
/// author's pre-merge check reads this line from a Finder-launched debug
/// build (`open --stderr`) to learn what that very process was allowed.
/// It runs after the move, never between an item's check and its move.
#[cfg(all(target_os = "macos", debug_assertions))]
fn report_trash_access(trashed: &Path) {
    let Some(trash) = trashed.parent() else {
        return;
    };
    match std::fs::read_dir(trash) {
        Ok(_) => eprintln!(
            "[canager] debug: read_dir({}) -> Ok: this process can list the Trash (it has Full Disk Access)",
            trash.display()
        ),
        Err(error) => eprintln!("[canager] debug: read_dir({}) -> Err: {error}", trash.display()),
    }
}

#[cfg(not(target_os = "macos"))]
impl Trasher for RealTrasher {
    fn trash(&self, _path: &Path, _kind: ItemKind) -> Result<PathBuf, TrashError> {
        Err(TrashError::Unsupported)
    }
}
```

Prepend to `crates/banager-core/src/trash/mock.rs` (above `#[cfg(test)]`):

```rust
//! `MockTrasher`: the Trash as a temporary directory, for tests only --
//! `trash/mod.rs` compiles this module under `cfg(test)` or the
//! `test-support` feature, never into a release build.

use super::{TrashError, Trasher};
use crate::model::ItemKind;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tokio_util::sync::CancellationToken;

/// Renames each item into a temporary directory of its own -- a symbolic
/// link is renamed as a link, exactly as `trashItemAtURL:` moves one --
/// records every call in order, with the kind it was told the item is, and
/// can be told to refuse the n-th call or to fire a cancellation token
/// after it: the two ways an uninstall stops partway. Its bin is removed on
/// drop -- a permanent delete of whatever was moved into it, which is why
/// it is test-only. Read by the removal and adapter tests
/// (adapters/standalone/), tests/ops_upgrade_version_test.rs and
/// tests/standalone_uninstall_test.rs.
pub struct MockTrasher {
    bin: PathBuf,
    calls: Mutex<Vec<(PathBuf, ItemKind)>>,
    refuse: Mutex<Option<(usize, String)>>,
    cancel_after: Mutex<Option<(usize, CancellationToken)>>,
}

impl MockTrasher {
    pub fn new() -> MockTrasher {
        // Every `adapter()` in the standalone tests builds one, and those
        // tests run in parallel in one process: two can read the same time
        // (macOS's realtime clock counts whole microseconds), so a sequence
        // number keeps each bin its own -- a shared bin would be deleted
        // under one test by the other one's drop.
        static NEXT_BIN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let raw = std::env::temp_dir().join(format!(
            "canager-mock-trash-{}-{}-{}",
            std::process::id(),
            NEXT_BIN.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&raw).expect("create the mock trash directory");
        MockTrasher {
            // Canonical, so the paths `trash` returns compare equal to what
            // a test builds from `bin()` (macOS's `/var/folders` is
            // `/private/var/…`).
            bin: std::fs::canonicalize(&raw).expect("canonical mock trash directory"),
            calls: Mutex::new(Vec::new()),
            refuse: Mutex::new(None),
            cancel_after: Mutex::new(None),
        }
    }

    /// Where trashed items land.
    pub fn bin(&self) -> &Path {
        &self.bin
    }

    /// Every path handed to `trash`, in order -- a refused one included.
    pub fn calls(&self) -> Vec<PathBuf> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .map(|(path, _)| path.clone())
            .collect()
    }

    /// The kind each call was told its item is, in the same order as
    /// `calls`: what the check immediately before the call saw.
    pub fn kinds(&self) -> Vec<ItemKind> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .map(|(_, kind)| *kind)
            .collect()
    }

    /// The `nth` call (0-based, counted over this trasher's whole life)
    /// fails with `Refused { detail }` and moves nothing; every other call
    /// proceeds.
    pub fn refuse_call(&self, nth: usize, detail: &str) {
        *self.refuse.lock().unwrap() = Some((nth, detail.to_string()));
    }

    /// Fires `token` right after the `nth` call (0-based) has moved its
    /// item and before the call returns: a user pressing Cancel while that
    /// move is still being reported.
    pub fn cancel_after_call(&self, nth: usize, token: CancellationToken) {
        *self.cancel_after.lock().unwrap() = Some((nth, token));
    }
}

impl Default for MockTrasher {
    fn default() -> Self {
        MockTrasher::new()
    }
}

impl Trasher for MockTrasher {
    fn trash(&self, path: &Path, kind: ItemKind) -> Result<PathBuf, TrashError> {
        let nth = {
            let mut calls = self.calls.lock().unwrap();
            calls.push((path.to_path_buf(), kind));
            calls.len() - 1
        };
        let refused = self
            .refuse
            .lock()
            .unwrap()
            .clone()
            .filter(|(refused, _)| *refused == nth);
        if let Some((_, detail)) = refused {
            return Err(TrashError::Refused { detail });
        }
        let name = path.file_name().ok_or_else(|| TrashError::Refused {
            detail: format!("{} has no file name", path.display()),
        })?;
        let mut dest = self.bin.join(name);
        if std::fs::symlink_metadata(&dest).is_ok() {
            // The system suffixes a colliding name with the time of day; the
            // call index does the same job here and is predictable.
            dest = self.bin.join(format!("{} {nth}", name.to_string_lossy()));
        }
        std::fs::rename(path, &dest).map_err(|error| TrashError::Refused {
            detail: error.to_string(),
        })?;
        if let Some((_, token)) = self
            .cancel_after
            .lock()
            .unwrap()
            .as_ref()
            .filter(|(after, _)| *after == nth)
        {
            token.cancel();
        }
        Ok(dest)
    }
}

impl Drop for MockTrasher {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.bin);
    }
}

```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p banager-core --lib trash`
Expected: PASS — 5 tests. `RealTrasher` compiles on this Mac (clippy checks it at 6h; the revised `trash` body, with the kind parameter, the typed `autoreleasepool` result and the debug-only line, was built and linted clean against objc2 0.6.4 / objc2-foundation 0.3.2 in a scratch crate, debug and release); nothing calls it until stage 6e. `objc2`/`objc2-foundation` resolve from `Cargo.lock` without the network; if cargo nonetheless reaches for the index, `cargo test -p banager-core --lib trash --offline` works, because the sources are in the local registry cache. `git diff Cargo.lock` now shows banager-core's dependency list with the two crates added, and banager-core itself (the dev-dependency on itself).

- [ ] **Step 5: Continue the task**

No commit: continue to stage 6b.

#### Stage 6b: `Uninstall::Paths` and friends, Claude Code's list, the invariants

- [ ] **Step 1: Write the failing tests**

In `crates/banager-core/src/adapters/standalone/recipes.rs`, inside `mod tests`, after B's `use` lines (`use super::*;`, `use crate::adapters::AdapterMeta;`, `use crate::model::CancelPolicy;`, `use std::path::Path;`) add:

```rust
    use super::super::recipe::{Expect, Uninstall, SHARED_FOLDERS};
    use crate::model::{KeptWhat, RemovedWhat};
```

and after the module's last test (B's `test_every_recipe_latest_url_is_an_allowed_https_host`), before the module's closing `}`, insert:

```rust

    /// The remove and keep paths of a recipe with a path list; empty for
    /// one without (or with another kind of uninstall, step E on).
    fn path_lists(recipe: &Recipe) -> (Vec<&'static str>, Vec<&'static str>) {
        if let Some(Uninstall::Paths { remove, keep }) = &recipe.uninstall {
            (
                remove.iter().map(|spec| spec.path).collect(),
                keep.iter().map(|spec| spec.path).collect(),
            )
        } else {
            (Vec::new(), Vec::new())
        }
    }

    #[test]
    fn test_every_uninstall_path_is_under_home_and_not_in_a_shared_folder() {
        // `route::expand` panics on a path that does not start with `~/`,
        // and the removal's check 1 refuses a path whose folder is the home
        // folder or one of `SHARED_FOLDERS` (ruling 5) -- a recipe listing
        // one would refuse every uninstall, and the never-list exists so no
        // recipe can quietly move `~/.local/bin` whole. Held here, on the
        // data as spelled, so the first test run says so rather than a
        // user's dialog.
        for recipe in RECIPES {
            let (remove, keep) = path_lists(recipe);
            for path in remove.iter().chain(keep.iter()) {
                let rest = path
                    .strip_prefix("~/")
                    .unwrap_or_else(|| panic!("{}: {path:?} must start with ~/", recipe.id));
                assert!(
                    !rest.is_empty() && !rest.ends_with('/') && !rest.contains("..") && !rest.contains("/./"),
                    "{}: {path:?} must name one plain path",
                    recipe.id
                );
            }
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
    fn test_every_paths_recipe_moves_its_launcher_last_and_lists_no_path_inside_another() {
        // Spec §6.2: the launcher last, so a run that stops partway leaves
        // exactly the launcher-only state a second run finishes. Spec
        // §6.3's former check 7: no removed path is inside another removed
        // path (moving `a` and then `a/b` would fail on the second), and no
        // kept path is inside a removed one (it would go with it) -- both
        // properties of the constant, not of the Mac.
        for recipe in RECIPES {
            let Some(Uninstall::Paths { remove, keep }) = &recipe.uninstall else {
                continue;
            };
            let last = remove
                .last()
                .unwrap_or_else(|| panic!("{}: an empty remove list", recipe.id));
            // The launcher itself. (Grok's list ends with `~/.grok/bin`, the
            // folder that holds its launcher: step D widens this with that
            // recipe, not before.)
            assert_eq!(
                last.path, recipe.route.launcher,
                "{}: the last path must be the launcher",
                recipe.id
            );
            assert_eq!(last.what, RemovedWhat::Launcher, "{}", recipe.id);
            assert!(!last.optional, "{}: the launcher is never optional", recipe.id);
            let removed: Vec<&str> = remove.iter().map(|spec| spec.path).collect();
            for a in &removed {
                for b in &removed {
                    assert!(
                        a == b || !b.starts_with(&format!("{a}/")),
                        "{}: {b:?} is inside {a:?}",
                        recipe.id
                    );
                }
                for kept in keep.iter().map(|spec| spec.path) {
                    assert!(
                        kept != *a && !kept.starts_with(&format!("{a}/")),
                        "{}: kept {kept:?} is inside removed {a:?}",
                        recipe.id
                    );
                }
            }
        }
    }

    #[test]
    fn test_claude_codes_uninstall_is_anthropics_two_paths_plus_the_download_cache() {
        // The list, exactly, in execution order: what the dialog shows
        // (spec §6.3's claude row, §6.6). The provenance is the constant's
        // doc comment and the fixture README.
        let Some(Uninstall::Paths { remove, keep }) = &CLAUDE.uninstall else {
            panic!("claude has a path list");
        };
        let remove: Vec<(&str, Expect, RemovedWhat, bool)> = remove
            .iter()
            .map(|spec| (spec.path, spec.expect, spec.what, spec.optional))
            .collect();
        assert_eq!(
            remove,
            vec![
                ("~/.local/share/claude", Expect::Dir, RemovedWhat::Program, false),
                ("~/.claude/downloads", Expect::Dir, RemovedWhat::Cache, true),
                ("~/.local/bin/claude", Expect::SymlinkIntoRoot, RemovedWhat::Launcher, false),
            ]
        );
        let keep: Vec<(&str, KeptWhat)> = keep.iter().map(|spec| (spec.path, spec.what)).collect();
        assert_eq!(
            keep,
            vec![
                ("~/.claude", KeptWhat::SettingsAndHistory),
                ("~/.claude.json", KeptWhat::Settings),
            ]
        );
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p banager-core --lib adapters::standalone::recipes`
Expected: FAIL to compile — `unresolved imports \`super::super::recipe::Expect\`, \`super::super::recipe::Uninstall\``; `no field \`uninstall\` on type \`&Recipe\``.

- [ ] **Step 3: Write the types, the list and the provenance**

In `crates/banager-core/src/adapters/standalone/recipe.rs`, replace B's module-doc paragraph

```rust
//! Only the shapes this step produces exist here. Step C adds the
//! uninstall method (`uninstall: Option<Uninstall>`), step D `backup_globs`,
//! a `FlatFile` route, a `SecondToken` version parse, the other `Latest`
//! sources and an optional `upgrade` (agy updates itself only), step E
//! `$CARGO_HOME` paths. A variant or field defined before anything
//! produces it is this project's most common defect (spec §十三 #41).
```

with

```rust
//! Only the shapes something produces exist here. Step C added the
//! path-list uninstall (`uninstall: Option<Uninstall>`, `Uninstall::Paths`);
//! step D adds `backup_globs`, a `FlatFile` route, `Expect::File`, a
//! `SecondToken` version parse, the other `Latest` sources and an optional
//! `upgrade` (agy updates itself only); step E `$CARGO_HOME` paths and
//! `Uninstall::Command`. A variant or field defined before anything
//! produces it is this project's most common defect (spec §十三 #41).
```

change `use crate::model::CancelPolicy;` to `use crate::model::{CancelPolicy, KeptWhat, RemovedWhat};`, and inside `Recipe`, after the `upgrade` field (its doc `/// The tool's own documented update command. Read by \`plan(Upgrade)\`.` and `pub upgrade: UpgradeCmd,`), add:

```rust
    /// How the tool is removed, or `None` when there is no safe way: the
    /// artifact then carries `UninstallBlocked::NoSafeMethod`, the gate
    /// refuses and the page says so (spec §6.1 "Neither"; no first-batch
    /// recipe since step C, the second batch's Ollama.app). Read by
    /// `inventory` (`uninstall_blocked`), `plan(Uninstall)` and `execute`.
    pub uninstall: Option<Uninstall>,
```

and append after `UpgradeCmd`'s closing `}`:

```rust

/// How a tool is removed (phase 4 spec §6.1). Only the arm this step
/// produces exists: `Command` (rustup's own `self uninstall`, with a
/// probe, warnings and extra locks) arrives with step E.
#[derive(Debug)]
pub enum Uninstall {
    /// No command exists; the vendor's own instructions are a list of
    /// paths. `removal::execute_removal` moves each of `remove` to the
    /// Trash in this order -- the launcher last, so a run that stops
    /// partway leaves the one state a second run finishes (spec §6.2) --
    /// and `keep` is listed in the preview so the user sees what stays.
    /// Where the list comes from is the recipe constant's doc comment and
    /// the fixture README, not a field: nothing in production would read
    /// it (spec §十三 #8/#36). Read by `removal::plan_removal`,
    /// `removal::execute_removal` and the invariants tests in
    /// `recipes.rs`.
    Paths {
        remove: &'static [RemoveSpec],
        keep: &'static [KeepSpec],
    },
}

/// One path a path-list uninstall moves to the Trash.
#[derive(Debug)]
pub struct RemoveSpec {
    /// `~/…`, expanded by `route::expand` against the detected home.
    /// Never directly in the home folder or in one of `SHARED_FOLDERS`
    /// (check 1; `recipes::tests`), and, on the disk, reached only through
    /// real folders (`removal::check_item`).
    pub path: &'static str,
    /// What must be there for the move to be safe (check 4).
    pub expect: Expect,
    /// What it is, for the preview's sentence (`Warning::WillTrash`).
    pub what: RemovedWhat,
    /// Whether its absence is fine (a cache the tool may not have made):
    /// skipped silently when missing. Never the launcher
    /// (`recipes::tests` hold every recipe to that).
    pub optional: bool,
}

/// The folders a `RemoveSpec.path` must never sit directly in, besides the
/// home folder itself: the ones directly under it that many tools share
/// (spec §6.3 check 1's never-list). Moving `~/.local/bin` or
/// `~/.config/fish` whole would take other tools' files with it. Read by
/// `removal::plan_removal` (check 1, against the resolved folder) and by
/// `recipes::tests` (against the recipe's spelling).
pub const SHARED_FOLDERS: [&str; 5] = [".local", ".config", ".cache", "Library", ".cargo"];

/// What check 4 requires at a `RemoveSpec.path` (spec §6.3) -- at the
/// path itself: every folder above it must be a real folder whatever it
/// expects (the ancestry rule, `removal::check_item`). Only the kinds
/// Claude Code's list has exist in this step; `File` (Antigravity's
/// launcher, a plain executable) arrives with step D.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Expect {
    /// A symbolic link -- the one kind of listed path that may be a link --
    /// whose own text points into the recipe's root and which resolves
    /// there, or, dangling, whose own text points into it (the
    /// launcher-only state): exactly as `route::probe` decides for the
    /// launcher.
    SymlinkIntoRoot,
    /// A real directory, not a link.
    Dir,
}

/// One path a path-list uninstall leaves alone, named in the preview so
/// the user knows their settings stay (`Warning::WillKeep`); listed only
/// when it exists.
#[derive(Debug)]
pub struct KeepSpec {
    pub path: &'static str,
    pub what: KeptWhat,
}
```

In `crates/banager-core/src/adapters/standalone/recipes.rs`, replace B's two `use` lines with:

```rust
use super::recipe::{
    Expect, KeepSpec, Latest, Recipe, RemoveSpec, Route, RouteKind, Uninstall, UpgradeCmd,
    VersionCmd, VersionParse,
};
use crate::model::{CancelPolicy, KeptWhat, RemovedWhat};
```

In `CLAUDE`'s doc comment, replace its last three lines

```rust
/// There is no `claude uninstall` subcommand (§2a, `claude --help`); the
/// documented uninstall is two paths, which step C's path-list removal
/// carries. Until then the artifact says `NoSafeMethod`.
```

with

```rust
/// There is no `claude uninstall` subcommand (§2a, `claude --help`). The
/// removal list is Anthropic's own "Uninstall Claude Code → Native"
/// instructions at code.claude.com/docs/en/setup (§7, VERIFIED: exactly
/// `rm -f ~/.local/bin/claude` and `rm -rf ~/.local/share/claude`), plus
/// `~/.claude/downloads`, the staging directory install.sh names as
/// `DOWNLOAD_DIR` for the native route's downloads (§2a, VERIFIED from
/// install.sh; optional -- it may not be there). The kept paths are the
/// same page's separate, explicitly optional step ("Removing configuration
/// files will delete all your settings…"; the VS Code extension, the
/// JetBrains plugin and the desktop app write to `~/.claude/` too, §7):
/// `~/.claude` and `~/.claude.json`, which Canager keeps (spec Q4) -- of
/// `~/.claude` it moves only `downloads`, the cache above. Order: program
/// files, cache, the launcher last (spec §6.2).
```

and inside the `CLAUDE` literal, after the `upgrade: UpgradeCmd { … },` field, add:

```rust
    uninstall: Some(Uninstall::Paths {
        remove: &[
            RemoveSpec {
                path: "~/.local/share/claude",
                expect: Expect::Dir,
                what: RemovedWhat::Program,
                optional: false,
            },
            RemoveSpec {
                path: "~/.claude/downloads",
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
        ],
        keep: &[
            KeepSpec {
                path: "~/.claude",
                what: KeptWhat::SettingsAndHistory,
            },
            KeepSpec {
                path: "~/.claude.json",
                what: KeptWhat::Settings,
            },
        ],
    }),
```

Find the one version directory B recorded (`ls adapters/fixtures/standalone-claude/` prints one name, `2.1.281` unless the recording day's version differed) and append to its `README.md` (documentation, not a recording; every recorded file stays byte-identical):

```markdown

## Uninstall list (phase 4 step C)

Nothing here was recorded for the uninstall: Canager runs no command for
it. The list in `crates/banager-core/src/adapters/standalone/recipes.rs`
(`CLAUDE.uninstall`) comes from Anthropic's "Uninstall Claude Code →
Native" instructions at <https://code.claude.com/docs/en/setup>, read on
2026-09-24: `rm -f ~/.local/bin/claude` and `rm -rf ~/.local/share/claude`
— moved to the Trash by Canager instead, the launcher last — plus
`~/.claude/downloads`, the download staging directory install.sh names
as `DOWNLOAD_DIR` (read from the script), listed as optional. The same
page's separate, optional step removes `~/.claude` and `~/.claude.json`;
Canager keeps both — of `~/.claude` it moves only `downloads`, the cache
above — and says so in the preview. No `claude uninstall`
subcommand exists (`claude --help`, 2026-09-24).
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p banager-core --lib adapters::standalone::recipes`
Expected: PASS — B's invariants and the three new ones. (Nothing in `standalone/mod.rs` reads `uninstall` yet, and `SHARED_FOLDERS`' run-time reader, `plan_removal`, arrives in 6c; the gates run at 6h.)

- [ ] **Step 5: Continue the task**

No commit: continue to stage 6c.

#### Stage 6c: the one-hop launcher and `probe_strict`, `Detected.euid`, `ItemIdentity`, `removal::plan_removal` and its checks

- [ ] **Step 1: Write the failing tests**

In `crates/banager-core/src/adapters/standalone/mod.rs`, replace B's lines

```rust
pub mod recipes;
pub mod route;
```

with

```rust
pub mod recipes;
pub mod removal;
pub mod route;
```

and inside B's `#[cfg(test)] pub(super) mod testing { … }`, after the closing `}` of `pub fn claude_layout` and before the module's closing `}`, insert:

```rust

    /// Takes every permission off the folder `path` until dropped, so an
    /// `lstat` of anything inside it fails with a permission error -- the
    /// "could not tell" a probe must never read as "gone" -- and gives
    /// `0o755` back on drop, so `TempHome` can remove the tree. `None` when
    /// the tests run as root, whom permissions do not stop: the caller
    /// then skips its check and says so.
    pub struct Unreadable(PathBuf);

    impl Unreadable {
        pub fn new(path: &Path) -> Option<Unreadable> {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            if std::fs::metadata(path).expect("the folder exists").uid() == 0 {
                eprintln!("running as root: permissions stop nothing, check skipped");
                return None;
            }
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o000))
                .expect("take the folder's permissions away");
            Some(Unreadable(path.to_path_buf()))
        }
    }

    impl Drop for Unreadable {
        fn drop(&mut self) {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
        }
    }
```

In `crates/banager-core/src/adapters/standalone/route.rs`, inside `mod tests`, change B's line `use super::super::testing::{claude_layout, TempHome};` to `use super::super::testing::{claude_layout, TempHome, Unreadable};`; in B's `test_probe_follows_a_two_hop_link_into_the_root`, after its comment's second line (`// \`current\` link inside the root still resolves into it.`) add the line `// Its first hop lands inside the root, as the one-hop rule requires.`; and after that test's closing `}` (before `test_probe_accepts_a_home_reached_through_a_symlink`) insert:

```rust
    #[test]
    fn test_probe_refuses_a_launcher_that_reaches_the_root_through_a_link_outside_it() {
        // Phase 4 step C: the launcher is one link straight into the root.
        // Through `~/.local/bin/claude-current` it resolves into the root
        // all the same, but once the root is in the Trash that second link
        // dangles, and the launcher's own text -- `claude-current`, outside
        // the root -- no longer says whose it is: a stopped uninstall would
        // read as a finished one. Not the installer's layout, so not this
        // route's instance (the Unknown page lists it), before and after.
        let home = TempHome::new("probe-hop-outside");
        let real = home.executable(".local/share/claude/versions/2.1.281");
        let current = home.link(".local/bin/claude-current", &real);
        let launcher = home.link(".local/bin/claude", &current);
        let root = home.path().join(".local/share/claude");
        assert_eq!(
            probe(RouteKind::SymlinkIntoRoot, &launcher, &root),
            Probe::Absent
        );
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(
            probe(RouteKind::SymlinkIntoRoot, &launcher, &root),
            Probe::Absent
        );
    }

    #[test]
    fn test_probe_counts_a_launcher_dangling_through_the_roots_own_link_as_launcher_only() {
        // The other side of the one-hop rule: a link the tool keeps inside
        // its root is its own business. With the version it points at gone
        // and the root's `current` link left dangling, the launcher's own
        // text still names a place inside the root: launcher-only, the row
        // an Uninstall finishes -- not `Absent`, which would have hidden a
        // root that still holds files.
        let home = TempHome::new("probe-dangling-through-current");
        let real = home.executable(".local/share/claude/versions/2.1.281");
        let current = home.link(".local/share/claude/current", &real);
        let launcher = home.link(".local/bin/claude", &current);
        let root = home.path().join(".local/share/claude");
        std::fs::remove_dir_all(root.join("versions")).unwrap();
        assert_eq!(
            probe(RouteKind::SymlinkIntoRoot, &launcher, &root),
            Probe::LauncherOnly
        );
    }

    #[test]
    fn test_probe_strict_says_it_cannot_tell_where_probe_says_absent() {
        // A launcher whose folder cannot be read: `probe` answers `Absent`
        // (detection's "not installed"), `probe_strict` an error -- the
        // reading after an uninstall must not take a permission error for
        // "the launcher is gone". A launcher that really is gone is
        // `Ok(Absent)` for both.
        let home = TempHome::new("probe-strict-unreadable");
        let layout = claude_layout(&home, "2.1.281");
        let bin = home.path().join(".local/bin");
        {
            let Some(_locked) = Unreadable::new(&bin) else {
                return;
            };
            assert_eq!(
                probe(RouteKind::SymlinkIntoRoot, &layout.launcher, &layout.root),
                Probe::Absent
            );
            let error = probe_strict(RouteKind::SymlinkIntoRoot, &layout.launcher, &layout.root)
                .expect_err("a permission error is not an answer");
            assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
        }
        std::fs::remove_file(&layout.launcher).unwrap();
        assert_eq!(
            probe_strict(RouteKind::SymlinkIntoRoot, &layout.launcher, &layout.root).unwrap(),
            Probe::Absent
        );
    }
```

Create `crates/banager-core/src/adapters/standalone/removal.rs` with the test module only (Step 3 adds the code above it):

```rust
#[cfg(test)]
mod tests {
    use super::super::recipe::{Expect, KeepSpec, RemoveSpec, Uninstall};
    use super::super::recipes::CLAUDE;
    use super::super::testing::{claude_layout, TempHome};
    use super::*;
    use crate::model::{KeptWhat, RemovedWhat, UninstallUnsafeReason, Warning};

    fn claude_lists() -> (&'static [RemoveSpec], &'static [KeepSpec]) {
        match &CLAUDE.uninstall {
            Some(Uninstall::Paths { remove, keep }) => (remove, keep),
            None => panic!("claude has a path list"),
        }
    }

    /// What `detect` would have written for `home`, as the user this test
    /// runs as (the files it makes are that user's).
    fn detected(home: &Path) -> Detected {
        Detected {
            home: home.to_path_buf(),
            euid: std::fs::metadata(home).expect("home metadata").uid(),
        }
    }

    fn claude_job(d: &Detected) -> Job {
        let (remove, keep) = claude_lists();
        Job {
            recipe: &CLAUDE,
            detected: d.clone(),
            remove,
            keep,
        }
    }

    /// A one-path list for a check's own test: `'static`, as a recipe's
    /// is (leaked; a test's lifetime is the process's).
    fn only(spec: RemoveSpec) -> &'static [RemoveSpec] {
        Box::leak(Box::new([spec]))
    }

    fn trash(path: &str, what: RemovedWhat) -> Warning {
        Warning::WillTrash {
            path: path.to_string(),
            what,
        }
    }

    fn keep(path: &str, what: KeptWhat) -> Warning {
        Warning::WillKeep {
            path: path.to_string(),
            what,
        }
    }

    fn refused(result: Result<Removal, AdapterError>) -> (String, UninstallUnsafeReason) {
        match result {
            Err(AdapterError::UninstallUnsafe { path, reason }) => (path, reason),
            other => panic!("expected UninstallUnsafe, got {other:?}"),
        }
    }

    fn identity(path: &Path) -> ItemIdentity {
        identity_of(&std::fs::symlink_metadata(path).expect("lstat"))
    }

    #[test]
    fn test_plan_removal_lists_claude_codes_paths_in_execution_order_with_the_kept_ones_after() {
        // Spec §6.6's dialog, from a full native install: three moves in
        // the recipe's order (the launcher last), then the two kept paths
        // that exist. `~/.claude/downloads` lies inside the kept
        // `~/.claude` exactly as the recipe lists it, which ruling 25
        // allows. Identities are what `execute_removal` compares.
        let home = TempHome::new("removal-full");
        let layout = claude_layout(&home, "2.1.281");
        home.dir(".claude/downloads");
        home.file(".claude/projects/p/session.jsonl");
        home.file(".claude.json");
        let d = detected(home.path());

        let removal = plan_removal(&claude_job(&d)).expect("a plan");

        assert_eq!(
            removal.paths,
            vec![
                home.path().join(".local/share/claude"),
                home.path().join(".claude/downloads"),
                layout.launcher.clone(),
            ]
        );
        let launcher_meta = std::fs::symlink_metadata(&layout.launcher).unwrap();
        assert_eq!(
            removal.identities,
            vec![
                identity(&layout.root),
                identity(&home.path().join(".claude/downloads")),
                ItemIdentity {
                    dev: launcher_meta.dev(),
                    ino: launcher_meta.ino(),
                    kind: ItemKind::Symlink,
                },
            ],
            "the link's own identity, not its target's"
        );
        assert_eq!(removal.identities[0].kind, ItemKind::Dir);
        assert_eq!(
            removal.warnings,
            vec![
                trash("~/.local/share/claude", RemovedWhat::Program),
                trash("~/.claude/downloads", RemovedWhat::Cache),
                trash("~/.local/bin/claude", RemovedWhat::Launcher),
                keep("~/.claude", KeptWhat::SettingsAndHistory),
                keep("~/.claude.json", KeptWhat::Settings),
            ]
        );
    }

    #[test]
    fn test_plan_removal_skips_a_missing_optional_path_and_a_missing_kept_path_silently() {
        // A Mac without `~/.claude/downloads` or `~/.claude.json`: neither
        // is a sentence (ruling 6).
        let home = TempHome::new("removal-minimal");
        let layout = claude_layout(&home, "2.1.281");
        let d = detected(home.path());

        let removal = plan_removal(&claude_job(&d)).expect("a plan");

        assert_eq!(
            removal.paths,
            vec![
                home.path().join(".local/share/claude"),
                layout.launcher.clone()
            ]
        );
        assert_eq!(
            removal.warnings,
            vec![
                trash("~/.local/share/claude", RemovedWhat::Program),
                trash("~/.local/bin/claude", RemovedWhat::Launcher),
            ]
        );
    }

    #[test]
    fn test_plan_removal_refuses_a_required_path_that_is_missing() {
        // No launcher at all is not the launcher-only state: the row
        // should not exist, and a plan for it stops (check 2).
        let home = TempHome::new("removal-missing");
        let layout = claude_layout(&home, "2.1.281");
        std::fs::remove_file(&layout.launcher).unwrap();
        let d = detected(home.path());

        let (path, reason) = refused(plan_removal(&claude_job(&d)));
        assert_eq!(
            (path.as_str(), reason),
            ("~/.local/bin/claude", UninstallUnsafeReason::Missing)
        );
    }

    #[test]
    fn test_plan_removal_lists_the_program_dir_as_already_gone_on_a_launcher_only_install() {
        // The state a stopped run leaves (spec §6.2): the program files are
        // in the Trash, the link dangles. Check 2 on the program directory
        // becomes `AlreadyGone`, read from the disk now rather than from the
        // row's note (ruling 4), and the list is the launcher.
        let home = TempHome::new("removal-launcher-only");
        let layout = claude_layout(&home, "2.1.281");
        std::fs::remove_dir_all(&layout.root).unwrap();
        let d = detected(home.path());

        let removal = plan_removal(&claude_job(&d)).expect("a plan");

        assert_eq!(removal.paths, vec![layout.launcher.clone()]);
        assert_eq!(
            removal.warnings,
            vec![
                Warning::AlreadyGone {
                    path: "~/.local/share/claude".to_string()
                },
                trash("~/.local/bin/claude", RemovedWhat::Launcher),
            ]
        );
    }

    #[test]
    fn test_plan_removal_refuses_a_parent_that_leads_outside_home() {
        // Check 1, for a directory and for a link alike (spec §十三 #26):
        // a `~/.local/share` or `~/.local/bin` that is itself a link to
        // another volume would carry the move out of the home folder.
        let outside = TempHome::new("removal-outside");

        // (a) The program directory's parent is a link out.
        let home = TempHome::new("removal-parent-dir");
        let real_share = outside.dir("share");
        let real = outside.executable("share/claude/versions/2.1.281");
        home.link(".local/share", &real_share);
        home.link(".local/bin/claude", &real);
        let d = detected(home.path());
        let (path, reason) = refused(plan_removal(&claude_job(&d)));
        assert_eq!(
            (path.as_str(), reason),
            ("~/.local/share/claude", UninstallUnsafeReason::OutsideHome)
        );

        // (b) The launcher's directory is a link out; the program directory
        // is fine, so it is the launcher that is refused.
        let home = TempHome::new("removal-parent-link");
        let real = home.executable(".local/share/claude/versions/2.1.281");
        let outside_bin = outside.dir("bin");
        std::os::unix::fs::symlink(real, outside_bin.join("claude")).unwrap();
        home.link(".local/bin", &outside_bin);
        let d = detected(home.path());
        let (path, reason) = refused(plan_removal(&claude_job(&d)));
        assert_eq!(
            (path.as_str(), reason),
            ("~/.local/bin/claude", UninstallUnsafeReason::OutsideHome)
        );
    }

    #[test]
    fn test_plan_removal_refuses_a_path_reached_through_a_linked_folder_inside_home() {
        // Ruling 24, the review's second counterexample: with `~/.claude ->
        // ~/Documents`, an unrelated `~/Documents/downloads` would pass
        // every other check as Claude Code's cache. Every folder between the
        // home folder and a listed path must be a real folder -- inside the
        // home folder or not -- so it is refused, and so is a launcher whose
        // `~/.local/bin` is kept as a link to a dotfiles folder.
        let home = TempHome::new("removal-linked-cache-parent");
        let _layout = claude_layout(&home, "2.1.281");
        let documents = home.dir("Documents");
        home.dir("Documents/downloads");
        home.link(".claude", &documents);
        let d = detected(home.path());
        let (path, reason) = refused(plan_removal(&claude_job(&d)));
        assert_eq!(
            (path.as_str(), reason),
            (
                "~/.claude/downloads",
                UninstallUnsafeReason::NotWhatInstructionsExpect
            )
        );

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

    #[test]
    fn test_plan_removal_refuses_when_a_kept_path_leads_into_what_it_would_move() {
        // Ruling 25, the review's first counterexample: with `~/.claude ->
        // ~/.local/share/claude`, moving the program folder would take the
        // settings and history the preview says it keeps. Refused, naming
        // the kept path -- and the same for a kept file that is a link into
        // the program folder.
        let home = TempHome::new("removal-settings-aliased");
        let layout = claude_layout(&home, "2.1.281");
        home.link(".claude", &layout.root);
        let d = detected(home.path());
        let (path, reason) = refused(plan_removal(&claude_job(&d)));
        assert_eq!(
            (path.as_str(), reason),
            ("~/.claude", UninstallUnsafeReason::OverlapsKept)
        );

        let home = TempHome::new("removal-settings-file-aliased");
        let layout = claude_layout(&home, "2.1.281");
        let inside = home.file(".local/share/claude/settings.json");
        home.link(".claude.json", &inside);
        let d = detected(home.path());
        let (path, reason) = refused(plan_removal(&claude_job(&d)));
        assert_eq!(
            (path.as_str(), reason),
            ("~/.claude.json", UninstallUnsafeReason::OverlapsKept)
        );
        assert!(layout.root.is_dir());
    }

    #[test]
    fn test_plan_removal_accepts_a_home_reached_through_a_symlink() {
        // `HostEnv.home` may be a link to the real home; both sides are
        // resolved before check 1 compares them, and the plan's paths keep
        // the spelling the home was given.
        let tmp = TempHome::new("removal-linked-home");
        let real_home = tmp.dir("real-home");
        let real = tmp.executable("real-home/.local/share/claude/versions/2.1.281");
        tmp.link("real-home/.local/bin/claude", &real);
        let linked_home = tmp.link("linked-home", &real_home);
        let d = detected(&linked_home);

        let removal = plan_removal(&claude_job(&d)).expect("a plan");

        assert_eq!(
            removal.paths,
            vec![
                linked_home.join(".local/share/claude"),
                linked_home.join(".local/bin/claude"),
            ]
        );
        assert_eq!(
            removal.warnings,
            vec![
                trash("~/.local/share/claude", RemovedWhat::Program),
                trash("~/.local/bin/claude", RemovedWhat::Launcher),
            ]
        );
    }

    #[test]
    fn test_plan_removal_refuses_a_path_the_user_does_not_own() {
        // Check 3, with an injected euid: a test cannot make a file owned
        // by someone else, but the check compares two numbers.
        let home = TempHome::new("removal-owner");
        let _layout = claude_layout(&home, "2.1.281");
        let d = Detected {
            euid: detected(home.path()).euid + 1,
            ..detected(home.path())
        };

        let (path, reason) = refused(plan_removal(&claude_job(&d)));
        assert_eq!(
            (path.as_str(), reason),
            (
                "~/.local/share/claude",
                UninstallUnsafeReason::NotOwnedByYou
            )
        );
    }

    #[test]
    fn test_plan_removal_refuses_what_the_instructions_do_not_describe() {
        // Check 4, each `Expect`: a launcher that links elsewhere, a
        // launcher that is a plain file, a program directory that is a link.
        let home = TempHome::new("removal-launcher-elsewhere");
        let layout = claude_layout(&home, "2.1.281");
        let elsewhere = home.executable("elsewhere/claude");
        std::fs::remove_file(&layout.launcher).unwrap();
        std::os::unix::fs::symlink(elsewhere, &layout.launcher).unwrap();
        let d = detected(home.path());
        let (path, reason) = refused(plan_removal(&claude_job(&d)));
        assert_eq!(
            (path.as_str(), reason),
            (
                "~/.local/bin/claude",
                UninstallUnsafeReason::NotWhatInstructionsExpect
            )
        );

        let home = TempHome::new("removal-launcher-file");
        let layout = claude_layout(&home, "2.1.281");
        std::fs::remove_file(&layout.launcher).unwrap();
        home.executable(".local/bin/claude");
        let d = detected(home.path());
        let (path, reason) = refused(plan_removal(&claude_job(&d)));
        assert_eq!(
            (path.as_str(), reason),
            (
                "~/.local/bin/claude",
                UninstallUnsafeReason::NotWhatInstructionsExpect
            )
        );

        let home = TempHome::new("removal-root-is-link");
        let real_root = home.dir("elsewhere/claude-root");
        home.executable("elsewhere/claude-root/versions/2.1.281");
        home.link(".local/share/claude", &real_root);
        home.link(
            ".local/bin/claude",
            &home.path().join(".local/share/claude/versions/2.1.281"),
        );
        let d = detected(home.path());
        let (path, reason) = refused(plan_removal(&claude_job(&d)));
        assert_eq!(
            (path.as_str(), reason),
            (
                "~/.local/share/claude",
                UninstallUnsafeReason::NotWhatInstructionsExpect
            )
        );
    }

    #[test]
    fn test_plan_removal_refuses_an_optional_path_of_the_wrong_shape() {
        // Ruling 1: in this step an `optional` path that exists but is not
        // what the list expects -- `~/.claude/downloads` as a link, or as a
        // file -- refuses the whole uninstall. Step D replaces this branch
        // with a skip and `WillKeep { NotOurs }`, and this test with its own.
        let home = TempHome::new("removal-optional-link");
        let _layout = claude_layout(&home, "2.1.281");
        let elsewhere = home.dir("elsewhere/downloads");
        home.link(".claude/downloads", &elsewhere);
        let d = detected(home.path());
        let (path, reason) = refused(plan_removal(&claude_job(&d)));
        assert_eq!(
            (path.as_str(), reason),
            (
                "~/.claude/downloads",
                UninstallUnsafeReason::NotWhatInstructionsExpect
            )
        );

        let home = TempHome::new("removal-optional-file");
        let _layout = claude_layout(&home, "2.1.281");
        home.file(".claude/downloads");
        let d = detected(home.path());
        let (path, reason) = refused(plan_removal(&claude_job(&d)));
        assert_eq!(
            (path.as_str(), reason),
            (
                "~/.claude/downloads",
                UninstallUnsafeReason::NotWhatInstructionsExpect
            )
        );
    }

    #[test]
    fn test_plan_removal_refuses_a_path_whose_folder_is_home_or_shared() {
        // Check 1's never-list (spec §6.3, ruling 5): a path directly in the
        // home folder, or directly in one of the folders many tools share,
        // is refused -- a recipe that lists `~/.local/bin` for
        // `~/.local/bin/claude` must not move every tool's launcher -- and
        // so is one whose folder leads into a shared folder through a link
        // (dotfiles kept elsewhere in the home folder). No shipped recipe
        // lists such a path (`recipes::tests`); this pins the check itself.
        let home = TempHome::new("removal-shared-folder");
        let _layout = claude_layout(&home, "2.1.281");
        home.dir("claude-thing");
        let dotfiles = home.dir("dotfiles/config");
        home.dir("dotfiles/config/fish");
        home.link(".config", &dotfiles);
        let d = detected(home.path());
        for listed in ["~/claude-thing", "~/.local/bin", "~/.config/fish"] {
            let job = Job {
                recipe: &CLAUDE,
                detected: d.clone(),
                remove: only(RemoveSpec {
                    path: listed,
                    expect: Expect::Dir,
                    what: RemovedWhat::Launcher,
                    optional: false,
                }),
                keep: &[],
            };
            let (path, reason) = refused(plan_removal(&job));
            assert_eq!(
                (path.as_str(), reason),
                (listed, UninstallUnsafeReason::SharedFolder)
            );
        }
    }

    #[test]
    fn test_plan_removal_refuses_an_empty_list_as_a_plain_refusal() {
        // Every listed path optional and absent: nothing to move. A plain
        // `Refused`, on purpose without copy of its own -- unreachable for a
        // shipped recipe, whose launcher is never optional and exists while
        // the row does (spec §6.3).
        let home = TempHome::new("removal-empty");
        let _layout = claude_layout(&home, "2.1.281");
        let d = detected(home.path());
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
        };

        assert!(matches!(plan_removal(&job), Err(AdapterError::Refused(_))));
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p banager-core --lib adapters::standalone`
Expected: FAIL to compile — in `route.rs`, `cannot find function \`probe_strict\` in this scope`; in `removal.rs`, `cannot find function \`plan_removal\``, `cannot find type \`Job\``, `\`Removal\``, `\`ItemIdentity\`` (the test module's `use super::*;` has nothing to bring in yet); and `struct \`Detected\` has no field named \`euid\``.

- [ ] **Step 3: Write `Detected.euid`, widen `display_path`, the one-hop probe, `ItemIdentity` and `plan_removal`**

In `crates/banager-core/src/adapters/standalone/mod.rs`, replace B's `Detected` (its doc comment `/// What \`detect\` learned that the \`Adapter\` methods without a \`HostEnv\`` through the struct's closing `}`) with:

```rust
/// What `detect` learned that the `Adapter` methods without a `HostEnv`
/// need later -- the same seat `CargoAdapter.binstall` is (detect writes,
/// later calls read; `Session` always detects before it asks anything
/// else of an instance). `home`, which `check_updates` needs to find
/// `~/.claude/settings.json` and the removal needs to expand its paths;
/// `euid`, which the removal's check 3 compares each path's owner with
/// (`removal::plan_removal`). Step E adds `cargo_home` (rustup's cargo
/// lock). `Clone`, so `plan` and `execute` take a copy out of the mutex
/// before they await anything, and the removal owns one for the blocking
/// pool (`removal::Job`).
#[derive(Clone, Debug)]
pub struct Detected {
    pub home: PathBuf,
    pub euid: u32,
}
```

and in `detect`, replace

```rust
        *self.detected.lock().unwrap() = Some(Detected {
            home: env.home.clone(),
        });
```

with

```rust
        *self.detected.lock().unwrap() = Some(Detected {
            home: env.home.clone(),
            euid: env.euid,
        });
```

In `crates/banager-core/src/scan/mod.rs`, change F's signature line `fn display_path(path: &Path, home: &Path) -> PathBuf {` to `pub(crate) fn display_path(path: &Path, home: &Path) -> PathBuf {`, and add to the end of its doc comment (after `/// compares absolute paths; only the output is abbreviated.`):

```rust
/// Also the one `~` rule for the sentences a path-list uninstall sends
/// (`adapters::standalone::removal`): data the user reads, never a path
/// anything acts on.
```

In `crates/banager-core/src/model.rs`, after the closing `}` of `pub enum ItemKind` (stage 6a), insert:

```rust

/// Which file a path named at one moment: `(st_dev, st_ino)` and the kind,
/// from `lstat` -- a link's own, never its target's. A link re-pointed (as
/// `ln -sf` and Claude Code's updater re-point one) or a folder replaced by
/// another of the same name is a new identity. Recorded by
/// `removal::plan_removal` for every path it lists; the preview's travel
/// with the plan (`PlanAction::TrashPaths.previewed`, stage 6e), and
/// `removal::execute_removal` compares them with what is there at the
/// confirmation and again immediately before each move. Server-side only:
/// no serde, and the field that carries it is skipped on the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemIdentity {
    pub dev: u64,
    pub ino: u64,
    pub kind: ItemKind,
}
```

In `crates/banager-core/src/adapters/standalone/route.rs`, replace B's `probe` — from its doc comment `/// Whether \`launcher\` is this route's install of the tool whose root is` through the function's closing `}`, just above `/// \`target\` as seen from \`dir\`, with \`.\` and \`..\` folded away without` — with (Ruling 27; B's steps 1–3 and B's `canonicalize_existing_prefix` unchanged inside it):

```rust
/// Whether `launcher` is this route's install of the tool whose root is
/// `root`, and if so which binary it runs (spec §3.3 steps 1-3). Whatever
/// `probe_strict` cannot tell -- a symlink loop, a permission error, a
/// dangling link along the way -- reads as `Absent` here: "not installed",
/// never "not responding", for detection and every refresh.
pub fn probe(kind: RouteKind, launcher: &Path, root: &Path) -> Probe {
    probe_strict(kind, launcher, root).unwrap_or(Probe::Absent)
}

/// `probe`, keeping what it could not tell: `Ok(Absent)` only when the
/// disk says so -- no launcher at all (its `lstat` answers "no such
/// file"), or one that is not this route's -- and `Err` for any other
/// error on the way. Read by `probe`, and by
/// `StandaloneAdapter::reconcile_after_uninstall`, which must not call an
/// uninstall finished because a permission error hid the launcher.
///
/// The launcher is one link, from the installer's fixed path straight
/// into the root: its own text (`one_hop`) must name a place inside the
/// root, and so must where it finally resolves. A launcher that reaches
/// the root through another link outside it (`claude ->
/// ~/.local/bin/claude-current -> ~/.local/share/claude/versions/<v>`) is
/// not the installer's layout and is `Absent` -- the Unknown page lists it
/// -- because once the root has gone to the Trash that other link would
/// dangle and the launcher's own text would no longer say whose it is: a
/// stopped uninstall would read as a finished one. A link the tool keeps
/// inside its root (a `current`) moves with the root and is its business.
pub fn probe_strict(kind: RouteKind, launcher: &Path, root: &Path) -> std::io::Result<Probe> {
    // Step 1: `lstat`, not `stat` -- a dangling link is still a launcher.
    let meta = match std::fs::symlink_metadata(launcher) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Probe::Absent),
        Err(error) => return Err(error),
    };
    match std::fs::canonicalize(launcher) {
        Ok(real) => {
            // Step 2: shared exclusion.
            if has_component(&real, &PACKAGE_MANAGER_MARKERS) {
                return Ok(Probe::Absent);
            }
            // Step 3: the fingerprint.
            match kind {
                RouteKind::SymlinkIntoRoot => {
                    if !meta.file_type().is_symlink() {
                        return Ok(Probe::Absent);
                    }
                    let canonical_root = match std::fs::canonicalize(root) {
                        Ok(canonical_root) => canonical_root,
                        // The launcher runs something, and there is no
                        // root it could be in.
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                            return Ok(Probe::Absent)
                        }
                        Err(error) => return Err(error),
                    };
                    if one_hop(launcher)?.starts_with(&canonical_root)
                        && real.starts_with(&canonical_root)
                    {
                        Ok(Probe::Present { real })
                    } else {
                        Ok(Probe::Absent)
                    }
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if !meta.file_type().is_symlink() {
                return Ok(Probe::Absent);
            }
            let hop = one_hop(launcher)?;
            let root = canonicalize_existing_prefix(root)?;
            if !has_component(&hop, &PACKAGE_MANAGER_MARKERS) && hop.starts_with(&root) {
                Ok(Probe::LauncherOnly)
            } else {
                Ok(Probe::Absent)
            }
        }
        // A loop or a permission error: not a dangling native install --
        // and no proof that there is none.
        Err(error) => Err(error),
    }
}

/// Where `launcher`'s own text points, as a place on the disk: the text
/// (`readlink`) taken from the launcher's resolved directory -- so a
/// relative `../` means what it means under a `~/.local/bin` that is
/// itself a link -- with the destination's own directory resolved as far
/// as it exists and the destination itself not followed: it may be gone
/// (the launcher-only state) or a link the tool keeps inside its root.
/// Read by `probe_strict`, in both of its launcher arms.
fn one_hop(launcher: &Path) -> std::io::Result<PathBuf> {
    let text = std::fs::read_link(launcher)?;
    let dir = std::fs::canonicalize(launcher.parent().unwrap_or(Path::new("/")))?;
    // `join` with an absolute text is that text.
    let joined = dir.join(text);
    match (joined.parent(), joined.file_name()) {
        (Some(parent), Some(name)) => Ok(canonicalize_existing_prefix(parent)?.join(name)),
        // A text ending in `..`, or naming `/`: a directory, resolved like
        // any other -- never a program the route could run.
        _ => canonicalize_existing_prefix(&joined),
    }
}
```

Prepend to `crates/banager-core/src/adapters/standalone/removal.rs` (above `#[cfg(test)]`):

```rust
//! The path-list uninstall: how a tool with no uninstall command is removed
//! (phase 4 spec §6.2-§6.3, D8). `plan_removal` turns a recipe's
//! `Uninstall::Paths` list into the absolute paths to move, what each one
//! is (`ItemIdentity`) and the warnings the dialog lists, under the checks
//! below; `execute_removal` runs the same checks again at the confirmation,
//! compares every identity with the one the preview recorded, and then
//! moves each path to the Trash in order -- the launcher last -- running
//! every check on that item once more immediately before its move. Nothing
//! here knows the `Adapter` contract (`mod.rs` does) or how an item is
//! moved (`crate::trash` does).
//!
//! What the checks guard against is change by accident: the tool's own
//! updater, the user, another app doing its ordinary work between the
//! preview and the click, or during the pauses between moves. A program
//! running as the user can do everything Canager can; one that swaps an
//! item in the instant between that item's last check and the system's
//! move could still race it (`take_turn`; docs/what-we-run.md says so).

use super::recipe::{Expect, KeepSpec, Recipe, RemoveSpec, SHARED_FOLDERS};
use super::route::{self, Probe};
use super::Detected;
use crate::adapters::AdapterError;
use crate::model::{ItemIdentity, ItemKind, UninstallUnsafeReason, Warning};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

/// One tool's removal as the checks see it: the recipe (for its route),
/// what detect learned (home, euid) and the recipe's two lists. Owned --
/// a copy of `Detected`, the rest `'static` recipe data -- so each item's
/// last check and its move can run together on tokio's blocking pool
/// (`execute_removal`). Built by `StandaloneAdapter::plan` and `::execute`;
/// read by `plan_removal` and `execute_removal`.
#[derive(Clone, Debug)]
pub struct Job {
    pub recipe: &'static Recipe,
    pub detected: Detected,
    pub remove: &'static [RemoveSpec],
    pub keep: &'static [KeepSpec],
}

/// What `plan_removal` found: the absolute paths to move, in order; what
/// each one was at that moment; and the dialog's warnings -- one
/// `WillTrash` per path in the same order, an `AlreadyGone` for a program
/// path an earlier stopped run already moved, then one `WillKeep` per kept
/// path that exists. Read by `StandaloneAdapter::plan` (all three, into
/// the `Plan`: `PlanAction::TrashPaths { paths, previewed: identities }`
/// and `warnings`) and by `execute_removal` (its fresh look).
#[derive(Debug, PartialEq, Eq)]
pub struct Removal {
    pub paths: Vec<PathBuf>,
    pub identities: Vec<ItemIdentity>,
    pub warnings: Vec<Warning>,
}

/// A path that fails a check, and which one. The path is absolute -- a
/// listed path, or for `OverlapsKept` the kept path a listed one would
/// disturb -- and abbreviated only on its way into a sentence.
#[derive(Debug)]
struct Refusal {
    path: PathBuf,
    reason: UninstallUnsafeReason,
}

impl Refusal {
    fn new(path: &Path, reason: UninstallUnsafeReason) -> Refusal {
        Refusal {
            path: path.to_path_buf(),
            reason,
        }
    }

    /// The preview's refusal (`UninstallUnsafe`), the path abbreviated.
    fn into_error(self, home: &Path) -> AdapterError {
        AdapterError::UninstallUnsafe {
            path: shown(home, &self.path),
            reason: self.reason,
        }
    }
}

/// `path` with the home folder abbreviated to `~`, for a sentence: F's one
/// rule (`scan::display_path`).
fn shown(home: &Path, path: &Path) -> String {
    crate::scan::display_path(path, home).display().to_string()
}

/// A recipe path as the recipe spells it under the home folder, `~/`
/// dropped: `.claude/downloads`.
fn spelled(recipe_path: &'static str) -> &'static Path {
    Path::new(recipe_path.strip_prefix("~/").unwrap_or(recipe_path))
}

/// What `lstat` said about a path: `(st_dev, st_ino)` and the kind -- a
/// link's own, never its target's.
fn identity_of(meta: &std::fs::Metadata) -> ItemIdentity {
    let file_type = meta.file_type();
    let kind = if file_type.is_symlink() {
        ItemKind::Symlink
    } else if file_type.is_dir() {
        ItemKind::Dir
    } else if file_type.is_file() {
        ItemKind::File
    } else {
        ItemKind::Other
    };
    ItemIdentity {
        dev: meta.dev(),
        ino: meta.ino(),
        kind,
    }
}

/// One look at the disk: the home folder resolved, and the route's two
/// paths as the recipe expands them. Taken afresh by `plan_removal` and by
/// every item's turn (`take_turn`), never kept across a pause.
struct Look<'j> {
    job: &'j Job,
    canonical_home: PathBuf,
    launcher: PathBuf,
    root: PathBuf,
}

impl<'j> Look<'j> {
    fn new(job: &'j Job) -> std::io::Result<Look<'j>> {
        let home = job.detected.home.as_path();
        Ok(Look {
            job,
            canonical_home: std::fs::canonicalize(home)?,
            launcher: route::expand(home, job.recipe.route.launcher),
            root: route::expand(home, job.recipe.route.root),
        })
    }
}

/// A kept path that is there, and where it is: `entry` is the path itself
/// with its folder resolved (a link is not followed), `target` where it
/// leads with every link followed (`None` for a link to nothing).
struct Kept {
    spec: &'static KeepSpec,
    path: PathBuf,
    entry: PathBuf,
    target: Option<PathBuf>,
}

/// The kept paths that exist -- a missing one is neither listed nor
/// protected (ruling 6) -- each placed. One that is there but cannot be
/// placed refuses the whole list: Canager could not confirm the moves
/// leave it alone (`OverlapsKept`).
fn kept_places(look: &Look<'_>) -> Result<Vec<Kept>, Refusal> {
    let home = look.job.detected.home.as_path();
    let mut kept = Vec::new();
    for spec in look.job.keep {
        let path = route::expand(home, spec.path);
        match std::fs::symlink_metadata(&path) {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(_) => return Err(Refusal::new(&path, UninstallUnsafeReason::OverlapsKept)),
        }
        let entry = match (path.parent().map(std::fs::canonicalize), path.file_name()) {
            (Some(Ok(folder)), Some(name)) => folder.join(name),
            _ => return Err(Refusal::new(&path, UninstallUnsafeReason::OverlapsKept)),
        };
        let target = match std::fs::canonicalize(&path) {
            Ok(target) => Some(target),
            // A link to nothing: only the link itself is here to keep.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => return Err(Refusal::new(&path, UninstallUnsafeReason::OverlapsKept)),
        };
        kept.push(Kept {
            spec,
            path,
            entry,
            target,
        });
    }
    Ok(kept)
}

/// The kept path that moving the item at `location` would disturb, if
/// any (ruling 25). With every link resolved, the item may lie inside a
/// kept path only where the recipe lists it there -- `~/.claude/downloads`
/// inside `~/.claude`, a real folder -- and may never be a kept path, hold
/// one, or hold what one leads to: the preview says those stay. `location`
/// is where the item itself is (its folders are real folders by then, the
/// ancestry rule), `rel` how the recipe spells it.
fn disturbed<'k>(kept: &'k [Kept], rel: &Path, location: &Path) -> Option<&'k Kept> {
    kept.iter().find(|kept| {
        let kept_rel = spelled(kept.spec.path);
        let target = kept.target.as_deref();
        let takes_it =
            kept.entry.starts_with(location) || target.is_some_and(|t| t.starts_with(location));
        let listed_inside =
            rel.starts_with(kept_rel) && rel != kept_rel && target == Some(kept.entry.as_path());
        let inside_it = target.is_some_and(|t| location.starts_with(t)) && !listed_inside;
        takes_it || inside_it
    })
}

/// Whether `folder` (fully resolved) is the home folder or one of
/// `SHARED_FOLDERS` -- each compared both as the resolved home spells it
/// and where it resolves, so a shared folder kept elsewhere in the home
/// folder through a link (dotfiles) still counts. Check 1's never-list.
fn is_shared_folder(folder: &Path, canonical_home: &Path) -> bool {
    folder == canonical_home
        || SHARED_FOLDERS.iter().any(|name| {
            let shared = canonical_home.join(name);
            folder == shared || std::fs::canonicalize(&shared).is_ok_and(|real| folder == real)
        })
}

/// Every check on one listed path that is there (spec §6.3 checks 1, 3
/// and 4, and rulings 24 and 25), returning what it is. Its last step is
/// the `lstat` whose answer it returns, so a caller that moves the item
/// next has nothing on the disk between the check and the move
/// (`take_turn`). In order:
///
/// - check 1: the folder the path is in, fully resolved, is inside the
///   home folder (`OutsideHome`) and is neither the home folder itself nor
///   a folder many tools share (`SharedFolder`, ruling 5);
/// - the ancestry rule (ruling 24): every folder between the home folder
///   and the path is a real folder, not a link -- the resolved folder is
///   exactly the resolved home joined with the recipe's own spelling.
///   `~/.claude -> ~/Documents` would otherwise make `~/Documents/downloads`
///   Claude Code's cache, and an ancestor renamed away and replaced by a
///   link would carry the move outside the home folder while the item kept
///   its identity (`NotWhatInstructionsExpect`);
/// - kept paths stay kept (ruling 25, `disturbed`; `OverlapsKept`, naming
///   the kept path);
/// - check 4 for a launcher: the one-hop link into the root, or its
///   dangling launcher-only state (`route::probe`);
/// - last, the item's own `lstat`: there (`Missing`), the user's own
///   (check 3, `NotOwnedByYou`), and the kind the instructions describe --
///   a real directory for `Dir`, a link for `SymlinkIntoRoot`, so the item
///   is a link only where the recipe says so (check 4,
///   `NotWhatInstructionsExpect`).
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
    let (Some(folder), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(refuse(NotWhatInstructionsExpect));
    };
    let Ok(real_folder) = std::fs::canonicalize(folder) else {
        return Err(refuse(NotWhatInstructionsExpect));
    };
    if !real_folder.starts_with(&look.canonical_home) {
        return Err(refuse(OutsideHome));
    }
    if is_shared_folder(&real_folder, &look.canonical_home) {
        return Err(refuse(SharedFolder));
    }
    if real_folder
        != look
            .canonical_home
            .join(rel.parent().unwrap_or(Path::new("")))
    {
        return Err(refuse(NotWhatInstructionsExpect));
    }
    if let Some(kept) = disturbed(kept, rel, &real_folder.join(name)) {
        return Err(Refusal::new(&kept.path, OverlapsKept));
    }
    if spec.expect == Expect::SymlinkIntoRoot
        && !matches!(
            route::probe(look.job.recipe.route.kind, path, &look.root),
            Probe::Present { .. } | Probe::LauncherOnly
        )
    {
        return Err(refuse(NotWhatInstructionsExpect));
    }
    let meta = match std::fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Err(refuse(Missing)),
        Err(_) => return Err(refuse(NotWhatInstructionsExpect)),
    };
    if meta.uid() != look.job.detected.euid {
        return Err(refuse(NotOwnedByYou));
    }
    let identity = identity_of(&meta);
    let expected = match spec.expect {
        Expect::Dir => ItemKind::Dir,
        Expect::SymlinkIntoRoot => ItemKind::Symlink,
    };
    if identity.kind != expected {
        return Err(refuse(NotWhatInstructionsExpect));
    }
    Ok(identity)
}

/// Spec §6.3 on every path the recipe lists (check 5, the backup-file
/// patterns, arrives with step D's `backup_globs`), then the kept paths.
/// Check 2 (is it there?) goes first, because the others need something
/// to look at: a missing optional path is skipped, a missing program
/// directory of a launcher-only install is `AlreadyGone` (re-probed from
/// the disk now, ruling 4), anything else missing refuses. Then
/// `check_item`. Any failure refuses the whole list with nothing moved
/// (`AdapterError::UninstallUnsafe`, one of six reasons); an empty list is
/// a plain `Refused` (unreachable while the launcher is listed and the row
/// exists), and so is a home folder that cannot be resolved.
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
    for spec in job.remove {
        let path = route::expand(home, spec.path);
        // Check 2: is it there? `lstat`, so a dangling launcher counts.
        match std::fs::symlink_metadata(&path) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                if spec.optional {
                    continue;
                }
                if launcher_only && path != look.launcher {
                    warnings.push(Warning::AlreadyGone {
                        path: shown(home, &path),
                    });
                    continue;
                }
                return Err(Refusal::new(&path, UninstallUnsafeReason::Missing).into_error(home));
            }
            // Unreadable (a permission error, a loop): not something the
            // instructions describe, and not something to move blind.
            Err(_) => {
                return Err(
                    Refusal::new(&path, UninstallUnsafeReason::NotWhatInstructionsExpect)
                        .into_error(home),
                )
            }
        }
        let identity =
            check_item(&look, &kept, spec, &path).map_err(|refusal| refusal.into_error(home))?;
        warnings.push(Warning::WillTrash {
            path: shown(home, &path),
            what: spec.what,
        });
        identities.push(identity);
        paths.push(path);
    }
    if paths.is_empty() {
        return Err(AdapterError::Refused(format!(
            "{}: nothing on the uninstall list is there to move",
            job.recipe.id
        )));
    }
    warnings.extend(kept.iter().map(|kept| Warning::WillKeep {
        path: shown(home, &kept.path),
        what: kept.spec.what,
    }));
    Ok(Removal {
        paths,
        identities,
        warnings,
    })
}

```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p banager-core --lib adapters::standalone::removal`, `cargo test -p banager-core --lib adapters::standalone::route` and `cargo test -p banager-core --lib scan`
Expected: PASS — the 13 removal tests; B's route tests (24 at `cf7a955`, B's review having added two `shadow_note` tests) and the 3 new ones (B's detection, inventory and upgrade tests elsewhere in the crate are untouched by the one-hop rule: every layout they build links straight into its root, and `tests/ops_upgrade_version_test.rs`'s `claude_home` spells that link through the non-canonical `/var/folders`, which `one_hop` resolves component by component); F's `test_display_path_abbreviates_home_and_only_home` unchanged. The same code and these 16 tests were run in a scratch mirror of the crate's module paths (stubs for the rest), with `cargo clippy --all-targets -- -D warnings` clean. (`plan_removal` has no production caller until stage 6e; it is `pub`, so no dead-code warning, and the gates run at 6h.)

- [ ] **Step 5: Continue the task**

No commit: continue to stage 6d.

#### Stage 6d: `removal::execute_removal` — the fresh look against the preview, each item's turn on the blocking pool, the log, the pacing

- [ ] **Step 1: Write the failing tests**

In `crates/banager-core/src/adapters/standalone/removal.rs`, add to the test module's `use` lines:

```rust
    use crate::events::VecSink;
    use crate::trash::{MockTrasher, TrashError};
    use std::sync::Mutex as StdMutex;
```

and append before the test module's closing `}`:

```rust

    /// A run of `execute_removal` over `job` with what the preview
    /// `removal` found, through a `VecSink`, returning the outcome and the
    /// notes written.
    async fn run(
        job: &Job,
        removal: &Removal,
        trasher: &Arc<dyn Trasher>,
        pacing: Pacing,
        cancel: CancellationToken,
    ) -> (Outcome, Vec<LogNote>) {
        let sink = Arc::new(VecSink::new());
        let confirmed = Confirmed {
            paths: &removal.paths,
            previewed: &removal.identities,
        };
        let outcome = execute_removal(job, confirmed, trasher, pacing, sink.clone(), 9, cancel)
            .await
            .expect("execute_removal");
        let notes = sink
            .snapshot()
            .into_iter()
            .filter_map(|event| match event {
                OperationEvent::Note { op_id: 9, note } => Some(note),
                _ => None,
            })
            .collect();
        (outcome, notes)
    }

    fn no_gap() -> Pacing {
        Pacing {
            settle: Duration::ZERO,
            budget: Duration::from_secs(TIMEOUT_SECS),
        }
    }

    fn moved(path: &str, to: &Path) -> LogNote {
        LogNote::MovedToTrash {
            path: path.to_string(),
            trashed_to: to.display().to_string(),
        }
    }

    fn path_changed(path: &str) -> Outcome {
        Outcome::CanagerFailed(Fault::PathChanged {
            path: path.to_string(),
        })
    }

    #[tokio::test]
    async fn test_execute_removal_moves_each_path_in_order_and_notes_each() {
        let home = TempHome::new("removal-exec-full");
        let layout = claude_layout(&home, "2.1.281");
        home.dir(".claude/downloads");
        home.file(".claude.json");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let mock = Arc::new(MockTrasher::new());
        let trasher: Arc<dyn Trasher> = mock.clone();

        let (outcome, notes) =
            run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;

        assert_eq!(outcome, Outcome::Succeeded);
        assert_eq!(mock.calls(), preview.paths);
        // Each item is handed over as what its last check saw: the launcher
        // as a link, never as a folder a URL's trailing slash could enter.
        assert_eq!(
            mock.kinds(),
            vec![ItemKind::Dir, ItemKind::Dir, ItemKind::Symlink]
        );
        // Both `claude`s are in the bin: the directory under its own name,
        // the link -- moved as a link -- under a suffixed one (the mock
        // suffixes with the call index; the system with the time).
        assert!(mock.bin().join("claude/versions/2.1.281").is_file());
        assert!(mock.bin().join("downloads").is_dir());
        let link = mock.bin().join("claude 2");
        assert!(std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(std::fs::symlink_metadata(&layout.launcher).is_err());
        assert!(home.path().join(".claude.json").is_file(), "kept");
        assert!(home.path().join(".claude").is_dir(), "kept");
        assert_eq!(
            notes,
            vec![
                moved("~/.local/share/claude", &mock.bin().join("claude")),
                moved("~/.claude/downloads", &mock.bin().join("downloads")),
                moved("~/.local/bin/claude", &link),
            ]
        );
    }

    #[tokio::test]
    async fn test_execute_removal_refuses_when_the_fresh_list_differs_from_the_preview() {
        // The user confirmed two paths; by run time the updater has made
        // `~/.claude/downloads`. Not the list they saw: stop before the
        // first move, naming the path that appeared.
        let home = TempHome::new("removal-exec-list-grew");
        let _layout = claude_layout(&home, "2.1.281");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        home.dir(".claude/downloads");
        let mock = Arc::new(MockTrasher::new());
        let trasher: Arc<dyn Trasher> = mock.clone();

        let (outcome, notes) =
            run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;

        assert_eq!(outcome, path_changed("~/.claude/downloads"));
        assert!(mock.calls().is_empty());
        assert!(notes.is_empty());

        // And the other way round: a path the user confirmed is gone.
        let preview = plan_removal(&job).unwrap();
        std::fs::remove_dir(home.path().join(".claude/downloads")).unwrap();
        let (outcome, _) = run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;
        assert_eq!(outcome, path_changed("~/.claude/downloads"));
        assert!(mock.calls().is_empty());
    }

    #[tokio::test]
    async fn test_execute_removal_refuses_when_a_check_now_fails() {
        // The launcher was re-pointed elsewhere after the preview: check 4
        // fails at run time, and that is a changed path, not a refused
        // plan. Nothing is moved -- the program directory is first on the
        // list and would have passed.
        let home = TempHome::new("removal-exec-check-fails");
        let layout = claude_layout(&home, "2.1.281");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let elsewhere = home.executable("elsewhere/claude");
        std::fs::remove_file(&layout.launcher).unwrap();
        std::os::unix::fs::symlink(elsewhere, &layout.launcher).unwrap();
        let mock = Arc::new(MockTrasher::new());
        let trasher: Arc<dyn Trasher> = mock.clone();

        let (outcome, _) = run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;

        assert_eq!(outcome, path_changed("~/.local/bin/claude"));
        assert!(mock.calls().is_empty());
        assert!(
            layout.root.join("versions/2.1.281").is_file(),
            "nothing moved"
        );
    }

    #[tokio::test]
    async fn test_execute_removal_refuses_what_the_preview_did_not_see() {
        // Spec §6.3 (ruling 10): what the preview saw travels with the plan.
        // Claude Code's updater re-points the launcher at a new version
        // inside the root -- the same shape, every check still passes, and
        // the same path string -- but it is a new link, not the one the user
        // looked at: stop before anything moves, and the user previews
        // again. The same for a cache folder replaced by a new one.
        let home = TempHome::new("removal-exec-self-update");
        let layout = claude_layout(&home, "2.1.281");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let newer = home.executable(".local/share/claude/versions/2.1.282");
        std::fs::remove_file(&layout.launcher).unwrap();
        std::os::unix::fs::symlink(newer, &layout.launcher).unwrap();
        assert!(plan_removal(&job).is_ok(), "every check still passes");
        let mock = Arc::new(MockTrasher::new());
        let trasher: Arc<dyn Trasher> = mock.clone();

        let (outcome, notes) =
            run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;

        assert_eq!(outcome, path_changed("~/.local/bin/claude"));
        assert!(mock.calls().is_empty());
        assert!(notes.is_empty());
        assert!(
            layout.root.join("versions/2.1.281").is_file(),
            "nothing moved"
        );

        let home = TempHome::new("removal-exec-cache-replaced");
        let _layout = claude_layout(&home, "2.1.281");
        let downloads = home.dir(".claude/downloads");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        // Made while the old one still exists, so it cannot get its inode.
        let old = home.path().join(".claude/downloads.old");
        std::fs::rename(&downloads, &old).unwrap();
        std::fs::create_dir(&downloads).unwrap();
        std::fs::remove_dir(&old).unwrap();

        let (outcome, _) = run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;

        assert_eq!(outcome, path_changed("~/.claude/downloads"));
        assert!(mock.calls().is_empty());
    }

    #[tokio::test]
    async fn test_execute_removal_refuses_a_settings_folder_linked_to_the_program_folder_after_the_preview(
    ) {
        // Ruling 25 at run time: between the preview and the click,
        // `~/.claude` became a link to `~/.local/share/claude`. The fresh
        // look finds the kept folder inside what the first move would take,
        // and stops before anything moves, naming the kept path.
        let home = TempHome::new("removal-exec-settings-aliased");
        let layout = claude_layout(&home, "2.1.281");
        home.file(".claude/projects/p/session.jsonl");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        std::fs::rename(
            home.path().join(".claude"),
            layout.root.join("moved-settings"),
        )
        .unwrap();
        home.link(".claude", &layout.root);
        let mock = Arc::new(MockTrasher::new());
        let trasher: Arc<dyn Trasher> = mock.clone();

        let (outcome, _) = run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;

        assert_eq!(outcome, path_changed("~/.claude"));
        assert!(mock.calls().is_empty());
    }

    /// A trasher that, while moving its first item, replaces the cache
    /// folder with a fresh one of the same name: a change during the pause,
    /// before the next item's check.
    struct ReplacingTrasher {
        inner: MockTrasher,
        replace: PathBuf,
        done: StdMutex<bool>,
    }

    impl Trasher for ReplacingTrasher {
        fn trash(&self, path: &Path, kind: ItemKind) -> Result<PathBuf, TrashError> {
            let result = self.inner.trash(path, kind);
            let mut done = self.done.lock().unwrap();
            if !*done {
                *done = true;
                // The new directory is made while the old one still exists,
                // so it cannot be given the old inode number.
                let old = self.replace.with_extension("old");
                std::fs::rename(&self.replace, &old).unwrap();
                std::fs::create_dir(&self.replace).unwrap();
                std::fs::remove_dir_all(&old).unwrap();
            }
            result
        }
    }

    #[tokio::test]
    async fn test_execute_removal_catches_a_substitution_before_an_items_check() {
        // Ruling 26's boundary, first half: a same-name replacement that
        // happens before an item's turn is caught by that turn's check --
        // the cache folder passed every check at the confirmation, but the
        // one about to be moved is another inode. The trasher never gets
        // it; the program directory, moved first, stays in the Trash; the
        // launcher, last, stays in place.
        let home = TempHome::new("removal-exec-replaced");
        let layout = claude_layout(&home, "2.1.281");
        let downloads = home.dir(".claude/downloads");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let replacing = Arc::new(ReplacingTrasher {
            inner: MockTrasher::new(),
            replace: downloads,
            done: StdMutex::new(false),
        });
        let trasher: Arc<dyn Trasher> = replacing.clone();

        let (outcome, notes) =
            run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;

        assert_eq!(outcome, path_changed("~/.claude/downloads"));
        assert_eq!(replacing.inner.calls(), preview.paths[..1].to_vec());
        assert_eq!(notes.len(), 1, "the program directory's move was logged");
        assert!(std::fs::symlink_metadata(&layout.launcher)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(std::fs::symlink_metadata(&layout.root).is_err(), "moved");
    }

    /// A trasher that swaps the item it is handed for another folder of the
    /// same name at the start of its second call, then moves what it finds:
    /// a substitution after that item's last check, inside the move itself.
    struct SwappingTrasher {
        inner: MockTrasher,
        calls: StdMutex<usize>,
    }

    impl Trasher for SwappingTrasher {
        fn trash(&self, path: &Path, kind: ItemKind) -> Result<PathBuf, TrashError> {
            let nth = {
                let mut calls = self.calls.lock().unwrap();
                *calls += 1;
                *calls - 1
            };
            if nth == 1 {
                std::fs::rename(path, path.with_extension("checked")).unwrap();
                std::fs::create_dir(path).unwrap();
                std::fs::write(path.join("substitute"), b"not what was checked").unwrap();
            }
            self.inner.trash(path, kind)
        }
    }

    #[tokio::test]
    async fn test_a_substitution_inside_the_move_itself_is_beyond_the_last_check() {
        // Ruling 26's boundary, second half, pinned so the documentation
        // stays true: the last check is immediately before the call, and
        // the system's call takes a path, so an item swapped between the
        // two -- here, inside the call -- is what gets moved, and the run
        // cannot tell. Out of scope by design (the threat model is change by
        // accident, and a program running as the user can do all Canager
        // can); docs/what-we-run.md says so in one sentence. If this ever
        // fails because the gap was closed, change that sentence too.
        let home = TempHome::new("removal-exec-swapped-in-call");
        let _layout = claude_layout(&home, "2.1.281");
        let downloads = home.dir(".claude/downloads");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let swapping = Arc::new(SwappingTrasher {
            inner: MockTrasher::new(),
            calls: StdMutex::new(0),
        });
        let trasher: Arc<dyn Trasher> = swapping.clone();

        let (outcome, _) = run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;

        assert_eq!(outcome, Outcome::Succeeded);
        assert!(swapping.inner.bin().join("downloads/substitute").is_file());
        assert!(
            downloads.with_extension("checked").is_dir(),
            "the checked one stayed"
        );
    }

    /// A trasher that, after moving its first item (the program folder),
    /// renames `~/.claude` to a folder outside the home folder on the same
    /// volume and leaves a link to it in its place: the cache inside keeps
    /// its inode, and only its folders changed.
    struct RelocatingTrasher {
        inner: MockTrasher,
        folder: PathBuf,
        away: PathBuf,
        done: StdMutex<bool>,
    }

    impl Trasher for RelocatingTrasher {
        fn trash(&self, path: &Path, kind: ItemKind) -> Result<PathBuf, TrashError> {
            let result = self.inner.trash(path, kind);
            let mut done = self.done.lock().unwrap();
            if !*done {
                *done = true;
                std::fs::rename(&self.folder, &self.away).unwrap();
                std::os::unix::fs::symlink(&self.away, &self.folder).unwrap();
            }
            result
        }
    }

    #[tokio::test]
    async fn test_execute_removal_checks_an_items_folders_again_after_the_pause() {
        // Ruling 24 at run time, the review's third counterexample: during
        // the pause after the first move, `~/.claude` is renamed away out of
        // the home folder and replaced by a link to where it went. The
        // cache's own `(st_dev, st_ino)` did not change -- an identity check
        // alone would move a folder outside the home folder -- but its turn
        // runs every check again, and its folder now leads outside home.
        let home = TempHome::new("removal-exec-ancestor-moved");
        let layout = claude_layout(&home, "2.1.281");
        home.dir(".claude/downloads");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let elsewhere = TempHome::new("removal-exec-ancestor-away");
        let away = elsewhere.path().join("claude-settings");
        let relocating = Arc::new(RelocatingTrasher {
            inner: MockTrasher::new(),
            folder: home.path().join(".claude"),
            away: away.clone(),
            done: StdMutex::new(false),
        });
        let trasher: Arc<dyn Trasher> = relocating.clone();

        let (outcome, _) = run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;

        assert_eq!(outcome, path_changed("~/.claude/downloads"));
        assert_eq!(
            identity(&away.join("downloads")),
            preview.identities[1],
            "the leaf kept its identity; only a folder above it changed"
        );
        assert_eq!(relocating.inner.calls(), preview.paths[..1].to_vec());
        assert!(away.join("downloads").is_dir(), "not moved");
        assert!(std::fs::symlink_metadata(&layout.launcher)
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[tokio::test]
    async fn test_execute_removal_stops_at_a_refused_item_with_the_systems_words() {
        // macOS refused the second item: the outcome is `Failed` with its
        // words as the summary (quoted by the front end like a tool's
        // stderr), the first item is in the Trash, the launcher is not.
        let home = TempHome::new("removal-exec-refused");
        let layout = claude_layout(&home, "2.1.281");
        home.dir(".claude/downloads");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let mock = Arc::new(MockTrasher::new());
        mock.refuse_call(1, "Operation not permitted");
        let trasher: Arc<dyn Trasher> = mock.clone();

        let (outcome, notes) =
            run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;

        assert_eq!(
            outcome,
            Outcome::Failed {
                exit_code: None,
                summary: "Operation not permitted".to_string()
            }
        );
        assert_eq!(mock.calls(), preview.paths[..2].to_vec());
        assert!(std::fs::symlink_metadata(&layout.launcher)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            notes,
            vec![
                moved("~/.local/share/claude", &mock.bin().join("claude")),
                LogNote::TrashFailed {
                    path: "~/.claude/downloads".to_string(),
                    error: "Operation not permitted".to_string(),
                },
            ]
        );
    }

    #[tokio::test]
    async fn test_execute_removal_stops_between_items_when_cancelled() {
        // Cancel while the first item's move is being reported: that move
        // is logged, and the run stops before the next item --
        // `Unconfirmed`, for `run_operation` to reconcile. The program
        // directory is in the Trash, the launcher still there, which is the
        // launcher-only state.
        let home = TempHome::new("removal-exec-cancel");
        let layout = claude_layout(&home, "2.1.281");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let token = CancellationToken::new();
        let mock = Arc::new(MockTrasher::new());
        mock.cancel_after_call(0, token.clone());
        let trasher: Arc<dyn Trasher> = mock.clone();

        let (outcome, notes) = run(&job, &preview, &trasher, no_gap(), token).await;

        assert_eq!(outcome, Outcome::Unconfirmed);
        assert_eq!(mock.calls(), preview.paths[..1].to_vec());
        assert_eq!(notes.len(), 1);
        assert!(std::fs::symlink_metadata(&layout.launcher)
            .unwrap()
            .file_type()
            .is_symlink());
    }

    /// A trasher whose moves take `delay` (a slow Trash), for the test that
    /// presses Cancel in the middle of one.
    struct SlowTrasher {
        inner: MockTrasher,
        delay: Duration,
    }

    impl Trasher for SlowTrasher {
        fn trash(&self, path: &Path, kind: ItemKind) -> Result<PathBuf, TrashError> {
            std::thread::sleep(self.delay);
            self.inner.trash(path, kind)
        }
    }

    #[tokio::test]
    async fn test_execute_removal_finishes_a_move_under_way_when_cancel_arrives() {
        // Ruling 28: a move already handed to the system is not abandoned.
        // Cancel arrives 50 ms into a 300 ms move; the run waits for it,
        // logs it, and only then stops -- nothing is still moving on another
        // thread when `execute_removal` reports.
        let home = TempHome::new("removal-exec-cancel-mid-move");
        let _layout = claude_layout(&home, "2.1.281");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let slow = Arc::new(SlowTrasher {
            inner: MockTrasher::new(),
            delay: Duration::from_millis(300),
        });
        let trasher: Arc<dyn Trasher> = slow.clone();
        let token = CancellationToken::new();
        let pressed = token.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            pressed.cancel();
        });
        let started = Instant::now();

        let (outcome, notes) = run(&job, &preview, &trasher, no_gap(), token).await;

        assert_eq!(outcome, Outcome::Unconfirmed);
        assert!(
            started.elapsed() >= Duration::from_millis(300),
            "{:?}",
            started.elapsed()
        );
        assert_eq!(slow.inner.calls(), preview.paths[..1].to_vec());
        assert_eq!(
            notes,
            vec![moved(
                "~/.local/share/claude",
                &slow.inner.bin().join("claude")
            )]
        );
    }

    #[tokio::test]
    async fn test_execute_removal_stops_before_an_item_when_the_budget_is_spent() {
        let home = TempHome::new("removal-exec-budget");
        let _layout = claude_layout(&home, "2.1.281");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let mock = Arc::new(MockTrasher::new());
        let trasher: Arc<dyn Trasher> = mock.clone();
        let spent = Pacing {
            settle: Duration::ZERO,
            budget: Duration::ZERO,
        };

        let (outcome, notes) = run(&job, &preview, &trasher, spent, CancellationToken::new()).await;

        assert_eq!(outcome, Outcome::Unconfirmed);
        assert!(mock.calls().is_empty());
        assert!(notes.is_empty());
    }

    #[tokio::test]
    async fn test_execute_removal_cuts_a_pause_to_what_is_left_of_the_budget() {
        // Ruling 28: no pause outlasts the budget. A 5 s pause with 1 s of
        // budget left ends when the budget does, and the run stops before
        // the next item.
        let home = TempHome::new("removal-exec-pause-budget");
        let _layout = claude_layout(&home, "2.1.281");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let mock = Arc::new(MockTrasher::new());
        let trasher: Arc<dyn Trasher> = mock.clone();
        let tight = Pacing {
            settle: Duration::from_secs(5),
            budget: Duration::from_secs(1),
        };
        let started = Instant::now();

        let (outcome, _) = run(&job, &preview, &trasher, tight, CancellationToken::new()).await;

        assert_eq!(outcome, Outcome::Unconfirmed);
        assert_eq!(mock.calls().len(), 1);
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "{:?}",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn test_execute_removal_waits_the_settle_gap_after_each_item() {
        // Author decision 1: a pause after each move (Finder's Put Back
        // record) -- before the next one, and after the last before the run
        // is reported finished -- and none before the first.
        let home = TempHome::new("removal-exec-paced");
        let _layout = claude_layout(&home, "2.1.281");
        home.dir(".claude/downloads");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let trasher: Arc<dyn Trasher> = Arc::new(MockTrasher::new());
        let paced = Pacing {
            settle: Duration::from_millis(100),
            budget: Duration::from_secs(TIMEOUT_SECS),
        };
        let started = Instant::now();

        let (outcome, _) = run(&job, &preview, &trasher, paced, CancellationToken::new()).await;

        assert_eq!(outcome, Outcome::Succeeded);
        assert!(
            started.elapsed() >= Duration::from_millis(300),
            "three pauses for three items -- two between them, one after the last: {:?}",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn test_execute_removal_ends_the_settle_wait_at_a_cancel() {
        // The pause is watched for Cancel. It is long enough here that
        // finishing quickly proves the cancel ended it, not the timer.
        let home = TempHome::new("removal-exec-gap-cancel");
        let _layout = claude_layout(&home, "2.1.281");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let token = CancellationToken::new();
        let mock = Arc::new(MockTrasher::new());
        mock.cancel_after_call(0, token.clone());
        let trasher: Arc<dyn Trasher> = mock.clone();
        let gap = Pacing {
            settle: Duration::from_secs(5),
            budget: Duration::from_secs(TIMEOUT_SECS),
        };
        let started = Instant::now();

        let (outcome, _) = run(&job, &preview, &trasher, gap, token).await;

        assert_eq!(outcome, Outcome::Unconfirmed);
        assert_eq!(mock.calls().len(), 1);
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "the cancel ended the 5 s wait: {:?}",
            started.elapsed()
        );
    }

    /// A trasher that cannot ask the system at all: what `RealTrasher`
    /// answers off macOS, or for a path that is not UTF-8.
    struct UnsupportedTrasher;

    impl Trasher for UnsupportedTrasher {
        fn trash(&self, _path: &Path, _kind: ItemKind) -> Result<PathBuf, TrashError> {
            Err(TrashError::Unsupported)
        }
    }

    #[tokio::test]
    async fn test_execute_removal_reports_a_trasher_that_cannot_ask_as_canagers_own_failure() {
        // Ruling 9: Canager's own limitation is `Fault::Internal` -- never a
        // `Failed` whose summary the front end would quote as the Mac's
        // words, and never a `TrashFailed` note. Nothing was moved.
        let home = TempHome::new("removal-exec-unsupported");
        let layout = claude_layout(&home, "2.1.281");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let trasher: Arc<dyn Trasher> = Arc::new(UnsupportedTrasher);

        let (outcome, notes) =
            run(&job, &preview, &trasher, no_gap(), CancellationToken::new()).await;

        assert_eq!(outcome, Outcome::CanagerFailed(Fault::Internal));
        assert!(notes.is_empty());
        assert!(
            layout.root.join("versions/2.1.281").is_file(),
            "nothing moved"
        );
    }

    #[tokio::test]
    async fn test_execute_removal_refuses_a_plan_that_lost_what_its_preview_saw() {
        // A `TrashPaths` plan read back from JSON carries no identities
        // (`previewed` is skipped by serde); one without them is a bug in
        // Canager, refused before anything moves.
        let home = TempHome::new("removal-exec-no-preview");
        let layout = claude_layout(&home, "2.1.281");
        let d = detected(home.path());
        let job = claude_job(&d);
        let preview = plan_removal(&job).unwrap();
        let mock = Arc::new(MockTrasher::new());
        let trasher: Arc<dyn Trasher> = mock.clone();

        let result = execute_removal(
            &job,
            Confirmed {
                paths: &preview.paths,
                previewed: &[],
            },
            &trasher,
            no_gap(),
            Arc::new(VecSink::new()),
            9,
            CancellationToken::new(),
        )
        .await;

        assert!(
            matches!(result, Err(AdapterError::Refused(_))),
            "{result:?}"
        );
        assert!(mock.calls().is_empty());
        assert!(layout.root.is_dir());
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p banager-core --lib adapters::standalone::removal`
Expected: FAIL to compile — `cannot find function \`execute_removal\` in this scope`; `cannot find struct \`Confirmed\``, `cannot find type \`Pacing\``; `cannot find value \`TIMEOUT_SECS\``; `cannot find trait \`Trasher\``, `cannot find type \`CancellationToken\``, `\`LogNote\``, `\`Outcome\`` (the parent does not import them yet).

- [ ] **Step 3: Write `execute_removal`**

In `crates/banager-core/src/adapters/standalone/removal.rs`, replace the line

```rust
use crate::model::{ItemIdentity, ItemKind, UninstallUnsafeReason, Warning};
```

with

```rust
use crate::events::{EventSink, LogNote, OpId, OperationEvent};
use crate::model::{Fault, ItemIdentity, ItemKind, Outcome, UninstallUnsafeReason, Warning};
use crate::trash::{TrashError, Trasher};
```

and after `use std::path::{Path, PathBuf};` add

```rust
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
```

Then insert, directly after the imports (before `/// One tool's removal as the checks see it`):

```rust

/// `Plan.timeout_secs` for a path-list uninstall. There is no process for
/// the runner to time out; `execute_removal` keeps its own clock against
/// this (through `Pacing.budget`) and stops between items once it is spent
/// -- before an item, never during one: a move already handed to the
/// system is waited for. The moves take milliseconds; the budget covers the
/// pauses below and a Trash that is slow to answer. Read by
/// `StandaloneAdapter::plan`.
pub const TIMEOUT_SECS: u64 = 120;

/// The pause after each item: before the next one, and after the last
/// before the run is reported finished. From a process without Full Disk
/// Access -- a Finder-launched Canager -- Finder wrote its "Put Back"
/// record for only the first of a burst of `trashItemAtURL:` calls up to
/// 1.5 s apart, and for every item when they were 2 s or more apart
/// (2026-09-25, one Mac, macOS 27.0: 15/15 and 4/4 runs; the mechanism is
/// not known). Those 4 runs also stayed alive 3 s after their last call,
/// and the record is written after the call returns (with Full Disk
/// Access, a process that exited at once lost the later records): hence
/// the same pause after the last item. Three seconds is the largest gap
/// measured to work, a full second above the largest that failed: it makes
/// Put Back likely for every item, not certain -- an item without the
/// record can still be dragged back out of the Trash -- and
/// `docs/what-we-run.md` says so. Each pause is cut short by Cancel and by
/// what is left of `TIMEOUT_SECS`. Read by `StandaloneAdapter::new`;
/// `with_trash_gap` sets it to zero for tests.
pub const PUT_BACK_SETTLE: Duration = Duration::from_secs(3);
```

after `Removal`'s closing `}` (before `/// A path that fails a check, and which one.`):

```rust
/// What the user confirmed: the plan's paths in order, and what the
/// preview saw at each (`PlanAction::TrashPaths`). Built by
/// `StandaloneAdapter::execute`; read by `execute_removal`.
#[derive(Clone, Copy, Debug)]
pub struct Confirmed<'a> {
    pub paths: &'a [PathBuf],
    pub previewed: &'a [ItemIdentity],
}

/// How `execute_removal` paces itself: the pause after each item
/// (`PUT_BACK_SETTLE` in production, zero in tests) and the budget it
/// stops between items once spent (`Plan.timeout_secs`). Built by
/// `StandaloneAdapter::execute`.
#[derive(Clone, Copy, Debug)]
pub struct Pacing {
    pub settle: Duration,
    pub budget: Duration,
}

```

after `shown`'s closing `}` (before `/// A recipe path as the recipe spells it under the home folder`):

```rust
/// A run that stopped at `path` because it is not what was confirmed.
fn changed(home: &Path, path: &Path) -> Outcome {
    Outcome::CanagerFailed(Fault::PathChanged {
        path: shown(home, path),
    })
}

```

and after `plan_removal`'s closing `}` (before `#[cfg(test)]`):

```rust
/// The first path the two lists disagree on: one that appeared since the
/// preview, else one that disappeared, else the first out of order.
fn first_difference<'a>(planned: &'a [PathBuf], fresh: &'a [PathBuf]) -> Option<&'a PathBuf> {
    fresh
        .iter()
        .find(|path| !planned.contains(path))
        .or_else(|| planned.iter().find(|path| !fresh.contains(path)))
        .or_else(|| {
            planned
                .iter()
                .zip(fresh)
                .find(|(a, b)| a != b)
                .map(|(a, _)| a)
        })
}

/// Waits `gap` -- Finder's time to write the Put Back record of the item
/// just moved (`PUT_BACK_SETTLE`), already cut to what is left of the
/// budget -- or until `cancel` fires, whichever comes first; `true` when it
/// was the cancel. No wait at all for a zero gap (tests,
/// `StandaloneAdapter::with_trash_gap`, a spent budget).
async fn pause(gap: Duration, cancel: &CancellationToken) -> bool {
    if gap.is_zero() {
        return false;
    }
    tokio::select! {
        biased;
        _ = cancel.cancelled() => true,
        _ = tokio::time::sleep(gap) => false,
    }
}

/// How one item's turn ended (`take_turn`).
enum Turn {
    /// Moved; where the system put it.
    Moved(PathBuf),
    /// Not what the preview saw: a check failed at `.0` (the item, or the
    /// kept path it would disturb) or the item's identity differs. Not
    /// moved.
    Changed(PathBuf),
    /// The system refused; its own words. Not moved.
    Refused(String),
    /// Canager could not ask the system at all (`TrashError::Unsupported`).
    CannotAsk,
}

/// One item's turn, on tokio's blocking pool: every check again, from a
/// fresh look (the home folder, the kept paths, the item's folders, the
/// item itself), its identity compared with what the preview saw, and then
/// -- with nothing in between -- the move. `check_item`'s last step is the
/// `lstat` that produced `seen`, and `Trasher::trash` is handed that
/// answer's kind rather than looking again: Canager checks each item
/// immediately before moving it; a program running as you that swaps the
/// item in that instant could still race it (docs/what-we-run.md, "Moving
/// files to the Trash"). That is the documented edge of the design: the
/// system's call takes a path, and what it finds there is what it moves.
fn take_turn(job: &Job, path: &Path, previewed: ItemIdentity, trasher: &dyn Trasher) -> Turn {
    let home = job.detected.home.as_path();
    let Some(spec) = job
        .remove
        .iter()
        .find(|spec| route::expand(home, spec.path) == path)
    else {
        return Turn::Changed(path.to_path_buf());
    };
    let Ok(look) = Look::new(job) else {
        return Turn::Changed(path.to_path_buf());
    };
    let seen = match kept_places(&look).and_then(|kept| check_item(&look, &kept, spec, path)) {
        Ok(seen) => seen,
        Err(refusal) => return Turn::Changed(refusal.path),
    };
    if seen != previewed {
        return Turn::Changed(path.to_path_buf());
    }
    match trasher.trash(path, seen.kind) {
        Ok(trashed_to) => Turn::Moved(trashed_to),
        Err(TrashError::Refused { detail }) => Turn::Refused(detail),
        Err(TrashError::Unsupported) => Turn::CannotAsk,
    }
}

/// Spec §6.2's execution. First the fresh look at the confirmation: every
/// check again on the list rebuilt from the disk (`plan_removal`), the
/// list compared with the confirmed one, and every item's identity
/// compared with the one the preview recorded (spec §6.3) -- any
/// difference is `Fault::PathChanged` before anything moves, so a Claude
/// Code that updated itself between the preview and the click (its updater
/// re-points the launcher) sends the user back to a fresh preview. Then
/// each path in order: after the first, the pause (Cancel ends it, and it
/// never outlasts the budget); Cancel and the budget checked; one turn on
/// the blocking pool (`take_turn`: every check again, the identity against
/// the preview's, the move), awaited to its end even if Cancel arrives
/// meanwhile -- a move handed to the system finishes and is reported; one
/// log note. After the last move the same pause once more, before
/// `Succeeded` (a Cancel there only cuts it short: everything is moved).
///
/// `Succeeded` only when every path was moved; `Failed` with the system's
/// own words when it refused one (the launcher, last, is then still there,
/// and the row comes back as launcher-only); `CanagerFailed(Internal)`
/// when Canager could not ask the system at all (`TrashError::Unsupported`,
/// at the first item); `Unconfirmed` when cancelled or out of time between
/// items, or when a turn panicked -- `run_operation` then reads the disk
/// and reports what it finds. Never an `Err` for a state of the Mac: the
/// `Err` arm is a bug's (a home folder that cannot be resolved, an empty
/// list, a plan that lost what its preview saw). Read by
/// `StandaloneAdapter::execute`.
pub async fn execute_removal(
    job: &Job,
    confirmed: Confirmed<'_>,
    trasher: &Arc<dyn Trasher>,
    pacing: Pacing,
    sink: Arc<dyn EventSink>,
    op_id: OpId,
    cancel: CancellationToken,
) -> Result<Outcome, AdapterError> {
    let home = job.detected.home.as_path();
    let started = Instant::now();
    let left = || pacing.budget.saturating_sub(started.elapsed());
    // What the preview saw travels with the plan, one identity per path;
    // a plan without it (built by hand, or read back from JSON, which
    // never carries it) has nothing to compare with, and moving anyway
    // would move what nobody looked at.
    if confirmed.previewed.len() != confirmed.paths.len() {
        return Err(AdapterError::Refused(format!(
            "{}: the plan does not carry what its preview saw",
            job.recipe.id
        )));
    }
    let fresh = match plan_removal(job) {
        Ok(fresh) => fresh,
        // A check that passed at preview time fails now: something at the
        // listed path changed. Not a refusal of a plan (that plan was
        // issued and confirmed) but a run that stopped before moving it.
        Err(AdapterError::UninstallUnsafe { path, .. }) => {
            return Ok(Outcome::CanagerFailed(Fault::PathChanged { path }));
        }
        Err(other) => return Err(other),
    };
    if let Some(path) = first_difference(confirmed.paths, &fresh.paths) {
        return Ok(changed(home, path));
    }
    if let Some((path, _)) = confirmed
        .paths
        .iter()
        .zip(confirmed.previewed.iter().zip(&fresh.identities))
        .find(|(_, (then, now))| then != now)
    {
        return Ok(changed(home, path));
    }
    for (index, (path, &previewed)) in confirmed.paths.iter().zip(confirmed.previewed).enumerate() {
        if index > 0 && pause(pacing.settle.min(left()), &cancel).await {
            return Ok(Outcome::Unconfirmed);
        }
        if cancel.is_cancelled() || left().is_zero() {
            return Ok(Outcome::Unconfirmed);
        }
        let turn = {
            let (job, path, trasher) = (job.clone(), path.clone(), Arc::clone(trasher));
            tokio::task::spawn_blocking(move || take_turn(&job, &path, previewed, trasher.as_ref()))
                .await
        };
        match turn {
            Ok(Turn::Moved(trashed_to)) => sink.emit(OperationEvent::Note {
                op_id,
                note: LogNote::MovedToTrash {
                    path: shown(home, path),
                    trashed_to: shown(home, &trashed_to),
                },
            }),
            Ok(Turn::Changed(at)) => return Ok(changed(home, &at)),
            Ok(Turn::Refused(detail)) => {
                sink.emit(OperationEvent::Note {
                    op_id,
                    note: LogNote::TrashFailed {
                        path: shown(home, path),
                        error: detail.clone(),
                    },
                });
                // macOS's own words, quoted by the front end as a tool's
                // stderr would be. Whatever was moved before is in the
                // Trash; the launcher (last) is not, and the row comes back.
                return Ok(Outcome::Failed {
                    exit_code: None,
                    summary: detail,
                });
            }
            // Canager could not ask the system at all (not macOS, or a path
            // `NSString` cannot carry): its own limitation, with no words of
            // the Mac's to quote (ruling 9). `RealTrasher` answers it for
            // every path alike, so it comes at the first item, before
            // anything moved.
            Ok(Turn::CannotAsk) => return Ok(Outcome::CanagerFailed(Fault::Internal)),
            // The turn panicked: whether this item moved is not known.
            Err(_) => return Ok(Outcome::Unconfirmed),
        }
    }
    // The same pause after the last move: Finder writes the Put Back record
    // after the call returns, so the run is not reported finished -- the cue
    // a user may quit Canager on -- before it had the time every measured
    // run gave it. Everything is in the Trash by now, so a Cancel or the
    // budget only cuts the wait short.
    pause(pacing.settle.min(left()), &cancel).await;
    Ok(Outcome::Succeeded)
}

```

Each item's turn — its checks and its move — runs on tokio's blocking pool (`spawn_blocking`, which banager-core's `tokio` has through `tokio-util`'s `rt` feature) and is awaited to its end, Cancel or not (Ruling 28); the checks and the move sit in one closure so nothing runs between the item's last `lstat` and `Trasher::trash` (Ruling 26).

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p banager-core --lib adapters::standalone::removal`
Expected: PASS — 30 tests (13 from stage 6c, 17 here); the pause test takes about 0.3 s, the move-under-way test about 0.3 s, the budget-cut test about 1 s, the cancel-during-pause test well under a second. The same 30 ran green in the scratch mirror of stage 6c, with clippy clean.

- [ ] **Step 5: Continue the task**

No commit: continue to stage 6e.

#### Stage 6e: the adapter — trasher, `plan(Uninstall)` with what the preview saw, `execute`, `inventory`, `reconcile_after_uninstall`, `all`, `Session::new`; B's tests updated

- [ ] **Step 1: Write the failing tests and update B's**

In `crates/banager-core/src/adapters/standalone/mod.rs`, inside `mod tests`:

(i) Add to the module's `use` lines (B's already import `Warning`, `CancelPolicy`, `OpKind`, `OpRequest`, `Outcome`, `ResourceLock`, `UninstallBlocked`, `InstanceNote`, `VecSink`, `Adapter`; do not import any of those a second time):

```rust
    use super::recipe::{Route, RouteKind, UpgradeCmd, VersionCmd, VersionParse};
    use crate::events::{LogNote, OperationEvent};
    use crate::model::{Fault, ItemKind, KeptWhat, PlanAction, RemovedWhat, UninstallUnsafeReason};
    use crate::trash::MockTrasher;
```

(ii) Replace B's helper

```rust
    fn adapter(runner: Arc<dyn CommandRunner>) -> StandaloneAdapter {
        StandaloneAdapter::new(&CLAUDE, runner, Arc::new(MockHttpClient::new()))
    }
```

with

```rust
    fn adapter(runner: Arc<dyn CommandRunner>) -> StandaloneAdapter {
        adapter_with(runner, Arc::new(MockTrasher::new()))
    }

    /// An adapter over a trasher the test holds on to (to read its calls
    /// and its bin), with no pause after each item (ruling 12).
    fn adapter_with(runner: Arc<dyn CommandRunner>, trasher: Arc<MockTrasher>) -> StandaloneAdapter {
        StandaloneAdapter::new(&CLAUDE, runner, Arc::new(MockHttpClient::new()), trasher)
            .with_trash_gap(Duration::ZERO)
    }

    /// `TempHome::env` with the euid of the user who made the temp home
    /// (the files in it are theirs), for a `detect` whose `Detected.euid`
    /// the removal's check 3 will compare against. (`TempHome::env` says
    /// 501; nothing before this step read it.)
    fn env_as_owner(home: &TempHome) -> HostEnv {
        use std::os::unix::fs::MetadataExt;
        HostEnv {
            euid: std::fs::metadata(home.path()).expect("home metadata").uid(),
            ..home.env(vec![])
        }
    }

    /// The claude layout plus the cache and settings the dialog lists,
    /// detected (so `Detected` is filled) with `--version` answered.
    async fn full_install(
        tag: &str,
        trasher: Arc<MockTrasher>,
    ) -> (TempHome, super::testing::ClaudeLayout, StandaloneAdapter, ManagerInstance) {
        let home = TempHome::new(tag);
        let layout = claude_layout(&home, "2.1.281");
        home.dir(".claude/downloads");
        home.file(".claude/projects/p/session.jsonl");
        home.file(".claude.json");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("2.1.281 (Claude Code)\n"),
        );
        let adapter = adapter_with(runner, trasher);
        let inst = adapter.detect(&env_as_owner(&home)).await.remove(0);
        (home, layout, adapter, inst)
    }

    fn uninstall() -> OpRequest {
        request(OpKind::Uninstall, ArtifactKind::Binary, "claude")
    }

    fn notes_of(sink: &VecSink) -> Vec<LogNote> {
        sink.snapshot()
            .into_iter()
            .filter_map(|event| match event {
                OperationEvent::Note { note, .. } => Some(note),
                _ => None,
            })
            .collect()
    }
```

(iii) Every other three-argument `StandaloneAdapter::new(&CLAUDE, …)` in this `mod tests` gains `, Arc::new(MockTrasher::new())` as its fourth argument — at B's HEAD there are seven: in `detected_adapter` (`let adapter = StandaloneAdapter::new(&CLAUDE, runner, http);`), `test_check_updates_uses_the_version_after_inventory_not_detects_version` (`StandaloneAdapter::new(&CLAUDE, runner.clone(), http)`), `test_prereleases_stay_available_and_incomparable_pairs_are_uncheckable` (`StandaloneAdapter::new(&CLAUDE, runner, http)`), `test_check_updates_does_not_use_a_stale_version_after_a_failed_read` (`StandaloneAdapter::new(&CLAUDE, runner.clone(), http.clone())`), `test_check_updates_asks_nothing_for_a_launcher_only_row` (`StandaloneAdapter::new(&CLAUDE, Arc::new(MockRunner::new()), http.clone())`), `test_the_adapter_trait_delegates_to_the_inherent_methods` (`Arc::new(StandaloneAdapter::new(` over three lines as landed — `&CLAUDE,` / `runner,` / `Arc::new(MockHttpClient::new()),` — so the fourth argument goes on a line of its own), and `test_check_updates_over_the_recorded_pointers_lists_only_a_real_update` (`let adapter = StandaloneAdapter::new(&CLAUDE, runner, http);`). The compiler lists any this sentence misses. In `test_all_builds_one_adapter_per_recipe_under_its_standalone_id`, change `let adapters = all(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()));` to `let adapters = all(Arc::new(MockRunner::new()), Arc::new(MockHttpClient::new()), Arc::new(MockTrasher::new()));`.

(iv) Rename B's `test_inventory_is_the_tool_itself_with_no_safe_uninstall_method` to `test_inventory_is_the_tool_itself_and_offers_the_path_list_uninstall`, and in it replace `assert_eq!(a.uninstall_blocked, Some(UninstallBlocked::NoSafeMethod));` with:

```rust
        // The recipe has a path list, so nothing blocks the uninstall: the
        // gate lets it through to `plan`, and the page shows the button.
        assert_eq!(a.uninstall_blocked, None);
```

In B's `test_inventory_of_a_launcher_only_install_has_no_version_and_no_path`, replace the assertion rustfmt wrote over four lines,

```rust
        assert_eq!(
            artifacts[0].uninstall_blocked,
            Some(UninstallBlocked::NoSafeMethod)
        );
```

with:

```rust
        // The uninstall that finishes this state is allowed on this row
        // (spec Q17).
        assert_eq!(artifacts[0].uninstall_blocked, None);
```

(v) Replace B's `test_plan_refuses_install_as_unsupported_and_uninstall_as_no_safe_method` (from its `#[tokio::test]` through its closing `}`) with:

```rust
    /// Claude Code's recipe with no uninstall method: what the second
    /// batch's Ollama.app will be (spec §6.1 "Neither"). Every other field
    /// is `CLAUDE`'s.
    static NO_UNINSTALL: Recipe = Recipe {
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
        uninstall: None,
    };

    #[tokio::test]
    async fn test_plan_refuses_install_as_unsupported() {
        // The gate has no install-specific rule: an install against an
        // installed, answering tool reaches this `plan`, and `Unsupported`
        // is the answer it gets (B's corrected comment, kept).
        let home = TempHome::new("plan-install");
        let layout = claude_layout(&home, "2.1.281");
        let adapter = adapter(Arc::new(MockRunner::new()));
        let inst = instance_for(&layout, Some("2.1.281"));
        assert!(matches!(
            adapter.plan(&inst, &request(OpKind::Install, ArtifactKind::Binary, "claude")).await,
            Err(AdapterError::Unsupported(_))
        ));
    }

    #[tokio::test]
    async fn test_a_recipe_without_an_uninstall_method_says_so_and_refuses_to_plan_one() {
        // Spec §6.1 "Neither": the artifact carries `NoSafeMethod` (the
        // gate and the page read it), and `plan(Uninstall)` refuses with
        // the same reason for a stale snapshot. The one production path of
        // that variant after this step.
        let home = TempHome::new("plan-no-uninstall");
        let layout = claude_layout(&home, "2.1.281");
        let runner = Arc::new(MockRunner::new());
        runner.respond(
            vec![layout.launcher.to_str().unwrap(), "--version"],
            exited_0("2.1.281 (Claude Code)\n"),
        );
        let adapter = StandaloneAdapter::new(
            &NO_UNINSTALL,
            runner,
            Arc::new(MockHttpClient::new()),
            Arc::new(MockTrasher::new()),
        );
        let inst = adapter.detect(&env_as_owner(&home)).await.remove(0);
        let artifacts = adapter.inventory(&inst).await.expect("inventory");
        assert_eq!(artifacts[0].uninstall_blocked, Some(UninstallBlocked::NoSafeMethod));
        match adapter.plan(&inst, &uninstall()).await {
            Err(AdapterError::UninstallBlocked { reason }) => {
                assert_eq!(reason, UninstallBlocked::NoSafeMethod)
            }
            other => panic!("expected UninstallBlocked(NoSafeMethod), got {other:?}"),
        }
    }
```

(vi) Append the new tests before the module's closing `}`:

```rust

    #[tokio::test]
    async fn test_plan_uninstall_is_a_trash_paths_plan_carrying_the_dialogs_list() {
        // Spec §6.2, §6.6: no command; the paths in execution order, the
        // launcher last, with what the preview saw at each (Ruling 10) --
        // which never reaches the window: the action's JSON is the paths
        // alone; the warnings the dialog lists, in that order; the
        // instance's lock; no password; the removal's own time budget.
        let trasher = Arc::new(MockTrasher::new());
        let (home, layout, adapter, inst) = full_install("plan-uninstall", trasher).await;

        let plan = adapter.plan(&inst, &uninstall()).await.expect("plan");

        let PlanAction::TrashPaths { paths, previewed } = &plan.action else {
            panic!("a path list, not a command: {:?}", plan.action);
        };
        let listed = vec![
            home.path().join(".local/share/claude"),
            home.path().join(".claude/downloads"),
            layout.launcher.clone(),
        ];
        assert_eq!(paths, &listed);
        assert_eq!(
            previewed.iter().map(|seen| seen.kind).collect::<Vec<_>>(),
            vec![ItemKind::Dir, ItemKind::Dir, ItemKind::Symlink]
        );
        assert_eq!(
            serde_json::to_value(&plan.action).unwrap(),
            serde_json::json!({ "TrashPaths": { "paths": listed } })
        );
        assert_eq!(
            plan.warnings,
            vec![
                Warning::WillTrash {
                    path: "~/.local/share/claude".to_string(),
                    what: RemovedWhat::Program
                },
                Warning::WillTrash {
                    path: "~/.claude/downloads".to_string(),
                    what: RemovedWhat::Cache
                },
                Warning::WillTrash {
                    path: "~/.local/bin/claude".to_string(),
                    what: RemovedWhat::Launcher
                },
                Warning::WillKeep {
                    path: "~/.claude".to_string(),
                    what: KeptWhat::SettingsAndHistory
                },
                Warning::WillKeep {
                    path: "~/.claude.json".to_string(),
                    what: KeptWhat::Settings
                },
            ]
        );
        assert!(!plan.needs_password);
        assert_eq!(plan.locks, vec![ResourceLock("standalone-claude".to_string())]);
        assert_eq!(plan.cancel_policy, CancelPolicy::KillThenReconcile);
        assert!(plan.affected.is_empty(), "a non-empty list would disable Uninstall");
        assert_eq!(plan.timeout_secs, removal::TIMEOUT_SECS);
        assert_eq!(plan.request, uninstall());
    }

    #[tokio::test]
    async fn test_plan_uninstall_refuses_before_anything_was_detected() {
        // Unreachable through `Session` (it detects before it plans); the
        // adapter's own answer, a plain `Refused`, for a caller that skips
        // that (spec §3.2).
        let home = TempHome::new("plan-undetected");
        let layout = claude_layout(&home, "2.1.281");
        let adapter = adapter(Arc::new(MockRunner::new()));
        let inst = instance_for(&layout, Some("2.1.281"));
        assert!(matches!(
            adapter.plan(&inst, &uninstall()).await,
            Err(AdapterError::Refused(_))
        ));
    }

    #[tokio::test]
    async fn test_plan_uninstall_refuses_a_path_that_fails_a_check_with_the_reason() {
        // One of the checks failing reaches the dialog as `UninstallUnsafe`
        // (Task 5's kind), with the path abbreviated.
        let trasher = Arc::new(MockTrasher::new());
        let (home, layout, adapter, inst) = full_install("plan-unsafe", trasher).await;
        let elsewhere = home.executable("elsewhere/claude");
        std::fs::remove_file(&layout.launcher).unwrap();
        std::os::unix::fs::symlink(elsewhere, &layout.launcher).unwrap();

        match adapter.plan(&inst, &uninstall()).await {
            Err(AdapterError::UninstallUnsafe { path, reason }) => {
                assert_eq!(path, "~/.local/bin/claude");
                assert_eq!(reason, UninstallUnsafeReason::NotWhatInstructionsExpect);
            }
            other => panic!("expected UninstallUnsafe, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_plan_uninstall_on_a_launcher_only_row_lists_the_program_dir_as_already_gone() {
        let home = TempHome::new("plan-launcher-only");
        let layout = claude_layout(&home, "2.1.281");
        std::fs::remove_dir_all(&layout.root).unwrap();
        let adapter = adapter(Arc::new(MockRunner::new()));
        let inst = adapter.detect(&env_as_owner(&home)).await.remove(0);
        assert_eq!(inst.status.notes, vec![InstanceNote::LauncherOnly]);

        let plan = adapter.plan(&inst, &uninstall()).await.expect("plan");

        let PlanAction::TrashPaths { paths, previewed } = &plan.action else {
            panic!("a path list, not a command: {:?}", plan.action);
        };
        assert_eq!(paths, &vec![layout.launcher.clone()]);
        assert_eq!(previewed.len(), 1, "what the preview saw at the launcher");
        assert_eq!(
            plan.warnings,
            vec![
                Warning::AlreadyGone {
                    path: "~/.local/share/claude".to_string()
                },
                Warning::WillTrash {
                    path: "~/.local/bin/claude".to_string(),
                    what: RemovedWhat::Launcher
                },
            ]
        );
    }

    #[tokio::test]
    async fn test_execute_moves_every_listed_path_in_order_and_logs_each() {
        // Review Focus 1: two paths named `claude`, both moved, both in the
        // Trash.
        let trasher = Arc::new(MockTrasher::new());
        let (home, layout, adapter, inst) = full_install("execute-uninstall", trasher.clone()).await;
        let plan = adapter.plan(&inst, &uninstall()).await.expect("plan");
        let sink = Arc::new(VecSink::new());

        let outcome = adapter
            .execute(&plan, sink.clone(), 9, CancellationToken::new())
            .await
            .expect("execute");

        assert_eq!(outcome, Outcome::Succeeded);
        let PlanAction::TrashPaths { paths, .. } = &plan.action else {
            panic!("a path list");
        };
        assert_eq!(trasher.calls(), *paths);
        assert!(trasher.bin().join("claude/versions/2.1.281").is_file());
        assert!(trasher.bin().join("downloads").is_dir());
        let link = trasher.bin().join("claude 2");
        assert!(std::fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert!(std::fs::symlink_metadata(&layout.launcher).is_err());
        assert!(home.path().join(".claude.json").is_file(), "settings stay");
        assert!(home.path().join(".claude/projects/p/session.jsonl").is_file());
        assert_eq!(
            notes_of(&sink),
            vec![
                LogNote::MovedToTrash {
                    path: "~/.local/share/claude".to_string(),
                    trashed_to: trasher.bin().join("claude").display().to_string(),
                },
                LogNote::MovedToTrash {
                    path: "~/.claude/downloads".to_string(),
                    trashed_to: trasher.bin().join("downloads").display().to_string(),
                },
                LogNote::MovedToTrash {
                    path: "~/.local/bin/claude".to_string(),
                    trashed_to: link.display().to_string(),
                },
            ]
        );
        // Afterwards the tool is gone for `detect` and `inventory` alike.
        assert!(adapter.detect(&env_as_owner(&home)).await.is_empty());
        assert!(adapter.inventory(&inst).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_execute_refuses_when_a_path_changed_since_the_preview() {
        // Review Focus 2: the launcher re-pointed between the preview and
        // the click. Nothing moved; the fault names the path.
        let trasher = Arc::new(MockTrasher::new());
        let (home, layout, adapter, inst) = full_install("execute-changed", trasher.clone()).await;
        let plan = adapter.plan(&inst, &uninstall()).await.expect("plan");
        let elsewhere = home.executable("elsewhere/claude");
        std::fs::remove_file(&layout.launcher).unwrap();
        std::os::unix::fs::symlink(elsewhere, &layout.launcher).unwrap();

        let outcome = adapter
            .execute(&plan, Arc::new(VecSink::new()), 9, CancellationToken::new())
            .await
            .expect("execute");

        assert_eq!(
            outcome,
            Outcome::CanagerFailed(Fault::PathChanged {
                path: "~/.local/bin/claude".to_string()
            })
        );
        assert!(trasher.calls().is_empty());
        assert!(layout.root.join("versions/2.1.281").is_file());
    }

    #[tokio::test]
    async fn test_execute_refuses_a_launcher_the_updater_re_pointed_after_the_preview() {
        // Ruling 10 through the adapter: what the preview saw rides in the
        // plan it issued (`PlanAction::TrashPaths.previewed`). Claude Code
        // updating itself between the preview and the click re-points the
        // launcher at a new version inside the root -- every check still
        // passes and the path is the same string -- but it is not the link
        // the user was shown: nothing moves, and the user previews again.
        let trasher = Arc::new(MockTrasher::new());
        let (home, layout, adapter, inst) =
            full_install("execute-self-updated", trasher.clone()).await;
        let plan = adapter.plan(&inst, &uninstall()).await.expect("plan");
        let newer = home.executable(".local/share/claude/versions/2.1.282");
        std::fs::remove_file(&layout.launcher).unwrap();
        std::os::unix::fs::symlink(newer, &layout.launcher).unwrap();

        let outcome = adapter
            .execute(&plan, Arc::new(VecSink::new()), 9, CancellationToken::new())
            .await
            .expect("execute");

        assert_eq!(
            outcome,
            Outcome::CanagerFailed(Fault::PathChanged {
                path: "~/.local/bin/claude".to_string()
            })
        );
        assert!(trasher.calls().is_empty());
        assert!(layout.root.join("versions/2.1.281").is_file());
    }

    #[tokio::test]
    async fn test_execute_stops_at_a_refused_item_and_leaves_the_launcher() {
        // Review Focus 4, first half: macOS refused the cache directory.
        let trasher = Arc::new(MockTrasher::new());
        trasher.refuse_call(1, "Operation not permitted");
        let (_home, layout, adapter, inst) = full_install("execute-refused", trasher.clone()).await;
        let plan = adapter.plan(&inst, &uninstall()).await.expect("plan");
        let sink = Arc::new(VecSink::new());

        let outcome = adapter
            .execute(&plan, sink.clone(), 9, CancellationToken::new())
            .await
            .expect("execute");

        assert_eq!(
            outcome,
            Outcome::Failed {
                exit_code: None,
                summary: "Operation not permitted".to_string()
            }
        );
        assert_eq!(trasher.calls().len(), 2);
        assert!(std::fs::symlink_metadata(&layout.launcher)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(matches!(
            notes_of(&sink)[..],
            [LogNote::MovedToTrash { .. }, LogNote::TrashFailed { .. }]
        ));
    }

    #[tokio::test]
    async fn test_execute_stops_between_items_when_cancelled() {
        // Review Focus 4, second half: Cancel after the first item.
        let token = CancellationToken::new();
        let trasher = Arc::new(MockTrasher::new());
        trasher.cancel_after_call(0, token.clone());
        let (_home, layout, adapter, inst) = full_install("execute-cancel", trasher.clone()).await;
        let plan = adapter.plan(&inst, &uninstall()).await.expect("plan");

        let outcome = adapter
            .execute(&plan, Arc::new(VecSink::new()), 9, token)
            .await
            .expect("execute");

        assert_eq!(outcome, Outcome::Unconfirmed);
        assert_eq!(trasher.calls().len(), 1);
        assert!(std::fs::symlink_metadata(&layout.launcher)
            .unwrap()
            .file_type()
            .is_symlink());
    }

    #[tokio::test]
    async fn test_execute_runs_out_its_budget_between_items_not_a_process() {
        // `Plan.timeout_secs` is the removal's own clock: a spent budget
        // stops the run before the next item, as `Unconfirmed`.
        let trasher = Arc::new(MockTrasher::new());
        let (_home, _layout, adapter, inst) = full_install("execute-budget", trasher.clone()).await;
        let plan = Plan {
            timeout_secs: 0,
            ..adapter.plan(&inst, &uninstall()).await.expect("plan")
        };

        let outcome = adapter
            .execute(&plan, Arc::new(VecSink::new()), 9, CancellationToken::new())
            .await
            .expect("execute");

        assert_eq!(outcome, Outcome::Unconfirmed);
        assert!(trasher.calls().is_empty());
    }

    #[tokio::test]
    async fn test_reconcile_after_uninstall_tells_there_gone_and_cannot_tell_apart() {
        // B's deviation 15, the hand-off, and Ruling 27: after an uninstall
        // only presence is asked. The launcher-only state a stopped run
        // leaves is present -- so `run_operation` reports the stop
        // truthfully (`Cancelled` after a Cancel,
        // `StillInstalledAfterUninstall` after an exit that claimed
        // success) -- while `reconcile` keeps B's strict rule for upgrades;
        // a launcher that is gone is absent; and one Canager cannot look at
        // (its folder unreadable) is neither: an error, which
        // `run_operation` reports as `Unconfirmed`, never as a finished
        // uninstall.
        let trasher = Arc::new(MockTrasher::new());
        let (_home, layout, adapter, inst) = full_install("reconcile-after", trasher).await;
        let key = adapter.artifact_key(&inst);
        std::fs::remove_dir_all(&layout.root).unwrap();

        let still_there = adapter
            .reconcile_after_uninstall(&inst, &key)
            .await
            .expect("a reading");
        assert!(still_there.present);
        assert!(matches!(
            adapter.reconcile(&inst, &key).await,
            Err(AdapterError::Parse(_))
        ));

        let bin = layout.launcher.parent().unwrap().to_path_buf();
        if let Some(_locked) = super::testing::Unreadable::new(&bin) {
            assert!(matches!(
                adapter.reconcile_after_uninstall(&inst, &key).await,
                Err(AdapterError::Parse(_))
            ));
        }

        std::fs::remove_file(&layout.launcher).unwrap();
        let gone = adapter
            .reconcile_after_uninstall(&inst, &key)
            .await
            .expect("a reading");
        assert!(!gone.present);
    }

    #[tokio::test]
    async fn test_detect_lists_nothing_for_a_launcher_that_reaches_the_root_through_another_link() {
        // Ruling 27, at the adapter: B listed this layout (its `realpath`
        // lands in the root); step C does not, because an uninstall that
        // stopped after moving the root would leave the middle link
        // dangling and the launcher reading as not installed. No instance,
        // so no Uninstall to offer; the Unknown page lists the two links.
        let home = TempHome::new("detect-hop-outside");
        let real = home.executable(".local/share/claude/versions/2.1.281");
        let current = home.link(".local/bin/claude-current", &real);
        home.link(".local/bin/claude", &current);
        let adapter = adapter(Arc::new(MockRunner::new()));

        assert!(adapter.detect(&env_as_owner(&home)).await.is_empty());
    }
```

In `crates/banager-core/tests/ops_upgrade_version_test.rs`, add `use banager_core::trash::MockTrasher;` to the imports (after the `use banager_core::runner::{…};` line), and in `claude_upgrade_outputs` append `Arc::new(MockTrasher::new())` as the fourth argument of both `StandaloneAdapter::new(` calls (the one rustfmt wrote over three lines inside `upgrade(&runner, Arc::new(StandaloneAdapter::new(` … `)), …)` — `&CLAUDE,` / `mutating,` / `Arc::new(MockHttpClient::new()),` — and `let adapter =` / `StandaloneAdapter::new(&CLAUDE, runner.clone(), Arc::new(MockHttpClient::new()));`, wrapped after the `=`).

Task 1's four `TrashPaths` literals gain the field stage Step 3 adds, and its wire test says what the field does on the wire:

- In `crates/banager-core/src/model.rs`, `test_plan_round_trips_through_json`: in the `trash` plan's `action: PlanAction::TrashPaths { paths: vec![ … ], }`, after the `paths` vector's closing `],` add the line `previewed: Vec::new(),` (a plan read back from JSON has none, so the round trip compares equal only with none).
- In the same file, in `test_plan_action_is_externally_tagged_on_the_wire`, replace the second `assert_eq!` (the one on `PlanAction::TrashPaths { paths: vec![PathBuf::from("/Users/someone/.local/bin/claude")], }`, through its closing `);`) with:

```rust
        // What the preview saw rides in the plan on this side only
        // (`previewed`, stage 6e of the step C plan): the wire, and so the
        // TypeScript mirror, carries the paths alone; a plan read back has
        // none; and a payload that names the field is not read.
        let trash = PlanAction::TrashPaths {
            paths: vec![PathBuf::from("/Users/someone/.local/bin/claude")],
            previewed: vec![ItemIdentity {
                dev: 1,
                ino: 2,
                kind: ItemKind::Symlink,
            }],
        };
        let json = serde_json::to_string(&trash).unwrap();
        assert_eq!(
            json,
            r#"{"TrashPaths":{"paths":["/Users/someone/.local/bin/claude"]}}"#
        );
        assert_eq!(
            serde_json::from_str::<PlanAction>(&json).unwrap(),
            PlanAction::TrashPaths {
                paths: vec![PathBuf::from("/Users/someone/.local/bin/claude")],
                previewed: Vec::new(),
            }
        );
        assert_eq!(
            serde_json::from_str::<PlanAction>(
                r#"{"TrashPaths":{"paths":[],"previewed":[{"dev":1,"ino":2}]}}"#
            )
            .unwrap(),
            PlanAction::TrashPaths {
                paths: Vec::new(),
                previewed: Vec::new(),
            }
        );
```

- In `crates/banager-core/src/adapters/mod.rs`, `test_run_plan_refuses_a_plan_that_runs_no_command`, and in `crates/banager-core/tests/ops_summaries_test.rs`, `test_summaries_gives_a_plan_that_runs_no_command_an_empty_argv_preview`: in each `action: PlanAction::TrashPaths { … }`, after the line `paths: vec![PathBuf::from("/Users/someone/.local/bin/claude")],` add the line `previewed: Vec::new(),` (neither plan reaches `execute`).

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p banager-core --lib adapters::standalone::tests`
Expected: FAIL to compile — `this function takes 3 arguments but 4 arguments were supplied` for `StandaloneAdapter::new` (and `all`); `no method named \`with_trash_gap\``; `no method named \`reconcile_after_uninstall\` found for struct \`StandaloneAdapter\``; `variant \`PlanAction::TrashPaths\` has no field named \`previewed\`` (in `standalone/mod.rs`'s tests, `model.rs`'s, `adapters/mod.rs`'s). `cargo test -p banager-core --test ops_upgrade_version_test` and `--test ops_summaries_test` fail the same way.

- [ ] **Step 3: Wire the adapter and the session**

In `crates/banager-core/src/adapters/standalone/mod.rs`:

(i) Imports (non-test). Add `Uninstall` to B's `use self::recipe::{…}` import (it holds `Latest, Recipe` since B's stage 7); add `CancelPolicy` to B's `use crate::model::{…}` list (alphabetical; rustfmt re-wraps — `PlanAction` is already there, from Task 1, and a second import of it would not compile); and add a line `use crate::trash::Trasher;`. (`CancelPolicy` is named by `plan` now; B kept it out of the non-test imports because only the tests named it.)

(ii) Replace B's `StandaloneAdapter` struct (from its doc comment `/// One tool installed by its own installer, as the \`Adapter\` contract` through its closing `}`) and `new` (from `/// Panics on a meta file that does not parse or whose \`id\` is not` through `new`'s closing `}`) with:

```rust
/// One tool installed by its own installer, as the `Adapter` contract
/// sees it. Built once per `Recipe` by `all()`; the instance it detects
/// *is* the native install (spec D2).
pub struct StandaloneAdapter {
    recipe: &'static Recipe,
    meta: AdapterMeta,
    runner: Arc<dyn CommandRunner>,
    /// The channel pointer request in `check_updates`.
    http: Arc<dyn HttpClient>,
    /// The system's "move to Trash", for a path-list uninstall
    /// (`removal::execute_removal`, from `execute`): `RealTrasher` in
    /// production (`Session::new`), `MockTrasher` in tests -- injected
    /// like the runner and the client.
    trasher: Arc<dyn Trasher>,
    /// The pause after each item of a path-list uninstall
    /// (`removal::PUT_BACK_SETTLE`); zero in tests (`with_trash_gap`).
    /// Read by `execute`.
    trash_gap: Duration,
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
        trasher: Arc<dyn Trasher>,
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
            trasher,
            trash_gap: removal::PUT_BACK_SETTLE,
            detected: Mutex::new(None),
        }
    }

    /// Test seam, like `BrewAdapter::with_background_change`: the pause
    /// after each item of a path-list uninstall -- zero in tests, so no
    /// test waits seconds per item. Public so `tests/` can use it too.
    pub fn with_trash_gap(mut self, gap: Duration) -> StandaloneAdapter {
        self.trash_gap = gap;
        self
    }
```

(B's `new` sat right after the struct at the top of `impl StandaloneAdapter { … }`; the block above ends inside that `impl`, before B's `detect`, which follows unchanged apart from stage 6c's `euid` line.)

(iii) In `inventory`, replace

```rust
            // No uninstall method in this step (spec §6.1 "Neither"): the
            // gate refuses, the page hides the button and says why. Step
            // C's path-list uninstall replaces this with `None`.
            uninstall_blocked: Some(UninstallBlocked::NoSafeMethod),
```

with

```rust
            // A recipe with no uninstall method (spec §6.1 "Neither"; none
            // in the first batch, the second batch's Ollama.app): the gate
            // refuses, the page hides the button and says why. With a path
            // list, nothing blocks it.
            uninstall_blocked: self
                .recipe
                .uninstall
                .is_none()
                .then_some(UninstallBlocked::NoSafeMethod),
```

(iv) Replace the doc comment directly above `pub async fn reconcile` — as landed in `71eacd0`, six lines:

```rust
    /// Step B only executes upgrades: an owned launcher without a readable
    /// version is not sufficient evidence that an upgrade succeeded.
    /// Inventory still preserves `LauncherOnly` presence for display and
    /// step C's removal. Step C must use that presence for uninstall
    /// verification while keeping this stricter upgrade check (phase 4
    /// step B plan, deviation 15).
```

— with:

```rust
    /// The reading before and after an upgrade (and after an install,
    /// which this adapter never plans): an owned launcher without a
    /// readable version is no evidence that an upgrade succeeded, so it is
    /// refused (`Parse`) and `run_operation` reports `Unconfirmed` (B's
    /// Astra finding 1). After an uninstall `run_operation` reads
    /// `reconcile_after_uninstall` instead, which counts that launcher as
    /// still there.
```

and after `reconcile`'s closing `}` insert:

```rust

    /// The reading after an uninstall (`Adapter::reconcile_after_uninstall`):
    /// presence alone, from the disk now, through `route::probe_strict`. A
    /// launcher-only launcher -- the dangling link a stopped path-list
    /// uninstall leaves -- is present, so `run_operation` reports the stop
    /// truthfully (`Cancelled` after the user's Cancel,
    /// `StillInstalledAfterUninstall` after a run that claimed success) and
    /// the next refresh shows the row a second Uninstall finishes; a
    /// launcher that is gone is absent (spec §3.6); and a launcher Canager
    /// cannot look at -- a permission error, a loop -- is neither: an
    /// error, which `run_operation` reports as `Unconfirmed`, never as a
    /// finished uninstall (Ruling 27). No version is read: presence is the
    /// whole question, and a stopped uninstall's launcher has none.
    pub async fn reconcile_after_uninstall(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        let present =
            match route::probe_strict(self.recipe.route.kind, &inst.exe_path, &inst.prefix) {
                Ok(Probe::Absent) => false,
                Ok(Probe::Present { .. } | Probe::LauncherOnly) => true,
                Err(error) => {
                    return Err(AdapterError::Parse(format!(
                        "cannot tell whether {} is still there: {error}",
                        inst.exe_path.display()
                    )))
                }
            };
        // The one artifact, matched as `reconcile_from` matches -- kind and
        // name, never the instance id (adapters/mod.rs says why).
        let this_tool = key.kind == ArtifactKind::Binary && key.name == self.recipe.id;
        Ok(Reconciled {
            present: present && this_tool,
            version: None,
        })
    }
```

(v) Replace B's `plan` (its doc comment `/// Spec §五: the tool's own documented update command, run against the` through its closing `}`) and `execute` (through its closing `}`) with:

```rust
    /// Spec §五: the tool's own documented update command, run against the
    /// launcher through `run_plan` unchanged. `Install` is `Unsupported`
    /// (the installer is Anthropic's and Canager never runs it; installing
    /// tools is phase 5). `Uninstall` is the recipe's path list as a
    /// `TrashPaths` plan under the removal's checks (spec §6.2-§6.3), with
    /// what the preview saw at each path riding along on this side only
    /// (`previewed`, skipped by serde; Ruling 10), or `NoSafeMethod` for a
    /// recipe without one -- the gate (`blocked_uninstall`) refuses that
    /// first; this is its late twin for a stale snapshot. The one artifact
    /// is `Binary`/`<recipe.id>`, so any other name or kind is a request
    /// this adapter cannot mean.
    pub async fn plan(&self, inst: &ManagerInstance, req: &OpRequest) -> Result<Plan, AdapterError> {
        ensure_instance_match(req, inst)?;
        validate_package_name(&req.name)?;
        if req.name != self.recipe.id || req.artifact_kind != ArtifactKind::Binary {
            return Err(AdapterError::InvalidName(req.name.clone()));
        }
        match req.kind {
            OpKind::Install => Err(AdapterError::Unsupported(format!(
                "{} is installed by its own installer, which Canager never runs",
                self.meta.name
            ))),
            OpKind::Uninstall => {
                let Some(uninstall) = &self.recipe.uninstall else {
                    return Err(AdapterError::UninstallBlocked {
                        reason: UninstallBlocked::NoSafeMethod,
                    });
                };
                match *uninstall {
                    Uninstall::Paths { remove, keep } => {
                        let removal = removal::plan_removal(&removal::Job {
                            recipe: self.recipe,
                            detected: self.detected_or_refuse()?,
                            remove,
                            keep,
                        })?;
                        Ok(Plan {
                            request: req.clone(),
                            // No command: `execute` moves these itself. What
                            // the preview saw at each stays with the plan on
                            // this side (`previewed`, skipped on the wire).
                            action: PlanAction::TrashPaths {
                                paths: removal.paths,
                                previewed: removal.identities,
                            },
                            // Everything lives under $HOME (spec §6.2).
                            needs_password: false,
                            locks: vec![ResourceLock(inst.id.clone())],
                            // Between items the token is watched; there is
                            // no process to stop (spec §6.2).
                            cancel_policy: CancelPolicy::KillThenReconcile,
                            warnings: removal.warnings,
                            // "Would break": nothing depends on a tool's
                            // own files this way, and a non-empty list
                            // disables the confirm button.
                            affected: Vec::new(),
                            timeout_secs: removal::TIMEOUT_SECS,
                        })
                    }
                }
            }
            OpKind::Upgrade => {
                let upgrade = &self.recipe.upgrade;
                Ok(Plan {
                    request: req.clone(),
                    action: PlanAction::Command {
                        // The launcher, exactly as previewed: never a
                        // program the recipe could name (spec 附录 B).
                        program: inst.exe_path.clone(),
                        args: upgrade.args.iter().map(|a| a.to_string()).collect(),
                        // Not the version read's environment: `claude
                        // update` must not be told to stop updating (spec
                        // §3.4).
                        env: Vec::new(),
                    },
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

    /// What `detect` wrote, or a plain `Refused` when nothing has been
    /// detected -- unreachable through `Session`, which detects before it
    /// plans (spec §3.2), so no sentence of its own.
    fn detected_or_refuse(&self) -> Result<Detected, AdapterError> {
        self.detected.lock().unwrap().clone().ok_or_else(|| {
            AdapterError::Refused(format!(
                "{} has not been detected in this session",
                self.meta.name
            ))
        })
    }

    /// A `Command` plan runs through `run_plan` like every source's; a
    /// `TrashPaths` plan is carried out here, item by item
    /// (`removal::execute_removal`), against the list re-read from the
    /// recipe and the disk and compared with what the preview saw
    /// (`previewed`) -- the plan's paths are what the user confirmed, not
    /// the source of truth.
    pub async fn execute(
        &self,
        plan: &Plan,
        sink: Arc<dyn EventSink>,
        op_id: OpId,
        cancel: CancellationToken,
    ) -> Result<Outcome, AdapterError> {
        match &plan.action {
            PlanAction::Command { .. } => run_plan(&self.runner, plan, sink, op_id, cancel).await,
            PlanAction::TrashPaths { paths, previewed } => {
                let Some(Uninstall::Paths { remove, keep }) = self.recipe.uninstall else {
                    return Err(AdapterError::Refused(format!(
                        "{} has no path list to carry out",
                        self.meta.name
                    )));
                };
                removal::execute_removal(
                    &removal::Job {
                        recipe: self.recipe,
                        detected: self.detected_or_refuse()?,
                        remove,
                        keep,
                    },
                    removal::Confirmed { paths, previewed },
                    &self.trasher,
                    removal::Pacing {
                        settle: self.trash_gap,
                        budget: Duration::from_secs(plan.timeout_secs),
                    },
                    sink,
                    op_id,
                    cancel,
                )
                .await
            }
        }
    }
```

(vi) Replace B's `all` (its doc comment `/// One adapter per recipe in \`recipes::RECIPES\`, over the shared runner` through its closing `}`) with:

```rust
/// One adapter per recipe in `recipes::RECIPES`, over the shared runner,
/// http client and trasher, for `Session::new`'s registration list.
pub fn all(
    runner: Arc<dyn CommandRunner>,
    http: Arc<dyn HttpClient>,
    trasher: Arc<dyn Trasher>,
) -> Vec<Arc<dyn Adapter>> {
    recipes::RECIPES
        .iter()
        .map(|&recipe| {
            Arc::new(StandaloneAdapter::new(
                recipe,
                runner.clone(),
                http.clone(),
                trasher.clone(),
            )) as Arc<dyn Adapter>
        })
        .collect()
}
```

(vii) In `impl Adapter for StandaloneAdapter`, after the `reconcile` method's closing `}`, insert:

```rust

    async fn reconcile_after_uninstall(
        &self,
        inst: &ManagerInstance,
        key: &ArtifactKey,
    ) -> Result<Reconciled, AdapterError> {
        StandaloneAdapter::reconcile_after_uninstall(self, inst, key).await
    }
```

(viii) In `crates/banager-core/src/model.rs`, in `pub enum PlanAction` (Task 1), replace the variant line `    TrashPaths { paths: Vec<PathBuf> },` (below its doc comment, which ends `/// \`uninstall\` is \`Uninstall::Paths\`.` and stays) with:

```rust
    TrashPaths {
        paths: Vec<PathBuf>,
        /// What the preview saw at each of `paths`, in the same order
        /// (`removal::plan_removal`): `removal::execute_removal` refuses
        /// with `Fault::PathChanged` when any path is no longer that file,
        /// before anything moves and again right before each move (spec
        /// §6.3). Skipped by serde: the `IssuedPlan` the window receives
        /// carries the paths alone, the TypeScript mirror has no such
        /// field, and a plan read back from JSON has none -- which
        /// `execute_removal` refuses rather than moving what nobody looked
        /// at. It rides in the plan `Session` keeps (`StoredPlan`) and
        /// hands to `OperationManager::submit`, so `Adapter::execute`
        /// reads it with no parameter of its own.
        #[serde(skip)]
        previewed: Vec<ItemIdentity>,
    },
```

(`#[serde(skip)]` on a field of an externally tagged variant: checked in a scratch crate on this lockfile's serde 1.0.229 — the variant serialises without the field, deserialises it as `Vec::new()`, needs no serde impls on `ItemIdentity`, and passes `cargo clippy -- -D warnings`. Ruling 10.)

In `crates/banager-core/src/session/mod.rs`, add `use crate::trash::RealTrasher;` to the imports (after B's `use crate::adapters::standalone;`), replace in `Session::new`

```rust
        adapters.extend(standalone::all(runner, http));
```

with

```rust
        // The one thing in this crate that moves a file itself: macOS's own
        // "move to Trash", for a confirmed path-list uninstall
        // (`removal::execute_removal`; docs/what-we-run.md).
        adapters.extend(standalone::all(runner, http, Arc::new(RealTrasher::new())));
```

and replace its doc comment's first sentence (B's `/// Registers all eight adapters over a shared \`RealRunner\` and` / `/// \`RealHttpClient\` (network-touching adapters only: pipx, cargo,` / `/// ollama, and the standalone tools' update checks). \`now_fn\` exists`) with

```rust
    /// Registers all eight adapters over a shared `RealRunner` and
    /// `RealHttpClient` (network-touching adapters only: pipx, cargo,
    /// ollama, and the standalone tools' update checks), and gives the
    /// standalone tools the real Trash (`RealTrasher`) for their path-list
    /// uninstalls. `now_fn` exists
```

(keeping the rest of that doc comment, `so tests can pin \`refreshed_at\`; production passes \`None\`.`, as it is).

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p banager-core --lib adapters::standalone`, `cargo test -p banager-core --lib model`, `cargo test -p banager-core --lib adapters::tests`, `cargo test -p banager-core --test ops_upgrade_version_test`, `cargo test -p banager-core --test ops_summaries_test` and `cargo test -p banager-core --lib session`
Expected: PASS — B's tests as updated, the stage 6b–6d tests, the new adapter tests (two in (v), twelve in (vi)); `test_plan_action_is_externally_tagged_on_the_wire` with the skipped field; B's `test_a_claude_update_exiting_zero_with_a_dangling_launcher_is_unconfirmed` still `Unconfirmed` (an upgrade is verified with `reconcile`, which stays strict, and its dangling launcher is still launcher-only under the one-hop rule, which that test's own assertion checks); `test_new_registers_all_eight_adapters` unchanged. The glue of (iv)–(v) — `let Some(Uninstall::Paths { remove, keep }) = self.recipe.uninstall else`, `match *uninstall`, `removal::Confirmed { paths, previewed }` built from the plan's `&Vec`s, and `probe_strict`'s three arms — was compiled clippy-clean against the scratch mirror of stage 6c.

- [ ] **Step 5: Continue the task**

No commit: continue to stage 6f.

#### Stage 6f: the words the row needs now that it offers Uninstall

B shipped the launcher-only notice with no promise of an uninstall and doc comments that say "until step C"; this commit makes the uninstall real, so the same commit makes those words true (Ruling 13).

- [ ] **Step 1: Write the failing tests**

In `src/lib/sources.test.ts`, inside `it("warns, and names the link, when only a standalone tool's launcher is left", …)`, replace its comment

```ts
    // The half-uninstalled state (program files gone, launcher dangling):
    // a warning because this launcher is broken; another PATH copy may work. No
    // button, and -- in this step -- no Uninstall on the row either (its
    // artifact carries NoSafeMethod until step C), so the sentence must
    // not promise one.
```

with

```ts
    // The half-uninstalled state (program files gone, launcher dangling):
    // a warning because this launcher is broken; another PATH copy may
    // work. No button on the notice: the row's own Uninstall moves the
    // link, which the sentence says.
```

and inside `it("puts the command and the source into every standalone notice's copy, in both locales", …)`, replace

```ts
      // Until step C the LauncherOnly row's artifact carries NoSafeMethod,
      // so the Installed page shows no Uninstall button on it: the notice
      // must not tell the user to press one (spec §9.2's sentence returns
      // with step C's uninstall).
      expect(locale.sourceNotice.launcherOnly.description).not.toMatch(/Uninstall removes|卸载会把/);
      expect(locale.sourceNotice.launcherOnly.description).not.toMatch(/typing .* in Terminal fails|输入 .* 会失败/);
```

with

```ts
      // The LauncherOnly row offers Uninstall (its artifact carries no
      // `uninstall_blocked` since step C), so spec §9.2's promises are
      // back: the link goes to the Trash too, and a folder an earlier
      // stopped uninstall moved may be in the Trash -- "may", as spec §9.2
      // says: the Trash can have been emptied since. It still does not claim
      // typing the command fails -- another copy on PATH may run (B's
      // review finding 9).
      expect(locale.sourceNotice.launcherOnly.description).toMatch(
        /Uninstall moves the link to the Trash|卸载会把这个链接也移到废纸篓/,
      );
      expect(locale.sourceNotice.launcherOnly.description).toMatch(/may be in the Trash|可能在废纸篓里/);
      expect(locale.sourceNotice.launcherOnly.description).not.toMatch(
        /typing .* in Terminal fails|输入 .* 会失败/,
      );
```

In the same file, B's review added (commit `3e19fd9`) `it("points at the tool's official documentation, not at a website Canager doesn't show, when only the launcher is left", …)`, which the new sentence below must keep passing — it names `{{source}}'s official documentation` / `{{source}} 官方文档` and neither "website" nor "网站". Only its comment goes stale with this step; replace its two lines

```ts
    // The same rule as the no-safe-method sentence on this instance's own
    // row (its artifact carries NoSafeMethod until step C): Canager shows
```

with

```ts
    // The same rule as the no-safe-method sentence (the row of a recipe
    // without an uninstall method; this row's own before step C): Canager shows
```

In `src/pages/InstalledPage.test.tsx`, in B's `it("shows the standalone summary alongside its real uninstall refusal", …)`, replace the comment lines

```tsx
    // Only the Homebrew artifact may offer Uninstall; B's actual Claude
    // artifact is NoSafeMethod and must still show both sentences.
```

with

```tsx
    // Only the Homebrew artifact may offer Uninstall: this Claude artifact
    // is marked NoSafeMethod, as a recipe without an uninstall method's
    // is, and must still show both sentences.
```

and after that test's closing `});` insert:

```tsx
  it("offers Uninstall on Claude Code, beside its summary, now that it has a path list", async () => {
    // Phase 4 step C: the standalone artifact carries no `uninstall_blocked`
    // once its recipe lists the paths to move, so the row shows its
    // summary and an Uninstall button like any other package's, and
    // `Session::issue_plan` lets the plan through (tests/
    // standalone_uninstall_test.rs drives that end to end).
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
          uninstall_blocked: null,
        },
      ],
      updates: [],
    };
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_snapshot") return Promise.resolve(claudeSnapshot);
      if (cmd === "get_settings") return Promise.resolve(settings);
      return Promise.resolve(undefined);
    });

    const { findByText, getAllByRole, queryByText } = renderWithProviders(<InstalledPage />);

    expect(
      await findByText(
        "Anthropic's coding assistant for the terminal. Installed with its own installer, not with Homebrew or npm.",
      ),
    ).toBeInTheDocument();
    expect(getAllByRole("button", { name: "Uninstall" })).toHaveLength(1);
    expect(queryByText("Can't uninstall here")).toBeNull();
  });

```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm exec vitest run src/lib/sources.test.ts src/pages/InstalledPage.test.tsx`
Expected: FAIL — `puts the command and the source into every standalone notice's copy, in both locales`: `expected "The program files targeted by this {{command}} link are missing…" to match /Uninstall moves the link to the Trash|卸载会把这个链接也移到废纸篓/`. (The new InstalledPage test already passes: it pins the row this step makes real, through B's `installedDescription`.)

- [ ] **Step 3: Write the sentence and the doc comments**

In `src/i18n/en.json`, inside `"sourceNotice": { "launcherOnly": { … } }`, replace the `"description"` value with:

```json
      "description": "The program files this {{command}} link points to are gone, so this link can't run (another installation may still work in Terminal). If an earlier uninstall stopped partway, they may be in the Trash. Uninstall moves the link to the Trash too. If you didn't mean to remove {{source}}, put its folder back from the Trash and refresh, or reinstall it by following the install steps in {{source}}'s official documentation."
```

In `src/i18n/zh-CN.json`, the same key:

```json
      "description": "这个 {{command}} 链接指向的程序文件已经不在了，所以这个链接无法运行（另一份安装在「终端」里可能仍然可用）。如果是上次卸载中途停下，它们可能在废纸篓里。卸载会把这个链接也移到废纸篓。如果你并不想删掉 {{source}}，把它的文件夹从废纸篓放回去再刷新，或者按 {{source}} 官方文档里的安装步骤重新安装。"
```

In `src/lib/sources.ts`, in `sourceNoticesFor`'s `LauncherOnly` branch, replace the comment

```ts
      // The half-uninstalled state: typing the command now fails, so a
      // warning. No button, and no promise of one: until step C the row's
      // artifact carries NoSafeMethod, so the gate refuses an uninstall
      // and the Installed page shows none. C's path-list uninstall is
      // what finishes the job (spec §3.3, §6.1).
```

with

```ts
      // The half-uninstalled state: this launcher cannot run, so a
      // warning. No button on the notice: the row's own Uninstall finishes
      // the job -- its preview lists the program directory as already gone
      // and moves the link (spec §3.3, §6.2).
```

In `src/lib/types.ts`, in `UninstallBlocked`'s doc comment, replace

```ts
 * the standalone adapter's inventory for a tool with no uninstall command
 * and no safe way yet to remove its files (Claude Code, phase 4 step B,
 * until step C). Read through
```

with

```ts
 * the standalone adapter's inventory for a tool with no uninstall command
 * and no safe way yet to remove its files (a recipe with no uninstall
 * method: none in the first batch since phase 4 step C gave Claude Code
 * its path list; the second batch's Ollama.app). Read through
```

In `crates/banager-core/src/model.rs`:

replace `InstanceNote::LauncherOnly`'s doc comment (B's, from `/// The launcher is still there but points at program files that are` through `/// \`LauncherOnly\`.`) with

```rust
    /// The launcher is still there but points at program files that are
    /// gone: the program directory was removed by hand or by another tool,
    /// or by a Canager uninstall that stopped after moving it and before
    /// moving the launcher -- the removal order (`removal::execute_removal`,
    /// launcher last) makes that the only state a stopped run leaves. The
    /// row stays, with no version, so the state is visible, and its
    /// artifact carries no `uninstall_blocked`: the row's Uninstall lists
    /// the program directory as already gone and moves the link (spec
    /// Q17). Produced by `StandaloneAdapter::detect` when `route::probe`
    /// answers `LauncherOnly`.
```

replace `UninstallBlocked::NoSafeMethod`'s doc comment (from `/// The tool has no uninstall command, and Canager has no safe way yet` through `/// src/lib/sources.ts).`) with

```rust
    /// The tool has no uninstall command and Canager has no safe way to
    /// remove its files -- no verified list of them, or no way yet to move
    /// them to the Trash -- so it does not offer to. Per artifact, not the
    /// instance's `read_only_reason`: that would hide the upgrade too,
    /// which works. Produced by `StandaloneAdapter::inventory`
    /// (`adapters/standalone/mod.rs`) for a recipe whose `uninstall` is
    /// `None`. No first-batch recipe has one since phase 4 step C gave
    /// Claude Code its path list; the second batch's Ollama.app will (spec
    /// §十), and `NO_UNINSTALL` in that module's tests keeps the path
    /// exercised. The gate refuses it (`blocked_uninstall` in
    /// session/plans.rs), the Installed page hides the button and says why
    /// (`UNINSTALL_BLOCKED_KEYS` in src/lib/sources.ts).
```

and in `CancelPolicy::KillThenReconcile`'s doc comment, after `/// adapter builds today says this (pip's \`plan()\` builds none).` add

```rust
    /// A path-list uninstall (`PlanAction::TrashPaths`) has no process to
    /// stop: `removal::execute_removal` watches the token between items and
    /// stops there -- a move already handed to the system is waited for --
    /// and `run_operation` reads the disk the same way.
```

In `crates/banager-core/src/adapters/standalone/route.rs`, replace the doc comment directly above `LauncherOnly,` in `pub enum Probe` — as landed in `71eacd0`:

```rust
    /// A dangling launcher whose own text points into the root: the
    /// program files are gone (removed by hand or by another tool, or --
    /// from step C -- by an uninstall that stopped partway), the link is
    /// left. Listed with no version and `InstanceNote::LauncherOnly` so the
    /// state is visible; in this step the artifact still carries
    /// `NoSafeMethod`, and step C's path-list uninstall is what removes the
    /// link.
```

with

```rust
    /// A dangling launcher whose own text points into the root: the
    /// program files are gone (removed by hand or by another tool, or by
    /// an uninstall that stopped partway), the link is left. Listed with no
    /// version and `InstanceNote::LauncherOnly` so the state is visible;
    /// the path-list uninstall (`removal::plan_removal` asks this same
    /// question) lists the program directory as already gone and moves the
    /// link. Because a launcher must be one link straight into its root
    /// (`probe_strict`), a stopped uninstall leaves this state, never an
    /// `Absent` that would read as finished.
```

- [ ] **Step 4: Run to verify it passes**

Run: `pnpm typecheck && pnpm exec vitest run src/lib src/pages/InstalledPage.test.tsx src/i18n`
Expected: PASS — including `completeness.test.ts` (no key added or removed) and `no-literal-strings.test.ts`.

- [ ] **Step 5: Continue the task**

No commit: continue to stage 6g.

#### Stage 6g: the trust file and the README say what this commit does

Stages 6a–6f make Canager move files, so this commit is also where the trust file stops saying that Canager never moves a file and that Claude Code has no uninstall, and where the README's row and safety bullets change (Ruling 23). Every sentence below describes code that exists after stages 6a–6f and names the function it describes (the trust file's own rule, its intro); where one rests on the spike, it says so and says what the spike did not reach. The smoke test's sentences wait for the test (Task 7); the pre-merge check's result is the author's to add.

- [ ] **Step 1: Write the failing tests**

In `crates/banager-core/tests/what_we_run_test.rs`, in the module doc, replace

```rust
//! enforces, and the one thing the allowlist refuses that a reader would
//! not expect: an `https://` `OLLAMA_HOST`. A source, host, variable or
//! limit added or changed without its line in the document fails here.
```

with

```rust
//! enforces, the one thing the allowlist refuses that a reader would not
//! expect (an `https://` `OLLAMA_HOST`), every path Claude Code's uninstall
//! moves or keeps with that uninstall's time budget, and the call Canager
//! makes to move a file to the Trash with the pause after each such move.
//! A source, host, variable, limit, path or pause added or changed without
//! its line in the document fails here.
```

(If A's wording of those three lines has moved, keep A's sentence and add the same clauses to it.) Add to the `use` lines, after `use banager_core::adapters::npm::NpmAdapter;`:

```rust
use banager_core::adapters::standalone::recipe::Uninstall;
use banager_core::adapters::standalone::recipes::CLAUDE;
use banager_core::adapters::standalone::removal::{PUT_BACK_SETTLE, TIMEOUT_SECS};
```

and append at the end of the file:

```rust

#[test]
fn test_what_we_run_names_every_path_claude_codes_uninstall_moves_or_keeps() {
    // The list is the recipe's, and a reader deciding whether to press
    // Uninstall reads it here: a path added to or dropped from
    // `CLAUDE.uninstall` without this section changing is a trust file
    // that no longer says what Canager moves.
    let doc = read_doc();
    let body = section_body(&doc, "Claude Code")
        .unwrap_or_else(|| panic!("docs/what-we-run.md has no `## Claude Code` section"));
    let Some(Uninstall::Paths { remove, keep }) = &CLAUDE.uninstall else {
        panic!("CLAUDE carries a path-list uninstall since phase 4 step C");
    };
    let listed = remove
        .iter()
        .map(|spec| spec.path)
        .chain(keep.iter().map(|spec| spec.path));
    for path in listed {
        assert!(
            body.contains(&format!("`{path}`")),
            "the `## Claude Code` section of docs/what-we-run.md does not name `{path}`, which CLAUDE.uninstall lists"
        );
    }
    let budget = format!("{TIMEOUT_SECS} s");
    assert!(
        body.contains(&budget),
        "the `## Claude Code` section of docs/what-we-run.md does not state the uninstall's budget, {budget:?} (removal::TIMEOUT_SECS)"
    );
}

#[test]
fn test_what_we_run_states_the_trash_call_and_the_pause_after_each_move() {
    let doc = read_doc();
    let body = section_body(&doc, "Moving files to the Trash").unwrap_or_else(|| {
        panic!("docs/what-we-run.md has no `## Moving files to the Trash` section for trash::RealTrasher")
    });
    // Hard-wrapped prose: compare with the line breaks folded away.
    let folded = body.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        folded.contains("trashItemAtURL:"),
        "the `## Moving files to the Trash` section of docs/what-we-run.md does not name the call RealTrasher makes"
    );
    let pause = format!("{} seconds", PUT_BACK_SETTLE.as_secs());
    assert!(
        folded.contains(&pause),
        "the `## Moving files to the Trash` section of docs/what-we-run.md does not state the pause {pause:?} (removal::PUT_BACK_SETTLE)"
    );
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p banager-core --test what_we_run_test`
Expected: FAIL — `the \`## Claude Code\` section of docs/what-we-run.md does not name \`~/.claude/downloads\`, which CLAUDE.uninstall lists` (B's section already names the launcher and the program directory, not the cache or the kept paths), and `docs/what-we-run.md has no \`## Moving files to the Trash\` section for trash::RealTrasher`. A's tests and B's still pass.

- [ ] **Step 3: Write the trust file**

In `docs/what-we-run.md`:

(a) In the opening paragraph, change `every file it reads or writes,` to `every file it reads, writes or moves to the Trash,`, and change the end of its test sentence, `and that the unknown-source scan's section states the two limits the code enforces.`, to `that the unknown-source scan's section states the two limits the code enforces, that Claude Code's section names every path its uninstall moves or keeps and that uninstall's time budget, and that the Trash section names the call and states the pause after each move.`

(b) Under `## When commands run`:
- change `no refresh runs a write command, launches an application or asks for a password.` to `no refresh runs a write command, moves a file, launches an application or asks for a password.`
- change `**An operation** is previewed first: \`plan\` builds the exact argv and the front end shows it` to `**An operation** is previewed first: \`plan\` builds the exact argv — or, for an uninstall that runs no command, the exact list of paths it will move to the Trash (Claude Code's section) — and the front end shows it`
- change `takes the plan's locks, runs the command, and then re-reads the inventory` to `takes the plan's locks, runs the command (or moves the listed paths to the Trash), and then re-reads the inventory`

(c) In B's `## Claude Code` section:

- In the **Detect.** paragraph, replace `checks with \`lstat\`, \`readlink\` and \`realpath\` that it is a symbolic link resolving into \`~/.local/share/claude\` (the installer's \`versions/<version>\` store).` with `checks with \`lstat\`, \`readlink\` and \`realpath\` that it is a symbolic link whose own text points into \`~/.local/share/claude\` (the installer's \`versions/<version>\` store) and that resolves there; a \`claude\` that reaches that folder only through another link outside it is not the installer's layout and is not listed (the Unknown page shows it).` (`route::probe`, Ruling 27.)
- In the same paragraph, replace `(the program files were removed by hand or by another tool) is listed with no version and a notice saying so; in this step Canager cannot remove the link either (see the write commands below).` with `(the program files were removed by hand or by another tool, or by an uninstall that stopped partway) is listed with no version and a notice saying so, and its Uninstall moves the link to the Trash (below).`
- In the **Write commands** table, after the row `| Upgrade | \`<claude> update\` | 1800 s | No |`, add:

```markdown
| Uninstall | none: Canager moves up to three paths to the Trash itself (below) | 120 s; Canager stops between items once it is spent | No |
```

- In the paragraph after that table, replace its last sentence, `There is no install (the installer is Anthropic's, not Canager's) and, in this step, no uninstall: Claude Code has no uninstall command, and until Canager can move its files to the Trash itself (phase 4 step C) the row says it cannot be uninstalled here and offers no button — \`Session::issue_plan\` refuses it as well.`, with `There is no install: the installer is Anthropic's, not Canager's.`, and after that paragraph insert:

```markdown

**Uninstall.** Claude Code has no uninstall command. Anthropic's own
instructions ("Uninstall Claude Code → Native" on
code.claude.com/docs/en/setup) are two `rm` commands; Canager runs
neither and instead moves the same paths, plus the installer's download
cache, to the Trash itself (`CLAUDE.uninstall` in `recipes.rs`; how, in
"Moving files to the Trash" below), in this order:

| Path | What it is | If it is not there |
|---|---|---|
| `~/.local/share/claude` | the program files, every downloaded version | refused — unless the launcher is still there and points into it, the state an uninstall that stopped partway leaves: then the preview says it is already gone |
| `~/.claude/downloads` | the installer's download cache (install.sh's `DOWNLOAD_DIR`) | skipped |
| `~/.local/bin/claude` | the launcher, the link that runs when `claude` is typed — last, so a stop partway always leaves it | refused |

It keeps `~/.claude` — settings, login, history and projects, which
Claude Code's VS Code extension, JetBrains plugin and desktop app use
too; of that folder only `downloads`, above, is moved — and
`~/.claude.json` (settings), and the preview names each of the two that
exists. Before the preview is shown every listed path is checked
(`removal::plan_removal`): the folder it is in, with every link
resolved, must be inside the home folder and be neither the home folder
itself nor one of the folders directly in it that many tools share
(`~/.local`, `~/.config`, `~/.cache`, `~/Library`, `~/.cargo`); every
folder between the home folder and the path must be a real folder, not
a link — so a `~/.local/bin` kept as a link to a dotfiles folder
refuses the uninstall, and so does a `~/.claude` that is a link when
the download cache is inside it; the path must belong to the user
Canager runs as; it must be what the instructions describe — the
program files and the download
cache real folders, the launcher one symbolic link straight into
`~/.local/share/claude`; and, with every link resolved, moving it must
not take `~/.claude` or `~/.claude.json` along (of `~/.claude`, only
`downloads` lies inside it, as listed). If any check fails, the whole
uninstall is refused, in the user's language, and nothing is moved. The
preview also records what each path is — its device, inode and kind,
from `lstat` — and Canager keeps that with the plan it issued, never
sending it to the window. When the preview is confirmed the list is
built again from the disk (`removal::execute_removal`): if a check now
fails, if the list is not the one the preview showed, or if any path is
no longer the one the preview recorded, nothing is moved — so if Claude
Code updated itself between the preview and the click (its updater
re-points the launcher), the uninstall stops and asks for a fresh look
at the preview. Then, right before each path is moved — after the pause
that follows the move before it — every check runs again on that path,
and it is compared once more with what the preview recorded; if
anything differs the uninstall stops before moving it
(`Fault::PathChanged`, naming the path), and the operation log lists
every path already moved. Canager checks each item immediately before
moving it; a program running as you that swaps the item in that instant
could still race it. The launcher is last, so a stop partway — macOS
refusing an item (its own words are shown), Cancel, or Canager stopping
between items once the 120 s budget is spent (a move already under way
is always finished first) — always leaves it: a stop before the first
move changes nothing, and the row stays as it was; once the program
files are in the Trash, the next refresh shows the launcher-only row,
and its Uninstall lists them as already gone and moves the rest.
Afterwards Canager looks for the launcher again
(`reconcile_after_uninstall`): the uninstall is reported as succeeded
only when it is gone, and as unconfirmed when Canager cannot tell (a
folder it may not read, say).
```

(d) Under `## Files Canager reads`, at the end of B's bullet `- Claude Code: whether \`~/.local/bin/claude\` exists and where it links to …` (the one that ends `a missing file or key means \`latest\`).`), add a sentence to the same bullet:

```markdown
  For an uninstall preview, when it is confirmed, and again right before
  each path is moved: `lstat` and the resolved path of each path on the
  uninstall list and of the folder it is in, the resolved home folder and
  the shared folders in it, the launcher's link text, and whether
  `~/.claude` and `~/.claude.json` exist and where they lead (Claude
  Code's section). After an uninstall: `lstat`, `readlink` and `realpath`
  of the launcher, and nothing else.
```

(e) Under `## Files Canager writes`, replace the sentence `Nothing else on the Mac is written, moved or deleted by Canager itself: every change to what is installed is made by the tool named in the preview, running the command shown there.` with:

```markdown
Nothing else on the Mac is written or deleted by Canager itself. It
moves files in one case: a confirmed uninstall of a tool that has no
uninstall command (Claude Code, today) moves the paths its preview
listed to the Trash (next section). Every other change to what is
installed is made by the tool named in the preview, running the command
shown there.
```

(f) After the `## Files Canager writes` section (before `## Network: Canager only connects to these hosts`), insert:

```markdown
## Moving files to the Trash

`RealTrasher` (`crates/banager-core/src/trash/real.rs`) is the only code
in Canager that changes a file on the Mac other than its own settings.
It makes one call per path, `NSFileManager
trashItemAtURL:resultingItemURL:error:` — the call Finder makes for Move
to Trash — through the `objc2-foundation` crate, and it is called only by
a confirmed path-list uninstall (`removal::execute_removal`, Claude
Code's section), for each path right after that path's last check. It
never deletes anything, never empties the Trash and never renames a file
itself, and a symbolic link is moved as the link, never its target: the
item's kind comes from the `lstat` that ends its last check, so a link is
never handed to the system as a folder, and nothing else looks at the
path between that check and the call. The call itself takes a path, so
one gap remains: Canager checks each item immediately before moving it;
a program running as you that swaps the item in that instant could still
race it. Each move is written to the operation log with where the item
now is (`LogNote::MovedToTrash`); an item macOS refuses stops the
uninstall there, with macOS's own reason (`LogNote::TrashFailed`).
After each move Canager waits 3 seconds
(`removal::PUT_BACK_SETTLE`) — before the next one, and before it
reports the uninstall finished; Cancel ends the wait, and no wait
outlasts the uninstall's time budget — and the second finding below says
why. A debug build of Canager, never a release one, also tries to list
the Trash after each move and prints whether it may; that is how the
pre-merge check learns the build it ran had no Full Disk Access.

How this was verified, on 2026-09-25, with a small test app on macOS
27.0 (build 26A428), Apple silicon — ad-hoc signed, launched the way
Finder launches an app (through LaunchServices), and without Full Disk
Access, which that same process confirmed in every run by being refused
a listing of `~/.Trash`:

- It moved a file, a folder and a symbolic link to the Trash with this
  call in 20 runs out of 20: no dialog, no error, the link moved as a
  link with its target left in place, and a name already in the Trash
  given the system's own time-of-day suffix — so Claude Code's two paths
  named `claude` both arrive.
- Finder keeps Put Back as a record per item in `~/.Trash/.DS_Store`.
  Every item got one when the calls were at least 2 seconds apart (4 runs
  out of 4); when they came 1.5 seconds apart or less, only the first
  item of the burst did (15 runs out of 15). Hence the 3-second pause: it
  makes Put Back likely for every item, not certain, and an item without
  the record can still be dragged back out of the Trash by hand. The
  runs that recorded every item also kept running for 3 seconds after the
  last call, and the record is written after the call returns — with Full
  Disk Access, a process that quit at once lost the later records — so
  Canager waits after the last move too, and quitting Canager while an
  uninstall is still running may leave the item it moved last without
  Put Back. Why macOS behaves this way is not known: the pause is a
  measurement on one Mac, not a documented guarantee.
- A plain `rename` into `~/.Trash` from the same process succeeded too
  (24 runs out of 24), where the design had expected it to be refused:
  the Trash's protection covers listing it, not adding to it, so a `mv`
  could have reached it. Canager does not use one anyway: a renamed item
  gets no Put Back record, and one `mv` of Claude Code's two paths named
  `claude` collides on the name — `mv -n` skips the second and still
  reports success.

Not verified by that app: a click on Put Back itself (the records were
checked, not used), a build of Canager itself, a symbolic link whose
target is gone — which is what every Claude Code uninstall moves last:
the launcher, after the program files it points to — other macOS
versions, and Intel Macs.
```

(g) Under `## What Canager never does`, replace the bullet `- Never writes, moves or deletes a file on the Mac itself, other than its own \`settings.json\`; never edits a shell startup file.` with:

```markdown
- Never deletes a file and never empties the Trash. Never writes a file
  on the Mac itself other than its own `settings.json`, and moves files
  only to the Trash, only for an uninstall the user confirmed, and only
  the paths its preview listed; never edits a shell startup file.
- Never moves anything outside the home folder, anything directly in the
  home folder or in a folder many tools share there (`~/.local`,
  `~/.config`, `~/.cache`, `~/Library`, `~/.cargo`), anything reached
  through a folder that is a link, anything that does not belong to the
  user, or anything that is not what the tool's uninstall instructions
  describe; never moves the settings, login and history Claude Code keeps
  in `~/.claude` (of that folder only its download cache,
  `~/.claude/downloads`) or `~/.claude.json`, nor anything they lead to.
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p banager-core --test what_we_run_test`
Expected: PASS — A's tests, B's, and the two new ones. Then check that no sentence in the file still says Claude Code cannot be uninstalled. The file is hard-wrapped, so fold it first:

```bash
tr '\n' ' ' < docs/what-we-run.md | tr -s ' ' | grep -o "in this step, no uninstall\|cannot be uninstalled here\|cannot remove the link either\|offers no button"
```

Expected: no output. (B's section had all four phrases; `no uninstall` alone is no test — "Claude Code has no uninstall command", true, stays in the new text.)

- [ ] **Step 5: The README's row and safety bullets**

In `README.md`:

(a) Replace B's row

```markdown
| Claude Code — the native install, via its own installer | yes | updates yes; install no (the installer is Anthropic's, and Canager never runs it); uninstall not yet — the row says so and offers no button |
```

with

```markdown
| Claude Code — the native install, via its own installer | yes | updates yes; install no (the installer is Anthropic's, and Canager never runs it); uninstall yes — its program files, download cache and launcher go to the Trash, and your settings and history stay |
```

(b) Under `## What makes it safe to point at your machine`, in the bullet `**You see the exact command before it runs.**`, change `Every update and uninstall shows its real argv and whether it needs your password.` to `Every update and uninstall shows its real argv and whether it needs your password — or, for the one uninstall that runs no command, the exact paths it will move to the Trash.` Then, after the bullet `- **Nothing is deleted quietly.** An uninstall that would break other packages says which ones, in your language.`, add:

```markdown
- **A tool with no uninstall command goes to the Trash, not away.** Claude Code's makers document
  its removal as a list of paths. Canager moves those paths, plus its installer's download cache,
  to the Trash itself, with the call Finder uses, so until you empty the Trash you can drag them
  back — and Finder's Put Back will likely work too; the preview lists each path it will move and
  each one it keeps (your settings and history, in `~/.claude` and `~/.claude.json`). It is the
  only change Canager makes to a file itself besides saving its own settings, and
  `docs/what-we-run.md` says how.
- **Only the paths you were shown are moved.** Each path must be inside your home folder — never
  directly in it or in a folder other apps share, such as `~/.local` or `~/Library`, and never
  through a folder that is a link — yours, what the instructions describe, and clear of what it
  keeps. Canager remembers what each path was when you saw the preview; when you confirm, and
  again right before each path moves, it checks everything once more, and if anything differs it
  stops before moving that path, and the operation log lists anything it had already moved.
```


- [ ] **Step 6: Continue the task**

No commit: continue to stage 6h.

#### Stage 6h: format, gates, commit

- [ ] **Step 1: Format and run the gates**

Run: `cargo fmt --all`, then all five gates from Global Constraints.
Expected: all clean. Notes for clippy: `execute_removal` has seven parameters, the `too_many_arguments` threshold, not above it (`Job`, `Confirmed` and `Pacing` are why it is not twelve); `RealTrasher::new` and `MockTrasher::new` have `Default`; `TrashError::Unsupported` is a `pub` variant, so its absence on macOS is not dead code, and `ItemKind::{File, Other}` are `pub` variants `identity_of` produces; `report_trash_access` exists only under `cfg(all(target_os = "macos", debug_assertions))`, exactly where its one caller does, so a release build has no dead code either (checked both ways in the scratch crate); `pause`, `take_turn`, `check_item`, `kept_places`, `disturbed`, `is_shared_folder`, `identity_of`, `spelled`, `changed` and `one_hop` are private helpers with callers; the dev-dependency on the crate itself is clean under `cargo clippy --workspace --all-targets -- -D warnings` (Ruling 20).

- [ ] **Step 2: Commit**

```bash
git add crates/banager-core/Cargo.toml Cargo.lock crates/banager-core/src/lib.rs crates/banager-core/src/trash/mod.rs crates/banager-core/src/trash/real.rs crates/banager-core/src/trash/mock.rs crates/banager-core/src/adapters/standalone/recipe.rs crates/banager-core/src/adapters/standalone/recipes.rs crates/banager-core/src/adapters/standalone/removal.rs crates/banager-core/src/adapters/standalone/mod.rs crates/banager-core/src/adapters/standalone/route.rs crates/banager-core/src/scan/mod.rs crates/banager-core/src/model.rs crates/banager-core/src/adapters/mod.rs crates/banager-core/src/session/mod.rs crates/banager-core/tests/ops_upgrade_version_test.rs crates/banager-core/tests/ops_summaries_test.rs crates/banager-core/tests/what_we_run_test.rs adapters/fixtures/standalone-claude src/lib/types.ts src/lib/sources.ts src/lib/sources.test.ts src/pages/InstalledPage.test.tsx src/i18n/en.json src/i18n/zh-CN.json docs/what-we-run.md README.md
git commit -m "$(cat <<'EOF'
Uninstall Claude Code by moving its three paths to the Trash, launcher last

Claude Code has no uninstall command; Anthropic's instructions are two
paths, and its installer stages downloads in a third. The recipe now
carries that list, and the standalone adapter plans it as a TrashPaths
plan: the preview names each path it will move and each it keeps, in the
user's language, after checks -- inside the home folder, not directly in
it or in a folder many tools share, reached only through real folders,
present, the user's own, the shape the instructions describe, and clear
of every path it keeps -- that refuse the whole uninstall with the
reason otherwise. What the preview saw at each path (device, inode,
kind) stays with the issued plan on this side and never reaches the
window. Running it checks everything again and compares each path with
the preview before anything moves; then, for each item on the blocking
pool, runs every check once more and hands it to macOS's own
move-to-Trash through a new Trasher seam with nothing in between,
pauses after each item so Finder is likely to record Put Back, and
stops at the first refusal, Cancel or spent budget, finishing a move
already under way. The launcher goes last and must be one link straight
into its root, so a run that stops partway leaves one state -- files in
the Trash, the link dangling -- which the row shows as launcher-only,
verified after an uninstall by presence alone (an unreadable launcher is
unconfirmed, never gone), and which a second uninstall finishes.
Session::new injects the real Trash; the test Trash is compiled only for
tests. The launcher-only notice, the
trust file and the README say what Uninstall now does, in this same
commit: what is moved and kept, the checks, what a stop partway leaves,
the one call that moves a file and the pause after it, and what Canager
still never does.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

`git add adapters/fixtures/standalone-claude` stages only the README change inside that directory (nothing else there changed); check with `git status --short` before committing that no other path is staged.

---

### Task 7: End to end through `Session`; the `#[ignore]` `RealTrasher` smoke; the CI step; the smoke test's lines in the documents

**Files:**
- Create: `crates/banager-core/tests/standalone_uninstall_test.rs`
- Modify: `.github/workflows/ci.yml` — one step after `live homebrew smoke (install/inventory/uninstall hello)`
- Modify: `docs/what-we-run.md` — one paragraph at the end of `## Moving files to the Trash` (stage 6g wrote the section)
- Modify: `README.md` — the ignored-tests paragraph and its code block; "plus 2 more" in the status block, en and zh  [B's + F's file: anchor by quoted text]
- Test: `tests/standalone_uninstall_test.rs`.

**Interfaces:**
- Consumes (all public, from Task 6 and B): `banager_core::adapters::standalone::{StandaloneAdapter, recipes::CLAUDE}`, `StandaloneAdapter::{new, with_trash_gap}`, `banager_core::trash::{MockTrasher, RealTrasher, TrashError, Trasher}`, `banager_core::model::ItemKind` (`MockTrasher` reaches `tests/` through the crate's dev-dependency on itself with `test-support`, stage 6a), `banager_core::session::Session::{with_adapters, refresh, issue_plan, submit, cancel, operations}`, `banager_core::runner::{HostEnv, MockRunner, CommandOutput}`, `banager_core::http::{MockHttpClient, HttpResponse}`, `banager_core::model::*`.
- Produces: tests only (no production item); a CI step that runs the `#[ignore]`d smoke with `CANAGER_LIVE=1`, the gate `brew_live`'s install test uses for a test that changes the machine; the trust file's and the README's sentences about that test (Ruling 23).

The home builder is this file's own: B's `standalone::testing::TempHome` is `#[cfg(test)] pub(super)`, invisible to `tests/`. It follows `tests/unknown_scan_test.rs`'s `Home` (canonical temp dir, `euid` from the home's owner).

- [ ] **Step 1: Write the tests**

Create `crates/banager-core/tests/standalone_uninstall_test.rs`:

```rust
//! A path-list uninstall end to end (phase 4 step C): the real
//! `StandaloneAdapter` over a synthetic native Claude Code layout in a
//! throwaway home, driven the way the window drives it -- `Session::refresh`,
//! `issue_plan` (the actionability gate), `submit`, `run_operation`
//! (`execute`, then the reading after an uninstall) -- with `MockTrasher`
//! standing in for the Trash, so nothing here touches anyone's Trash. The
//! last test is the exception, `#[ignore]`d and gated on `CANAGER_LIVE=1`
//! like `brew_live`'s install test: `RealTrasher` moving five throwaway
//! items it makes into the real Trash of the Mac running it (CI's runner,
//! whose Trash is discarded with it; on a developer's Mac, once, by hand).
//!
//! No recorded fixture: nothing here runs a command but the scripted
//! `--version`, and every layout is built by the test (spec §9.3). Every
//! `#[tokio::test]` here runs on tokio's default current-thread runtime,
//! which the cancel test relies on: a submitted operation does not start
//! until the test awaits.

use banager_core::adapters::standalone::recipes::CLAUDE;
use banager_core::adapters::standalone::StandaloneAdapter;
use banager_core::adapters::{Adapter, CheckOptions};
use banager_core::events::{OpId, VecSink};
use banager_core::http::{HttpResponse, MockHttpClient};
use banager_core::model::{
    ArtifactKind, Fault, InstanceNote, ItemKind, KeptWhat, OpKind, OpRequest, OpStatus, Outcome,
    PlanAction, RemovedWhat, Warning,
};
use banager_core::runner::{CommandOutput, HostEnv, MockRunner};
use banager_core::session::Session;
use banager_core::trash::{MockTrasher, TrashError, Trasher};
use std::collections::BTreeSet;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, Weak};
use std::time::{Duration, Instant};

/// A fresh home directory for one test, removed when the test ends.
/// Canonical, so the paths a test builds compare equal to what the
/// adapter resolves (macOS's `/var/folders` is `/private/var/…`).
struct Home(PathBuf);

impl Home {
    fn new(tag: &str) -> Home {
        let raw = std::env::temp_dir().join(format!(
            "canager-uninstall-{tag}-{}-{}",
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

    /// `HostEnv` for this home, as the user who owns it (the one running
    /// the test), with nothing on `PATH`.
    fn env(&self) -> HostEnv {
        HostEnv {
            path_dirs: Vec::new(),
            home: self.0.clone(),
            euid: std::fs::metadata(&self.0).expect("stat home").uid(),
            cargo_home: None,
            ollama_host: None,
        }
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The native Claude Code layout the installer writes (an executable
/// `versions/<v>` that is never run -- `--version` is scripted -- and an
/// absolute link to it), plus the download cache and the settings and
/// history a real one has: returns the launcher.
fn claude_layout(home: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let real = home.join(".local/share/claude/versions/2.1.281");
    std::fs::create_dir_all(real.parent().unwrap()).expect("versions dir");
    std::fs::write(&real, b"#!/bin/sh\n").expect("the program");
    std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o755)).expect("executable");
    let launcher = home.join(".local/bin/claude");
    std::fs::create_dir_all(launcher.parent().unwrap()).expect("bin dir");
    std::os::unix::fs::symlink(&real, &launcher).expect("the launcher");
    std::fs::create_dir_all(home.join(".claude/downloads")).expect("the cache");
    std::fs::create_dir_all(home.join(".claude/projects/p")).expect("projects");
    std::fs::write(home.join(".claude/projects/p/session.jsonl"), b"{}\n").expect("history");
    std::fs::write(home.join(".claude.json"), b"{}\n").expect("settings");
    launcher
}

/// Every entry under `root`, relative, links not followed: what "nothing
/// else in the home changed" is compared by.
fn tree(root: &Path) -> BTreeSet<PathBuf> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeSet<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("read_dir") {
            let path = entry.expect("entry").path();
            out.insert(path.strip_prefix(root).unwrap().to_path_buf());
            if std::fs::symlink_metadata(&path).unwrap().is_dir() {
                walk(root, &path, out);
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(root, root, &mut out);
    out
}

/// A session over one standalone Claude Code adapter whose `--version` is
/// scripted, whose update check answers "no newer version", and whose Trash
/// is `trasher`, with no pause after each item.
fn session_with(launcher: &Path, trasher: Arc<dyn Trasher>) -> Arc<Session> {
    let runner = Arc::new(MockRunner::new());
    runner.respond(
        vec![launcher.to_str().unwrap(), "--version"],
        CommandOutput {
            exit_code: Some(0),
            stdout: "2.1.281 (Claude Code)\n".to_string(),
            stderr: String::new(),
            timed_out: false,
            cancelled: false,
        },
    );
    let http = Arc::new(MockHttpClient::new());
    http.respond(
        "https://downloads.claude.ai/claude-code-releases/latest",
        HttpResponse {
            status: 200,
            body: "2.1.281\n".to_string(),
        },
    );
    let adapter = StandaloneAdapter::new(&CLAUDE, runner, http, trasher)
        .with_trash_gap(Duration::ZERO);
    Session::with_adapters(
        Arc::new(VecSink::new()),
        vec![Arc::new(adapter) as Arc<dyn Adapter>],
        None,
    )
}

fn uninstall() -> OpRequest {
    OpRequest {
        kind: OpKind::Uninstall,
        instance_id: "standalone-claude".to_string(),
        artifact_kind: ArtifactKind::Binary,
        name: "claude".to_string(),
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
        assert!(Instant::now() < deadline, "the uninstall never finished");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn test_uninstalling_claude_code_moves_its_three_paths_and_keeps_its_settings() {
    let home = Home::new("full");
    let launcher = claude_layout(home.path());
    let trasher = Arc::new(MockTrasher::new());
    let session = session_with(&launcher, trasher.clone());

    let snapshot = session.refresh(&home.env(), &CheckOptions::default()).await;
    let row = snapshot
        .artifacts
        .iter()
        .find(|a| a.key.instance_id == "standalone-claude")
        .expect("Claude Code is listed");
    assert_eq!(row.uninstall_blocked, None, "the row offers Uninstall");
    let before = tree(home.path());

    let issued = session
        .issue_plan(&uninstall())
        .await
        .expect("the gate lets it through and the preview is built");
    let moved = vec![
        home.path().join(".local/share/claude"),
        home.path().join(".claude/downloads"),
        launcher.clone(),
    ];
    let PlanAction::TrashPaths { paths, previewed } = &issued.plan.action else {
        panic!("a path list, not a command: {:?}", issued.plan.action);
    };
    assert_eq!(paths, &moved);
    assert_eq!(previewed.len(), moved.len(), "what the preview saw, one per path");
    // What the window receives: the paths alone (Ruling 10).
    assert_eq!(
        serde_json::to_value(&issued).unwrap()["plan"]["action"],
        serde_json::json!({ "TrashPaths": { "paths": moved } })
    );
    assert_eq!(
        issued.plan.warnings,
        vec![
            Warning::WillTrash {
                path: "~/.local/share/claude".to_string(),
                what: RemovedWhat::Program
            },
            Warning::WillTrash {
                path: "~/.claude/downloads".to_string(),
                what: RemovedWhat::Cache
            },
            Warning::WillTrash {
                path: "~/.local/bin/claude".to_string(),
                what: RemovedWhat::Launcher
            },
            Warning::WillKeep {
                path: "~/.claude".to_string(),
                what: KeptWhat::SettingsAndHistory
            },
            Warning::WillKeep {
                path: "~/.claude.json".to_string(),
                what: KeptWhat::Settings
            },
        ]
    );
    let op_id = session.submit(issued.id).expect("submit");

    assert_eq!(outcome_of(&session, op_id).await, Outcome::Succeeded);
    // Exactly the previewed paths, in order, and nothing else.
    assert_eq!(trasher.calls(), moved);
    // Both `claude`s are in the Trash: the program directory under its own
    // name, the launcher -- a link, not its target -- under a suffixed one.
    assert!(trasher.bin().join("claude/versions/2.1.281").is_file());
    let link = trasher.bin().join("claude 2");
    assert!(std::fs::symlink_metadata(link).unwrap().file_type().is_symlink());
    // Everything else in the home is exactly as it was.
    let expected: BTreeSet<PathBuf> = before
        .into_iter()
        .filter(|rel| !moved.iter().any(|m| home.path().join(rel).starts_with(m)))
        .collect();
    assert_eq!(tree(home.path()), expected);
    assert!(home.path().join(".claude/projects/p/session.jsonl").is_file());
    // And the next refresh has no Claude Code row.
    let snapshot = session.refresh(&home.env(), &CheckOptions::default()).await;
    assert!(snapshot.instances.iter().all(|i| i.id != "standalone-claude"));
}

#[tokio::test]
async fn test_a_path_changed_after_the_preview_stops_the_uninstall_before_it_moves_anything() {
    // Between the preview and the click the launcher was re-pointed at a
    // Homebrew copy: not what the user confirmed. Nothing moves.
    let home = Home::new("changed");
    let launcher = claude_layout(home.path());
    let trasher = Arc::new(MockTrasher::new());
    let session = session_with(&launcher, trasher.clone());
    session.refresh(&home.env(), &CheckOptions::default()).await;
    let issued = session.issue_plan(&uninstall()).await.expect("preview");

    let cask = home
        .path()
        .join("opt/homebrew/Caskroom/claude-code/2.1.267/claude");
    std::fs::create_dir_all(cask.parent().unwrap()).unwrap();
    std::fs::write(&cask, b"#!/bin/sh\n").unwrap();
    std::fs::remove_file(&launcher).unwrap();
    std::os::unix::fs::symlink(&cask, &launcher).unwrap();
    let op_id = session.submit(issued.id).expect("submit");

    assert_eq!(
        outcome_of(&session, op_id).await,
        Outcome::CanagerFailed(Fault::PathChanged {
            path: "~/.local/bin/claude".to_string()
        })
    );
    assert!(trasher.calls().is_empty());
    assert!(home
        .path()
        .join(".local/share/claude/versions/2.1.281")
        .is_file());
}

#[tokio::test]
async fn test_a_self_update_between_the_preview_and_the_click_stops_the_uninstall_before_it_moves_anything(
) {
    // Ruling 10 end to end: what the preview saw travels in the plan
    // `issue_plan` stores and `submit` hands to the operation -- the window
    // never sees it. Claude Code updating itself in between re-points the
    // launcher at a new version inside its root: every check still passes
    // and the path is the same, but the link is not the one the user was
    // shown. Nothing moves, and the user previews again.
    let home = Home::new("self-updated");
    let launcher = claude_layout(home.path());
    let trasher = Arc::new(MockTrasher::new());
    let session = session_with(&launcher, trasher.clone());
    session.refresh(&home.env(), &CheckOptions::default()).await;
    let issued = session.issue_plan(&uninstall()).await.expect("preview");

    let newer = home.path().join(".local/share/claude/versions/2.1.282");
    std::fs::write(&newer, b"#!/bin/sh\n").unwrap();
    std::fs::remove_file(&launcher).unwrap();
    std::os::unix::fs::symlink(&newer, &launcher).unwrap();
    let op_id = session.submit(issued.id).expect("submit");

    assert_eq!(
        outcome_of(&session, op_id).await,
        Outcome::CanagerFailed(Fault::PathChanged {
            path: "~/.local/bin/claude".to_string()
        })
    );
    assert!(trasher.calls().is_empty());
    assert!(home
        .path()
        .join(".local/share/claude/versions/2.1.281")
        .is_file());
}

#[tokio::test]
async fn test_an_uninstall_macos_refuses_partway_leaves_a_launcher_only_row_that_a_second_uninstall_finishes() {
    // Review Focus 4: macOS refuses the second item. The launcher (last)
    // is still there, so the next refresh shows the launcher-only row --
    // with an Uninstall -- and a second uninstall lists the program
    // directory as already gone and finishes.
    let home = Home::new("refused");
    let launcher = claude_layout(home.path());
    let trasher = Arc::new(MockTrasher::new());
    trasher.refuse_call(
        1,
        "“downloads” couldn’t be moved to the Trash because you don’t have permission to access it.",
    );
    let session = session_with(&launcher, trasher.clone());
    session.refresh(&home.env(), &CheckOptions::default()).await;

    let first = session.issue_plan(&uninstall()).await.expect("preview");
    let op_id = session.submit(first.id).expect("submit");
    assert_eq!(
        outcome_of(&session, op_id).await,
        Outcome::Failed {
            exit_code: None,
            summary: "“downloads” couldn’t be moved to the Trash because you don’t have permission to access it.".to_string()
        }
    );

    let snapshot = session.refresh(&home.env(), &CheckOptions::default()).await;
    let row = snapshot
        .instances
        .iter()
        .find(|i| i.id == "standalone-claude")
        .expect("the row is still there");
    assert_eq!(row.status.notes, vec![InstanceNote::LauncherOnly]);
    let artifact = snapshot
        .artifacts
        .iter()
        .find(|a| a.key.instance_id == "standalone-claude")
        .expect("its artifact");
    assert_eq!(artifact.version, "");
    assert_eq!(artifact.uninstall_blocked, None, "and it offers Uninstall");

    let second = session.issue_plan(&uninstall()).await.expect("second preview");
    assert_eq!(
        second.plan.warnings[..3],
        [
            Warning::AlreadyGone {
                path: "~/.local/share/claude".to_string()
            },
            Warning::WillTrash {
                path: "~/.claude/downloads".to_string(),
                what: RemovedWhat::Cache
            },
            Warning::WillTrash {
                path: "~/.local/bin/claude".to_string(),
                what: RemovedWhat::Launcher
            },
        ]
    );
    let op_id = session.submit(second.id).expect("submit");
    assert_eq!(outcome_of(&session, op_id).await, Outcome::Succeeded);
    let snapshot = session.refresh(&home.env(), &CheckOptions::default()).await;
    assert!(snapshot.instances.iter().all(|i| i.id != "standalone-claude"));
}

/// A `MockTrasher` that presses Cancel on the running uninstall right after
/// it moves its first item. The operation to cancel is named after
/// `submit` returns and before the test awaits, which on the current-thread
/// runtime is before the operation starts.
struct CancellingTrasher {
    inner: MockTrasher,
    op: OnceLock<(Weak<Session>, OpId)>,
}

impl Trasher for CancellingTrasher {
    fn trash(&self, path: &Path, kind: ItemKind) -> Result<PathBuf, TrashError> {
        let moved = self.inner.trash(path, kind)?;
        if self.inner.calls().len() == 1 {
            let (session, op_id) = self.op.get().expect("the test named its operation");
            session
                .upgrade()
                .expect("the session is alive")
                .cancel(*op_id)
                .expect("a Running uninstall accepts Cancel");
        }
        Ok(moved)
    }
}

#[tokio::test]
async fn test_an_uninstall_cancelled_between_items_is_reported_cancelled_and_a_second_uninstall_finishes(
) {
    // Review Focus 4, the other half, and the retry Astra's finding 6 asked
    // for: the user's Cancel lands after the program directory went to the
    // Trash. The launcher is still there -- one link into its root, so
    // launcher-only rather than gone -- so the reading after the uninstall
    // says the item is present and the cancel is what happened
    // (`Cancelled`; never `Succeeded`: Task 2's reading), the next refresh
    // shows the launcher-only row, and pressing Uninstall again finishes
    // the job.
    let home = Home::new("cancelled");
    let launcher = claude_layout(home.path());
    let trasher = Arc::new(CancellingTrasher {
        inner: MockTrasher::new(),
        op: OnceLock::new(),
    });
    let session = session_with(&launcher, trasher.clone());
    session.refresh(&home.env(), &CheckOptions::default()).await;
    let issued = session.issue_plan(&uninstall()).await.expect("preview");

    let op_id = session.submit(issued.id).expect("submit");
    trasher
        .op
        .set((Arc::downgrade(&session), op_id))
        .expect("named once");

    assert_eq!(outcome_of(&session, op_id).await, Outcome::Cancelled);
    assert_eq!(
        trasher.inner.calls(),
        vec![home.path().join(".local/share/claude")]
    );
    assert!(std::fs::symlink_metadata(&launcher)
        .unwrap()
        .file_type()
        .is_symlink());
    let snapshot = session.refresh(&home.env(), &CheckOptions::default()).await;
    let row = snapshot
        .instances
        .iter()
        .find(|i| i.id == "standalone-claude")
        .expect("the row is still there");
    assert_eq!(row.status.notes, vec![InstanceNote::LauncherOnly]);

    // The retry: the program directory is already gone, the cache and the
    // launcher are what is left, and this time nothing stops it (the
    // trasher presses Cancel after its first call only).
    let second = session.issue_plan(&uninstall()).await.expect("second preview");
    assert_eq!(
        second.plan.warnings[..3],
        [
            Warning::AlreadyGone {
                path: "~/.local/share/claude".to_string()
            },
            Warning::WillTrash {
                path: "~/.claude/downloads".to_string(),
                what: RemovedWhat::Cache
            },
            Warning::WillTrash {
                path: "~/.local/bin/claude".to_string(),
                what: RemovedWhat::Launcher
            },
        ]
    );
    let op_id = session.submit(second.id).expect("submit");
    assert_eq!(outcome_of(&session, op_id).await, Outcome::Succeeded);
    assert_eq!(
        trasher.inner.calls(),
        vec![
            home.path().join(".local/share/claude"),
            home.path().join(".claude/downloads"),
            launcher.clone(),
        ]
    );
    let snapshot = session.refresh(&home.env(), &CheckOptions::default()).await;
    assert!(snapshot.instances.iter().all(|i| i.id != "standalone-claude"));
}

/// A `MockTrasher` that, right after its `lock_after_call`-th move, takes
/// every permission off `folder` -- so the reading after the uninstall
/// cannot tell whether the launcher is there -- and gives `0o755` back
/// when dropped, so the test home can be removed.
struct LockingTrasher {
    inner: MockTrasher,
    lock_after_call: usize,
    folder: PathBuf,
}

impl Trasher for LockingTrasher {
    fn trash(&self, path: &Path, kind: ItemKind) -> Result<PathBuf, TrashError> {
        use std::os::unix::fs::PermissionsExt;
        let moved = self.inner.trash(path, kind)?;
        if self.inner.calls().len() == self.lock_after_call {
            std::fs::set_permissions(&self.folder, std::fs::Permissions::from_mode(0o000))
                .expect("take the folder's permissions away");
        }
        Ok(moved)
    }
}

impl Drop for LockingTrasher {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&self.folder, std::fs::Permissions::from_mode(0o755));
    }
}

#[tokio::test]
async fn test_an_uninstall_whose_last_reading_cannot_tell_is_unconfirmed_not_succeeded() {
    // Ruling 27 end to end: every item moved and `execute` said
    // `Succeeded`, but the reading after it cannot look into `~/.local/bin`
    // (a permission error). "Could not tell" is not "gone": the outcome is
    // `Unconfirmed`, never `Succeeded` on `execute`'s word. (Permissions do
    // not stop root, so the check is skipped, and says so, when the tests
    // run as root.)
    let home = Home::new("unreadable-after");
    if std::fs::metadata(home.path()).expect("stat home").uid() == 0 {
        eprintln!("running as root: permissions stop nothing, check skipped");
        return;
    }
    let launcher = claude_layout(home.path());
    let trasher = Arc::new(LockingTrasher {
        inner: MockTrasher::new(),
        lock_after_call: 3,
        folder: home.path().join(".local/bin"),
    });
    let session = session_with(&launcher, trasher.clone());
    session.refresh(&home.env(), &CheckOptions::default()).await;
    let issued = session.issue_plan(&uninstall()).await.expect("preview");

    let op_id = session.submit(issued.id).expect("submit");

    assert_eq!(outcome_of(&session, op_id).await, Outcome::Unconfirmed);
    assert_eq!(trasher.inner.calls().len(), 3, "every item was moved");
}

/// Review Focus 8: the real call, on each kind of item a path-list
/// uninstall can move -- a file, a directory, a link to a file, a link to
/// a directory, and a dangling link (what the launcher is when every
/// uninstall moves it, last) -- each of which must land in `~/.Trash`, a link as the
/// link itself with its target left where it was (spec §9.4). The items
/// are made under the temp directory, which on a Mac is on the home
/// folder's volume, so the system moves them to `~/.Trash` (the spike saw
/// the same from `$TMPDIR`). Each call is told the item's kind, as the
/// removal's last check tells it -- a link as `Symlink` whatever it points
/// at. It changes the machine -- it leaves five
/// throwaway items, named `canager-trash-smoke-…`, in the Trash of the Mac
/// running it -- so, like `brew_live`'s install test, it also requires
/// `CANAGER_LIVE=1` and skips loudly without it.
#[cfg(target_os = "macos")]
#[test]
#[ignore = "moves five throwaway items into the real Trash; run with CANAGER_LIVE=1 cargo test -p banager-core --test standalone_uninstall_test -- --ignored"]
fn test_real_trasher_moves_each_kind_of_item_and_links_as_links() {
    use banager_core::trash::RealTrasher;

    if std::env::var("CANAGER_LIVE").as_deref() != Ok("1") {
        eprintln!("CANAGER_LIVE is not 1; skipping the real Trash smoke test");
        return;
    }
    let trash = PathBuf::from(std::env::var_os("HOME").expect("HOME is set")).join(".Trash");
    let scratch = Home::new("real-trash");
    let stem = format!(
        "canager-trash-smoke-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let dir = scratch.path();
    let file = dir.join(format!("{stem}-file.txt"));
    std::fs::write(&file, b"smoke\n").unwrap();
    let folder = dir.join(format!("{stem}-dir"));
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(folder.join("inner.txt"), b"inner\n").unwrap();
    let link_to_file = dir.join(format!("{stem}-link-to-file"));
    std::os::unix::fs::symlink(&file, &link_to_file).unwrap();
    let link_to_dir = dir.join(format!("{stem}-link-to-dir"));
    std::os::unix::fs::symlink(&folder, &link_to_dir).unwrap();
    let dangling = dir.join(format!("{stem}-dangling"));
    std::os::unix::fs::symlink(dir.join("gone"), &dangling).unwrap();

    let trasher = RealTrasher::new();
    // Links first, while their targets are still in place.
    for (link, target) in [
        (&link_to_file, Some(&file)),
        (&link_to_dir, Some(&folder)),
        (&dangling, None),
    ] {
        let trashed = trasher
            .trash(link, ItemKind::Symlink)
            .expect("moved to the Trash");
        assert!(
            trashed.starts_with(&trash),
            "{} went to {}, not under {}",
            link.display(),
            trashed.display(),
            trash.display()
        );
        assert!(
            std::fs::symlink_metadata(&trashed)
                .unwrap()
                .file_type()
                .is_symlink(),
            "{} arrived as a link",
            link.display()
        );
        assert!(
            std::fs::symlink_metadata(link).is_err(),
            "{} left its place",
            link.display()
        );
        if let Some(target) = target {
            assert!(target.exists(), "{}'s target stayed put", link.display());
        }
    }
    let trashed = trasher.trash(&folder, ItemKind::Dir).expect("the directory");
    assert!(trashed.starts_with(&trash));
    assert!(trashed.join("inner.txt").is_file());
    let trashed = trasher.trash(&file, ItemKind::File).expect("the file");
    assert!(trashed.starts_with(&trash));
    assert!(std::fs::symlink_metadata(&trashed).unwrap().is_file());
}
```

In `.github/workflows/ci.yml`, after the step

```yaml
      - name: live homebrew smoke (install/inventory/uninstall hello)
        env:
          CANAGER_LIVE: "1"
        run: cargo test -p banager-core --test brew_live -- --ignored --nocapture
```

insert:

```yaml

      # The one test that calls macOS's real move-to-Trash: it moves five
      # throwaway items it makes (a file, a directory, and three kinds of
      # link) into the runner's Trash, which is discarded with the runner.
      - name: real Trash smoke (moves five throwaway items)
        env:
          CANAGER_LIVE: "1"
        run: cargo test -p banager-core --test standalone_uninstall_test -- --ignored --nocapture
```

- [ ] **Step 2: Run the end-to-end tests**

Run: `cargo test -p banager-core --test standalone_uninstall_test`
Expected: PASS — 6 tests, 1 ignored. (These are the first tests to drive `Session` over the real standalone adapter; if one fails, the defect is in Task 6's code, not here — fix it there, in a follow-up commit that names the failing test.)

- [ ] **Step 3: Check the smoke test's gate — and leave the real run to the author**

Run: `cargo test -p banager-core --test standalone_uninstall_test -- --ignored --nocapture`, without the variable.
Expected: PASS, printing `CANAGER_LIVE is not 1; skipping the real Trash smoke test` — the gate works.

Do **not** run it with `CANAGER_LIVE=1` here. It moves five items into the real Trash of the Mac it runs on, and spec §9.4 makes the development machine's run a manual one: it is the first step of the author's pre-merge check ("The pre-merge verification"), beside CI's run on every push. Neither is the Finder-launched, no-Full-Disk-Access case — a terminal here has Full Disk Access, and CI is a runner — so either run verifies the move itself (each kind of item, links as links, the dangling one included), not Put Back without FDA; the author's Finder check covers that.

- [ ] **Step 4: Say so in the trust file and the README**

In `docs/what-we-run.md`, at the end of `## Moving files to the Trash` (stage 6g's section, after its paragraph that begins `Not verified by that app:`), append:

```markdown

`crates/banager-core/tests/standalone_uninstall_test.rs` has an
`#[ignore]`d test that makes five throwaway items — a file, a folder, a
link to each, and a link to nothing — moves them with the real call, and
checks that each lands in `~/.Trash` as itself; CI runs it on every push.
It runs from a terminal or a CI runner, not from a Finder-launched app
without Full Disk Access, so it checks the move, not Put Back.
```

In `README.md`, replace the paragraph that begins `` `cargo test --workspace` has two `#[ignore]`d tests in `crates/banager-core/tests/brew_live.rs`, `` and ends `CI runs both; run them yourself with:`, and the code block after it, with:

````markdown
`cargo test --workspace` has three `#[ignore]`d tests, all skipped by a plain `cargo test`. Two are
in `crates/banager-core/tests/brew_live.rs`: one only reads the real Homebrew on the machine
running it, the other installs and removes the `hello` formula. The third, in
`crates/banager-core/tests/standalone_uninstall_test.rs`, moves five throwaway items it creates
(named `canager-trash-smoke-…`) into the real Trash of the Mac running it and leaves them there.
The two that change the machine refuse to touch anything without `CANAGER_LIVE=1`. CI runs all
three; run them yourself with:

```bash
CANAGER_LIVE=1 cargo test -p banager-core --test brew_live -- --ignored
CANAGER_LIVE=1 cargo test -p banager-core --test standalone_uninstall_test -- --ignored
```
````

and in the status block change `(plus 2 more that touch a real Homebrew and only run with \`--ignored\`)` to `(plus 3 more that touch a real Homebrew or the real Trash and only run with \`--ignored\`)`, and in the Chinese block `（另有 2 个要连着真实的 Homebrew 才跑，平时是跳过的）` to `（另有 3 个要连着真实的 Homebrew 或真实的废纸篓才跑，平时是跳过的）` (both wrap across lines in the file — the English one inside the `>` status block, so its words continue after a `> ` — match by their words). The test totals in those two sentences are Task 8's.

- [ ] **Step 5: Run the gates**

Run: `cargo fmt --all`, then all five from Global Constraints. Expected: all clean.

CI runs the new step on the first push after this commit (the branch's pushes wait for GitHub Actions' quota, which resets on 10/1). A failure there is reported to the author with the runner's error text — never silenced with `continue-on-error`.

- [ ] **Step 6: Commit**

```bash
git add crates/banager-core/tests/standalone_uninstall_test.rs .github/workflows/ci.yml docs/what-we-run.md README.md
git commit -m "$(cat <<'EOF'
Test a Claude Code uninstall end to end, and the real Trash call

Through Session, as the window drives it: a full uninstall moves exactly
the three previewed paths, both named claude, and leaves everything else
in the home as it was, while the plan the window receives carries the
paths alone; a launcher re-pointed after the preview -- at Homebrew's
copy, or at a new version by Claude Code's own updater -- stops the run
before anything moves; a refusal or a Cancel partway leaves the
launcher-only row, reported as Failed or Cancelled, and a second
uninstall finishes it; and a reading after the uninstall that cannot see
the launcher's folder is unconfirmed, not success. One ignored test, gated on CANAGER_LIVE=1 like
brew_live's install test, calls macOS's real move-to-Trash on a file, a
directory and three kinds of link -- including the dangling one every
uninstall moves last, which the spike did not try -- and CI runs it on
its throwaway runner. The trust file and the README say what it checks
and that it is not the Finder-launched case.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

---

### Task 8: The backlog, the README's Language sections, the test counts

Stage 6g put every sentence Task 6's behaviour made true or false into Task 6's own commit, and Task 7 described its own test (Ruling 23). What is left only lags — nothing in it contradicts the code in the meantime: the backlog's four entries, the README's Language sections (which list examples of what is translated), and the test counts, which only a finished suite can give.

**Files:**
- Modify: `docs/superpowers/backlog.md` — four entries under `## 阶段 4（独立安装工具）进行中的遗留（2026-09-25 立，分支 feat/phase-4-standalone）`
- Modify: `README.md` — the Language section (en and zh), the four test counts  [B's + F's file: anchor by quoted text]
- Test: the five gates.

**Interfaces:**
- Consumes: `removal::PUT_BACK_SETTLE` (Task 6, stage 6d), `recipe::SHARED_FOLDERS` (stage 6b), `removal::check_item`'s ancestry rule and `route::probe_strict` (stage 6c), which the backlog names; the refusal, fault and log-note copy of Tasks 4 and 5, which the Language section names; the finished test suites, for the counts.
- Produces: prose only.

- [ ] **Step 1: Add the backlog entries**

In `docs/superpowers/backlog.md`, at the end of the section `## 阶段 4（独立安装工具）进行中的遗留（2026-09-25 立，分支 feat/phase-4-standalone）` (after its last bullet, the one beginning `- **cask 的命令行链接只认第一个 \`app\`**`, and before `## 阶段 5（发现页）之前必须处理`), append:

```markdown

- **「放回原处」的记录只在一次卸载之内隔开**（2026-09-25，步骤 C）。无「完全磁盘访问」时，`trashItemAtURL:`
  间隔 ≤1.5 秒的连续调用只有第一项在 `~/.Trash/.DS_Store` 里留下 Finder 的「放回原处」记录，间隔 ≥2 秒时每项都有
  （spike：15/15 与 4/4，一台 Mac、macOS 27.0，机制不明；记录在调用返回之后才写）。`removal::execute_removal`
  在同一次卸载里每移一项之后停 `PUT_BACK_SETTLE` = 3 秒（最后一项之后也停，再报告完成），但这个停顿**只管一次操作之内**：
  操作管理器同时跑最多 3 个操作（`ops/mod.rs` 的 `Semaphore::new(3)`），两个 path-list 卸载并发时，两边的移动仍可能挤进
  2 秒之内，后一项就会丢掉记录——文件照样在废纸篓里，只是只能手动拖回；卸载还在运行时退出 Canager，刚移的那一项也可能丢掉记录。
  即使在一次卸载之内，3 秒也只是让每一项「多半」有记录（四次观察），不是保证；文案与信任文件都这样说（裁定 29）。
  **修法的形状**：把「上一次移到废纸篓的时刻」放进全进程共享的一处（`Session::new` 交给所有独立安装工具适配器的是同一个
  `Arc<RealTrasher>`），每次移动前补足到 3 秒，而不是只在 `execute_removal` 的循环里停；`MockTrasher` 与测试不受影响。
  **现在不做的理由**：C 只有 Claude Code 一个 path-list 卸载，两次卸载都要各自预览、确认；步骤 D 加入 grok 与 agy 之后再做。
  **同一处记两件没核实的事**：没有人真的点过「放回原处」（spike 只核对了 Finder 的记录；作者合并前在 Finder 启动的构建上
  手动核一次，结果写进 `docs/what-we-run.md` 的「Moving files to the Trash」）；macOS 27.0 以外的版本与 Intel Mac 没跑过
  （`tests/standalone_uninstall_test.rs` 的 `#[ignore]` 冒烟测试在作者的终端与 CI 上覆盖「移得进去」，悬空链接也在内，
  但那两处都不是无 FDA 的环境，不覆盖「放得回来」）。
- **检查 1 的「永不」清单挡住了 agy 的 `~/.cache/antigravity`**（2026-09-25，步骤 C）。步骤 C 照 spec §6.3 检查 1
  括号里的清单执行：路径的 canonical 父目录不得是 `~` 本身，也不得是 `~/.local`、`~/.config`、`~/.cache`、`~/Library`、
  `~/.cargo`（`recipe::SHARED_FOLDERS`；`removal::plan_removal` 按解析后的路径查，`recipes::tests` 按配方的写法查），
  拒绝理由是 `SharedFolder`。claude 与 grok 的清单都通过；spec 给 agy 列的 `~/.cache/antigravity`（父目录 `~/.cache`）过不了。
  **步骤 D 要做的决定**：给这一条路径一个有测试的明确例外（只对 optional 的 `Cache`），或者改 spec 的清单——不要悄悄放宽整条规则。
- **`~/.local/bin` 或 `~/.claude` 整个是链接时，卸载会被拒绝**（2026-09-25，步骤 C，裁定 24）。步骤 C 要求家目录到清单上
  每条路径之间的每一层都是真目录（`removal::check_item` 的祖先规则），所以用 dotfiles 工具把 `~/.local/bin` 整个链到别处
  （哪怕仍在家目录里）的用户——以及 `~/.claude` 是链接、里面又有 `downloads` 的用户——Claude Code 这一行照常显示
  （`route::probe` 先解析启动器所在的目录），但卸载在预览时就被拒绝，理由是 `not_what_instructions_expect`（文案说
  「它本身或它所在的某个文件夹可能链到了别处」）。spec §6.3 的检查 1 原本接受这种链接。
  **修法的形状**（真有人碰到再做）：只对启动器所在的那一层，允许它是一个指向家目录之内、又不在 `SHARED_FOLDERS` 里的链接，
  并把它解析后的目录与预览时记下的一起比对（`ItemIdentity` 已经随计划带着），配测试；不要整体放宽祖先规则——
  `~/.claude -> ~/Documents` 这类别名正是它挡住的。**现在不做的理由**：没有观察到这样的安装，放宽需要单独评审。
- **升级后的读取仍把「看不清」当成「不在了」**（2026-09-25，步骤 C 顺带发现）。B 的 `StandaloneAdapter::reconcile`
  （升级前后的读取）经 `inventory` 用 `route::probe`，权限错误、循环链接这类「看不清」一律成了 `Absent`：`claude update`
  退出 0 之后如果恰好读不了 `~/.local/bin`，`run_operation` 会报 `NeedsAttention(GoneAfterUpgrade)`——说升级后不见了，
  而事实是看不清。步骤 C 只把卸载之后的读取换成了 `route::probe_strict`（裁定 27）。**修法的形状**：`inventory` 改用
  `probe_strict`，把 `Err` 映成 `AdapterError`（刷新时这个来源计入「部分数据可能不是最新的」横幅，而不是这一行消失），
  升级前后的读取随之得到 `Err` → `Unconfirmed`；要连同 B 的「探测失败是『没装』，不是『没响应』」这条规则一起评审。
```

- [ ] **Step 2: The README's Language sections**

In `README.md`:

(a) Under `## Language`, in the paragraph beginning `Rust's refusals are translated too`:
- change `(a name it won't pass to a tool, a program that has gone missing)` to `(a name it won't pass to a tool, a program that has gone missing, a path on an uninstall list that is outside your home folder, in a folder other apps share, missing, not yours or not what the instructions describe)`
- change `(the program was removed between the check and the run, say)` to `(the program was removed between the check and the run, say, or a path changed between the preview and the click)`
- change `(waiting for Homebrew to finish updating, a stream it could no longer read)` to `(waiting for Homebrew to finish updating, a stream it could no longer read, each item it moved to the Trash)`

and in the bullet `- **Another program's own words.**`, change `or can't save Canager's settings for a cause Canager doesn't recognise.` to `can't save Canager's settings for a cause Canager doesn't recognise, or refuses to move an item to the Trash.`

(b) Under `## 中文`, in the paragraph beginning `Rust 侧返回的拒绝理由也会翻译`:
- change `（某个名字 Canager 不肯交给工具、某个程序不见了）` to `（某个名字 Canager 不肯交给工具、某个程序不见了、卸载清单上的某条路径不在你的个人文件夹里、放在其它应用共用的文件夹里、不存在、不属于你或者和说明写的不一样）`
- change `（比如程序在检查之后、运行之前被删掉了）` to `（比如程序在检查之后、运行之前被删掉了，或者某条路径在预览之后、点击之前变了）`
- change `（等待 Homebrew 更新完毕、某个输出流读不下去了）` to `（等待 Homebrew 更新完毕、某个输出流读不下去了、把哪一项移到了废纸篓）`

and in the bullet `- **其他程序自己的话。**`, change `或因为 Canager 不认识的原因无法保存设置时给出的原因。` to `或因为 Canager 不认识的原因无法保存设置、或拒绝把某一项移到废纸篓时给出的原因。`

- [ ] **Step 3: The counts**

Get the numbers from the suites, never by hand:

```bash
cargo test --workspace 2>&1 | grep -E '^test result' | awk '{ passed += $4 } END { print passed }'
pnpm test 2>&1 | grep -E '^\s*Tests\s'
```

Put the Rust total where the status block says `covered by <N> Rust tests` and the Chinese block says `有 <N> 个 Rust 测试`, and the front-end total where both say `<M> front-end tests` / `<M> 个前端测试` (B's Task 11 set the same four numbers; replace whatever they read now). Task 7 already changed "plus 2 more" to "plus 3 more" in the same two sentences. (Both sentences wrap across lines in the file; match by their words.)

- [ ] **Step 4: Run the gates**

Run: `cargo fmt --all`, then all five from Global Constraints. Expected: all clean.

- [ ] **Step 5: Commit**

```bash
git add docs/superpowers/backlog.md README.md
git commit -m "$(cat <<'EOF'
Record what the Trash pause does not cover, and bring the README's counts up

The backlog records that the pause Canager takes after each move to the
Trash holds within one operation only, with the shape of the fix and the
two things still unverified; that check 1's never-list refuses the cache
path spec lists for Antigravity, which step D must decide; that a
~/.local/bin kept as a link now refuses the uninstall; and that the
upgrade's reading still takes a probe it cannot finish for absence. The
README's Language sections name the new refusals, the changed-path
failure and the Trash log lines among what is translated, and its test
counts are the suites' own.

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
)"
```

- [ ] **Step 6: Delivery note (goes in the branch's PR description / handover; not a file)**

> **Step C: path-list uninstall (move to Trash).** Claude Code's row now offers Uninstall. The dialog lists in plain words the paths Canager will move to the Trash — `~/.local/share/claude`, `~/.claude/downloads` when it exists, and the launcher `~/.local/bin/claude` last — and the two it keeps, `~/.claude` and `~/.claude.json`, and says under "What Canager will do:" that no command runs and nothing is deleted. On confirmation Canager checks everything again and compares every path with what the preview recorded (device, inode, kind — kept with the issued plan, never sent to the window), then, for each path on the blocking pool, runs every check once more and calls macOS's `trashItemAtURL:` with nothing in between, waiting 3 seconds after each move so that Finder is likely to record Put Back for it (observed, not guaranteed; an item without it can be dragged back). The checks: inside the home folder, not directly in it or in a shared folder such as `~/.local`, reached only through real folders, yours, what Anthropic's instructions describe, and clear of the kept paths. A path that changed stops the run (`PathChanged`) — including a launcher Claude Code re-pointed by updating itself between the preview and the click; a refusal or a Cancel after the first move leaves the launcher-only row, which a second Uninstall finishes; the launcher must be one link into its root, and the outcome is verified by looking for it again — unconfirmed, never success, when Canager cannot tell. The trust file and the README say all of this in the same commit as the code.
>
> **Before merging (blocking, the author's own):** the check in this plan's section "The pre-merge verification": the `#[ignore]`d smoke test with `CANAGER_LIVE=1` in the author's own terminal — its dangling-link case is what every uninstall's last move depends on — then a Finder-launched debug build against a throwaway home made inside a fresh `mktemp -d` folder, after confirming `HOME` and the row's `0.0.1-canager-check` version; an uninstall; that the build has no Full Disk Access (not granted in System Settings, and its own debug line saying it was refused a listing of the Trash); and Put Back on the three items in Finder; then one sentence with the date and the results in `docs/what-we-run.md`'s "Moving files to the Trash". Never run it against the real `~/.local/bin/claude`.
>
> **What later steps change** — honest, not bugs: **D** adds Grok's and Antigravity's lists, check 5 (backup files), `Expect::File`, the `NotOurs` skip for an optional path that is not the tool's, `RemovedWhat::Backups` and the other `KeptWhat`s, widens the launcher-last test for Grok's `~/.grok/bin`, and decides agy's `~/.cache/antigravity` against check 1's never-list (backlog); **E** adds rustup's `Uninstall::Command`. The Put Back pause holds within one operation only; a `~/.local/bin` kept as a link refuses the uninstall; the upgrade's reading still reads "could not tell" as gone (backlog).
>
> **Rulings taken** (see "Rulings this plan makes"): the enums sliced to Claude Code's producers; `Recipe.uninstall: Option<Uninstall>`; `Detected.euid`; the launcher-only state re-probed from disk; check 1's never-list, with a refusal reason of its own; `KeepSpec` listed only when present; `~` in every sentence; log notes, not log lines; a refused item is `Failed`, a trasher that cannot ask is `CanagerFailed(Internal)`; `execute` rebuilds the list from disk; `RealTrasher` on every Unix, moving only on macOS; the pause is zero in tests, and follows the last move too; operation-aware verification through a trait method with a default; the `uninstall_unsafe` refusal without a frame; `PathChanged` copy true for a stop in the middle; the end-to-end test through `Session`; the pause per operation; `MockTrasher` test-only; the preview's label per arm; the documents in the commit that makes them true; the ancestry rule; kept paths stay kept (`OverlapsKept`); the last check immediately before the move; the one-hop launcher and `probe_strict`; the blocking pool and a between-items budget; Put Back observed, not promised; a debug-only Full Disk Access line.

---

## Self-review against the spec

**1. Spec coverage.** Every requirement spec §十 row C lists, and every C-relevant line of §6, §9, 附录 A and 附录 B, has a task:

| Spec requirement | Where |
|---|---|
| §6.2 `PlanAction { Command, TrashPaths }`, `Plan.action`, every `Plan {` construction (24, not 32), the four readers (`run_plan` refuses `TrashPaths`, `argv_preview` empty, `CommandPreview` branch, the `types.ts` mirror), the model and TS shape tests | Task 1 |
| §6.2 `uninstall.trashPreview` (the old `trashNote` merged into it, read off `plan.action`) | Task 1 |
| §6.2 step 3 / §6.3 the reading after an uninstall (`reconcile.present` from the launcher's route) — through B's deviation-15 hand-off, with "could not tell" an error, never "gone" (Ruling 27) | Task 2; Task 6, stage 6e |
| §6.5 `WillTrash`, `WillKeep`, `AlreadyGone`, `RemovedWhat`, `KeptWhat` (sliced to Claude Code), `warningKey`/`warningArgs`, copy; §6.6 the dialog's list | Task 3 |
| §6.3 `Fault::PathChanged`, `faultKey`/`faultArgs`, copy, `INTERPOLATED_SUBTREES["operations.outcome"]`; §6.2's log lines (as `LogNote`s) | Task 4 |
| §9.1 `AdapterError::UninstallUnsafe`, `UninstallUnsafeReason`, the `uninstall_unsafe` IPC kind with a `match`-spelled reason, `planFailureMessage`, `UninstallDialog.refusalText`, six sentences (the spec's four, plus `shared_folder` and `overlaps_kept`: Rulings 21, 25) | Task 5 |
| §6.2 `Trasher`, `RealTrasher` (`trashItemAtURL:`, links as links, the URL from the kind the last check saw), `MockTrasher` (call log, refuse-nth; test-only, Ruling 20), the `objc2`/`objc2-foundation` dependency, `Session::new` injecting `RealTrasher` | Task 6, stages 6a and 6e |
| §3.1/§6.1/§6.3 `Recipe.uninstall`, `Uninstall::Paths`, `RemoveSpec`, `KeepSpec`, `Expect`; Claude Code's list and its provenance (constant doc + fixture README); check 7, "launcher last" and check 1's never-list (`SHARED_FOLDERS`) as recipe tests | Task 6, stage 6b |
| §6.3 checks 1–4 in `plan()` (check 1 with the never-list, Ruling 5), with the ancestry rule and kept paths kept (Rulings 24, 25), `optional` skip, `LauncherOnly` → `AlreadyGone`, empty list / nothing detected → plain `Refused`; `(st_dev, st_ino)` and the kind, recorded per path (`ItemIdentity`); §3.3's launcher made one link into its root, and `probe_strict` (Ruling 27) | Task 6, stage 6c |
| §6.2 `execute()` and §6.3's comparison with what `plan()` saw: the checks again, the list compared, every identity compared with the preview's before anything moves (carried in `PlanAction::TrashPaths.previewed`, Ruling 10); every check and the identity again right before each move, on the blocking pool with nothing between the last check and the call (Rulings 26, 28); Cancel between items, a move under way finished first; the 120 s budget, stopping between items; one `trash` per path, `Failed` with the system's words, launcher last; the Put Back pause after each move (author decision 1), cut by the budget | Task 6, stages 6d and 6e |
| §6.2 `needs_password: false`, `timeout_secs: 120`, `KillThenReconcile`, `locks: [inst.id]`, `affected: []`; `inventory` stops producing `NoSafeMethod` for Claude Code; `NoSafeMethod` kept for a recipe without a method | Task 6, stage 6e |
| §9.2 `sourceNotice.launcherOnly.description` gets the spec's promises back (B withheld them until this step); doc comments | Task 6, stage 6f |
| §9.4 the temp-HOME end-to-end test (`MockTrasher`, "nothing else in the home changed") and the `#[ignore]` `RealTrasher` smoke on a file and a link, asserting `~/.Trash` and link-as-link; CI runs it | Task 7 |
| §9.5 the Claude Code section's uninstall, "files read", the never-list, the step-C verification results | Task 6, stage 6g; Task 7 (the smoke test's sentences); the author (the pre-merge results) |
| §6.2 the pre-merge verification (blocking) | "The pre-merge verification" above: the spike settles (1) except a Put Back click and a Tauri build, both closed by the author's check; (2) was not reproduced, and the spec's own sentence keeps the design; the author runs Task 7's smoke test — the two link kinds the spike did not try, the dangling one a merge blocker — and the Finder check, and writes the results |
| 附录 A rows for `LauncherOnly` (check 2), `TrashPaths`, `Trasher`, the three warnings, `PathChanged`, `UninstallUnsafe` | each task's Interfaces block names the producer and the readers landing in the same commit |
| 附录 B rows (no shell, nothing outside `$HOME`, nothing not the user's, nothing beyond the root, no permanent delete, no half-uninstall reported as success, no move after a change, no file-system write but the Trash) | Global Constraints; Task 6 (checks, order, `Trasher` has one method); Task 7 (`calls()` equals the plan's paths, the temp home otherwise unchanged); Task 6, stage 6g (the never-list in the trust file) |

Not in this step, by the spec's own slicing or this plan's rulings (each with its owner): check 5, `backup_globs`, `Expect::File`, `RemovedWhat::Backups`, `KeptWhat::{ToolState, ShellConfigLines, OutsideHome, NotOurs}` and the `NotOurs` skip (step D); `Uninstall::Command`, the rustup warnings, `operations.noCancelHint` (step E); a process-wide Put Back pacer (backlog, Task 8); agy's `~/.cache/antigravity` against check 1's never-list (step D; backlog, Task 8).

**2. Placeholder scan.** Searched every task for "TBD", "TODO", "implement later", "fill in", "similar to Task", "appropriate error handling", "handle edge cases": none. Every code step carries its code; every test its body. Two values are measured by the executor, each with the command that measures it: the recorded fixture directory's name (`ls adapters/fixtures/standalone-claude/`, Task 6 stage 6b) and the README's four test counts (Task 8 Step 3, B's commands). The smoke test's and the Finder check's results are the author's to record (the pre-merge check's third step); no task writes them in advance.

**3. Type consistency.** Checked across tasks against Core Interfaces: `PlanAction::{Command { program, args, env }, TrashPaths { paths }}` (Task 1), with `TrashPaths { paths, previewed }` from stage 6e on — the four Task 1 literals gain `previewed: Vec::new()` there, and every later match on the variant names both fields or `..` (Tasks 6e, 7); `ItemKind::{File, Dir, Symlink, Other}` (6a, 6c, 6d, 7) and `ItemIdentity { dev, ino, kind }` (6c, 6d, 6e); `Adapter::reconcile_after_uninstall(&self, inst, key)` (Tasks 2, 6e); `Warning::WillTrash { path, what: RemovedWhat }`, `WillKeep { path, what: KeptWhat }`, `AlreadyGone { path }` (Tasks 3, 6c, 7); `Fault::PathChanged { path }` and `LogNote::{MovedToTrash { path, trashed_to }, TrashFailed { path, error }}` — `trashed_to` on the wire, `{{trashedTo}}` in the copy (Tasks 4, 6d); `AdapterError::UninstallUnsafe { path, reason: UninstallUnsafeReason }` with the six snake_case spellings, `overlaps_kept` the sixth (Tasks 5, 6c); `Trasher::trash(&self, &Path, ItemKind) -> Result<PathBuf, TrashError>` in every implementation — `RealTrasher`, `MockTrasher`, and the test trashers of stage 6d (`ReplacingTrasher`, `SwappingTrasher`, `RelocatingTrasher`, `SlowTrasher`, `UnsupportedTrasher`) and Task 7 (`CancellingTrasher`, `LockingTrasher`); `MockTrasher::{new, bin, calls, kinds, refuse_call(nth, detail), cancel_after_call(nth, token)}` with `nth` 0-based everywhere it is used (Tasks 6a, 6d, 6e, 7: `refuse_call(1, …)` refuses `~/.claude/downloads`, the second call; a colliding name is suffixed with the call index, so Task 7's launcher lands as `claude 2`); `StandaloneAdapter::new(recipe, runner, http, trasher)` and `with_trash_gap(Duration)` (Tasks 6e, 7); `removal::{TIMEOUT_SECS, PUT_BACK_SETTLE}` (Tasks 6d, 6e, 6g); `removal::Job { recipe, detected, remove, keep }` owned (6c, 6d, 6e), `removal::Confirmed { paths, previewed }` and `Pacing { settle, budget }` (6d, 6e), `execute_removal`'s seven parameters (6d, 6e); `route::probe_strict` (6c, 6e) and `testing::Unreadable` (6c, 6e; Task 7 has its own `LockingTrasher`, since `testing` is invisible to `tests/`); `Detected { home, euid }` (Tasks 6c, 6e); `display_path` (Tasks 6c, 6d); `recipe::SHARED_FOLDERS` (Tasks 6b, 6c; named by the stage 6g trust file and the Task 8 backlog); `commandPreview.trashLabel` (Task 1); `MockTrasher` behind `#[cfg(any(test, feature = "test-support"))]` with the crate's dev-dependency on itself (Tasks 6a, 6e, 7). The front-end keys in Core Interfaces' list are the ones Tasks 1, 3, 4, 5 and 6f add, in both locale files.

**4. Review Focus.** The eight inputs are listed at the top with the tests that pin each, all inside tasks: the shared basename (Tasks 6e, 7), a change between the preview and the click — a check failing, the list changing, or a self-update re-pointing the launcher (Tasks 6d, 6e, 7) — a change during the pauses (Task 6d), a refusal or a Cancel partway and the second uninstall (Tasks 2, 6d, 6e, 7), a folder on the way that is a link (Task 6c), a kept path leading into a moved one (Tasks 6c, 6d), a reading after the uninstall that cannot see (Tasks 2, 6c, 6e, 7), and the real call on the two link kinds the spike skipped (Task 7, `#[ignore]`). Items 2, 3, 5, 6 and 7 answer the Astra review's findings 1–4 and 6 (its log, at the end). Ownership and check 1's never-list have their own tests (Task 6c). Checked and deliberately not added: a listed path that is a volume mounted inside the home folder (check 1 compares resolved paths, so it passes, and the system's call then moves the item to that volume's own Trash, as Finder does; a rare setup no test here builds), and two uninstalls whose moves interleave (the backlog entry Task 8 writes, with the fix's shape).

## Deviations from the spec, and facts found while writing this plan

1. **24 `Plan {` sites, not 32.** Counted at `3b5117a` (23) plus B's `Upgrade` arm; spec §6.2's "32" is §3.2's `HostEnv {` count carried over. Task 1 lists every site; the compiler is the completeness check.
2. **The spec's EPERM premise does not hold.** Spec §0.1 row 4 and §6.2 say an app without Full Disk Access gets `EPERM` renaming into `~/.Trash`; the spike's 24 runs all succeeded (TCC guards *listing* `~/.Trash`). The spec's fallback sentence applies ("若意外成功，也不回到 `mv`"); the design is unchanged and the trust file records the result.
3. **Put Back needs spacing, which the spec does not know.** Back-to-back `trashItemAtURL:` calls from a process without FDA record Put Back for the first item only. `PUT_BACK_SETTLE` = 3 s after each item, the last included, is author decision 1 (default taken; deviation 19 for the last); the pause holds within one operation (ruling 19, backlog).
4. **Check 1's never-list is applied to the resolved parent, and "之下至少两层" is not applied literally** (ruling 5): read literally, "at least two levels below home" would refuse `~/.claude/downloads`, which the spec's own list contains. The never-list refuses agy's `~/.cache/antigravity`, which spec §6.3 also lists — step D's decision (backlog). The refusal has its own reason, `SharedFolder` (deviation 21).
5. **Check 5, `Expect::File`, and the `NotOurs` skip move to step D** with their only producers (ruling 1); in this step an `optional` path of the wrong shape is refused. Spec §十 row C lists "五条检查"; four run here.
6. **Log lines are `LogNote::{MovedToTrash, TrashFailed}`, not `OperationEvent::Log { line: "Moved … " }`** (ruling 8): `events.rs` sends Canager's own words as a key plus arguments, and all copy is en + zh-CN.
7. **The reading after an uninstall is `Adapter::reconcile_after_uninstall`, not `reconcile`** — B's deviation 15 made `reconcile` refuse a launcher-only launcher (right for upgrades), so the operation-aware reading B handed to this step is a trait method with a default body (ruling 15). Where B and the spec differ, this plan follows B.
8. **Spec §6.2 step 3 says a cancelled run is reported "`Cancelled` or `StillInstalledAfterUninstall`".** The code at `3b5117a` (`run_operation`'s `Ok(Outcome::Unconfirmed)` arm) gives `Succeeded` when the launcher is gone, `Cancelled` when it is there and the user pressed Cancel, and `Unconfirmed` otherwise (the budget ran out) — never `StillInstalledAfterUninstall`, which is the exit-0 arm's. The plan follows the code; Task 7 pins `Cancelled`.
9. **`execute()` rebuilds the list from the disk, compares it with the plan's, and compares every identity with what the preview saw** (ruling 10). Spec §6.3's comparison with what `plan()` saw — "与 `plan()` 所见的差异（链接被重指…被替换成同名的另一个 inode）" — is implemented; the spec names no carrier for the identities, and this plan carries them in `PlanAction::TrashPaths.previewed`, a field serde skips, so neither the wire nor the TypeScript mirror changes and `Adapter::execute` keeps its signature. The identity is `st_dev`, `st_ino` and the kind (the controller's words), not the pair spec §6.2 names. And the plan goes past spec §6.2 step 3, which compares only the identity before each move: each item's turn runs every check again (rulings 24–26). A same-shape change between the preview and the click that every check still passes — Claude Code's updater re-pointing its launcher — is therefore refused, and the user previews again. (The revision before the Astra review dropped this comparison; it is restored.)
10. **`AlreadyGone` is decided by re-probing the disk, not by the instance's `LauncherOnly` note** (ruling 4); check 4's `SymlinkIntoRoot` is `route::probe`'s answer, B's implementation of §3.3 including B's deviation 17.
11. **`RealTrasher` compiles on every Unix and moves only on macOS** (`TrashError::Unsupported` elsewhere), where the spec cfg-gates the type; and it builds the URL with `fileURLWithPath:isDirectory:` from the kind the caller's last check saw rather than the spike's `fileURLWithPath:` (ruling 11) — so `Trasher::trash` takes that kind, where spec §6.2's trait takes the path alone (ruling 26).
12. **The `#[ignore]` smoke test does more than the spec asks and is gated like `brew_live`**: five item kinds instead of a file and a link (the dangling link and the link to a directory are the two the spike did not try), and `CANAGER_LIVE=1`, this repository's rule for a test that changes the machine. CI sets the variable. Its run on a development machine is the author's (deviation 24).
13. **The end-to-end test drives `Session`** (refresh → the gate → submit → `run_operation`) instead of calling `plan`/`execute`/`reconcile` directly (ruling 18), so it also proves the gate lets the uninstall through and the launcher-only row comes back.
14. **The `uninstall_unsafe` payload's shape test lives in `sources.test.ts`**, where the payload is decoded (`parseUninstallUnsafe`, beside `parseErrorPayload`), and in `src-tauri`'s test of `plan_operation_error`, rather than in `types.test.ts`: it is an IPC error payload, not a type `types.ts` mirrors.
15. **`sourceNotice.launcherOnly.description` restores the spec's three promises with B's correction kept** (ruling 13): "this link can't run", not "typing claude fails", since another installation may still run (B's finding 9).
16. **`operations.outcome.CanagerFailed.PathChanged`** does not say "so Canager didn't move anything" (spec §9.2): that is false for a stop after the first item. It names the path, says Canager stopped without moving it, and points at the log (ruling 17).
17. **Recorded but not decided here:** the Put Back click, a Tauri build and that build's lack of Full Disk Access are unverified until the author's check; macOS 13–26 and Intel are unobserved; the pause is a four-run measurement that makes Put Back likely, not certain. The trust file and the backlog say so in those words.
18. **`MockTrasher` is compiled only for tests** (ruling 20). Spec §6.2 describes it beside `RealTrasher` with no gate; it deletes its bin when dropped, so it sits behind `test-support`, which banager-core's own integration tests turn on through a dev-dependency on the crate itself.
19. **The Put Back pause also follows the last move** (author decision 1, extended). Spec §6.2 step 4 returns `Succeeded` right after the last move; this plan waits `PUT_BACK_SETTLE` first, because every run that recorded all items stayed alive 3 s after its last call and the record is written after the call returns. Claude Code's uninstall spends 9 s in pauses, not 6.
20. **`TrashError::Unsupported` is `CanagerFailed(Internal)`, not `Failed`** (ruling 9). Spec §6.2 step 3 sends every trash failure to `Failed { summary }`, whose summary the front end quotes as another program's words; "Canager could not ask the system" is Canager's own.
21. **Six refusal reasons, and two sentences reworded** (rulings 21, 25). Spec §9.1/§9.2 have four reasons, and check 1's never-list refusal would have used `outsideHome`, whose sentence ("it's outside your home folder") is false for `~/.local/bin`. `outsideHome` now says the folder leads outside the home folder, and `notWhatInstructionsExpect` says Canager couldn't confirm the path is what the instructions describe — the path, or a folder it is in, may be a link to somewhere else — since it is also the answer for a path that could not be examined and for the ancestry rule. The sixth, `overlapsKept`, names a kept path that a listed one would disturb (ruling 25).
22. **`commandPreview.trashLabel`** (ruling 22). Spec §6.6's dialog keeps "将执行：" above the `TrashPaths` sentence; that stays the zh label, and English says "What Canager will do:" rather than "This will run:" above "no command runs".
23. **The documents land with the code** (ruling 23). Spec §十 row C puts the trust file in step C without naming a commit; this plan puts every sentence Task 6 makes true or false into Task 6's commit and the smoke test's into Task 7's, and leaves Task 8 only what lags.
24. **The development machine's smoke run is the author's** — spec §9.4's "开发机手动跑一次" — as the first step of the pre-merge check, not an executor's step: it changes that Mac's Trash, and a terminal here has Full Disk Access, so it would not observe the no-FDA context anyway.
25. **The ancestry rule is stricter than spec §6.3's check 1** (ruling 24). Check 1 accepts any folder whose resolved path is inside the home folder; this plan also refuses one reached through a link inside the home folder (`~/.claude -> ~/Documents`, a `~/.local/bin` kept in a dotfiles folder), because such a link is exactly how an unrelated folder passes as the tool's or how settings alias the program folder (Astra findings 1, 3). A dotfiles-linked `~/.local/bin` therefore refuses the uninstall, and so does a linked `~/.claude` with the download cache inside (backlog).
26. **Kept paths are checked, not only listed** (ruling 25). Spec §6.3 lists `~/.claude` and `~/.claude.json` as kept and trusts the recipe test that no kept path is inside a removed one; this plan also checks, with every link resolved, at the preview and before each move, that no moved path is, holds or lies (other than as spelled) inside a kept one — a seventh check the spec does not have, with the sixth refusal reason.
27. **B's detection is narrowed to a launcher one link from its root** (ruling 27): B's `probe` accepted any chain that resolved into the root; a chain through a link outside it is now no instance (the Unknown page lists it), and a launcher dangling through a link the tool keeps inside its root is now launcher-only where B said absent. `probe` itself is B's contract ("not installed, never not responding") over the new `probe_strict`.
28. **Each move runs on tokio's blocking pool and is awaited through a Cancel** (ruling 28); spec §6.2 says only that the token is watched between items. The budget is described as a stop between items, not "120 s in all".
29. **The preview's sentence and `PathChanged`'s differ from the spec's** (ruling 29, ruling 10). Spec §6.2/§6.6's `uninstall.trashPreview` ("清空废纸篓之前都能放回来") reads as a promise of Finder's Put Back, which the spike showed is likely at best; both locales now say the items can be dragged back until the Trash is emptied and that Put Back will likely work too. `PathChanged`'s copy adds that a tool that updates itself can cause it.
30. **A debug build prints whether it may list the Trash** (ruling 30), for the author's pre-merge check; the spec's verification (§6.2) asks for a no-FDA build but gives no way to establish that the build is one. Release builds do not contain it.

## Review log

Adversarial review of this plan, 23 points, each checked against the worktree (`~/dev/Canager-phase4`, read only; the rest of step B landed during the check — `71eacd0`, `f61cd94`, `dcf0e7c` — and every anchor into it was re-checked against the landed files), the spec, `spike-trash-tcc.md`, `b-task3-progress.md`, and — where a claim was about the toolchain — scratch crates built outside the worktree with the repository's toolchain (rustc/cargo/clippy 1.98.1). 22 accepted, 1 rejected.

| # | Point | Verdict | Reason and change |
|---|---|---|---|
| 1 | Task 1 step 3g misses the retarget test's `args` override in `UninstallDialog.test.tsx` | **Accepted** | Confirmed at `UninstallDialog.test.tsx:641-652`: the reply spreads `issuedPlanFor()` and overrides the top-level `args`; after `PlanAction` that key is ignored and `findByText("… --formula yq")` (`:688`, `:697`) times out. 3g now converts that override to `action: { Command: { … planned.name … } }`, and Task 1's Files line names the test. |
| 2 | clippy `needless_borrows_for_generic_args` fails the gate on six `&owned` arguments | **Rejected** | False on this toolchain. A scratch crate with each flagged line copied exactly (`symlink(&real, outside_bin.join("claude"))`, `symlink(&elsewhere, &layout.launcher)`, `symlink_metadata(&link)`, each referent used once) is clean under `cargo clippy --all-targets -- -D warnings` on clippy 0.1.98, while the lint is demonstrably active (it flags `Command::args(&["-a", "-l"])` in the same crate). Clippy 0.1.98 does not fire it for an owned `PathBuf`/`String` local. The by-value form was adopted at the six sites anyway, at no cost, because CI's `dtolnay/rust-toolchain@stable` could pick up a clippy that does. |
| 3 | `MockTrasher` bins and three `TempHome` tags can collide in parallel tests | **Accepted** | Confirmed: bin name `canager-mock-trash-{pid}-{nanos}` with a 1 µs clock (B's deviation 4 measured 6/30 failures for the same shape), and `execute-refused`/`-cancel`/`-budget` used by both `removal.rs` and `mod.rs`. `MockTrasher::new` now adds a process-wide `AtomicUsize` sequence number; every `removal.rs` tag is `removal-exec-*`. A scan of the plan's and B's landed tags finds no duplicate. |
| 4 | Task 8's `grep "no uninstall…"` "prints nothing" is false | **Accepted** | The new text says "Claude Code has no uninstall command", and B's phrases are hard-wrapped, so the old grep matched the wrong line and missed the right ones. Now (stage 6g Step 4) the file is folded first and the check greps only B-era phrases: `in this step, no uninstall`, `cannot be uninstalled here`, `cannot remove the link either`, `offers no button` (all four verified present in the landed file today). |
| 5 | Trust file: "a stop partway … leaves the launcher-only row" is false for a refusal of the first item | **Accepted** | `execute_removal` returns at the first refused item; the program directory is first. The sentence (now in stage 6g) says a stop before the first move changes nothing and the row stays as it was, and only once the program files are in the Trash does the launcher-only row appear. |
| 6 | Two anchors quote B's plan, not the landed code | **Accepted** | Confirmed in `71eacd0`: `reconcile`'s doc begins `/// Step B only executes upgrades` and is six lines; the launcher-only assertion is rustfmt'd over four lines. Both are now anchored by symbol with the landed text quoted. Checking every other anchor into B's landed files found three more differences, fixed the same way: `Probe::LauncherOnly`'s doc (landed wording includes "or by another tool"), and the multi-line `StandaloneAdapter::new(` calls in `test_the_adapter_trait_delegates_to_the_inherent_methods` and `claude_upgrade_outputs`. The Baseline records `71eacd0`. |
| 7 | The replacement install test restores a comment B already corrected | **Accepted** | Confirmed (B deviation 1; landed `mod.rs:1545-1550`): the gate has no install rule. The test now carries B's corrected sentence. |
| 8 | Check 1 weakened: the never-list is gone, so `~/.local/bin` could be moved whole | **Accepted, with a different reason** | Confirmed: `parent == canonical_home` alone lets a typo'd `~/.local/bin` (Dir, Launcher) pass check 1, check 4 and both recipe tests; Ruling 5's "only reading" claim was wrong. Now `plan_removal` refuses a canonical parent that is home or one of `recipe::SHARED_FOLDERS` (`.local`, `.config`, `.cache`, `Library`, `.cargo`), each compared as spelled and as resolved; `recipes::tests` holds every `remove` path to the same rule as spelled; the launcher-last test is `last.path == route.launcher` (D widens it for `~/.grok/bin`); a refusal test covers `~/claude-thing`, `~/.local/bin` and a dotfiles-linked `~/.config/fish`. The refusal is a new fifth reason, `SharedFolder` (Task 5: Rust variant, IPC spelling `shared_folder`, TS union, keys and copy in both locales, tests), not `outside_home`, whose sentence would be false for it (point 14). agy's `~/.cache/antigravity` conflicts and is left to D, recorded in the backlog. Ruling 5 and deviation 4 rewritten; Rulings 21, deviation 21 added. |
| 9 | Never-list says Canager never touches `~/.claude`, yet it moves `~/.claude/downloads` | **Accepted** | Reworded as proposed. The same overclaim was also in `CLAUDE`'s doc comment ("which Canager never touches") and the fixture README ("keeps both"); both now say that of `~/.claude` only `downloads` is moved. |
| 10 | The author's check never confirms the throwaway home is in effect | **Accepted** | The fake install now prints `0.0.1-canager-check`; the check reads `HOME` from the launched process (`ps eww … | grep '^HOME='`) and requires the row to show that version before anything is pressed, quitting otherwise. It also has the author read the log's "now at …" lines, since where `trashItemAtURL:` puts items under an overridden `HOME` was never observed. |
| 11 | Every uninstall moves a dangling link last; the spike never tried one | **Accepted** | Confirmed: the launcher is an absolute link into `~/.local/share/claude`, which moves first; the spike moved its link while the target existed. The table row (1) now says "satisfied for a link whose target exists", (c) says every uninstall depends on a dangling link and makes the smoke test's dangling case a merge blocker, and the author's check names the launcher as a dangling link. |
| 12 | No pause after the last move; Put Back registration is asynchronous | **Accepted, both fixes** | Confirmed (spike §5 reading 4; every no-FDA run that recorded all items lingered 3 s). `execute_removal` now waits `PUT_BACK_SETTLE` after the last move too (a Cancel there only cuts the wait; everything is moved), through a `settle` helper; the pacing test expects three pauses; the trust file adds that quitting Canager mid-uninstall may lose the last item's record. Claude Code's pauses are 9 s of 120. |
| 13 | Canager's own English reaches `Failed.summary` and `TrashFailed` as "Your Mac gave this reason" | **Accepted** | `TrashError::Unsupported` (off macOS) and the non-UTF-8 path (now also `Unsupported`) become `CanagerFailed(Fault::Internal)` with no `TrashFailed` note; only `Refused` (NSError text) goes to `Failed`/`TrashFailed`. A new test pins it. `Internal`'s copy ("didn't start and nothing was changed") is true: `RealTrasher` answers it for every path of one uninstall alike, so at the first item. |
| 14 | Two refusal sentences can be false for the path they name | **Accepted, fix partly different** | `outsideHome` now reads "the folder it is in leads outside your home folder" and is no longer returned for a parent that is home (that is `SharedFolder`'s, point 8 — the proposed rewording alone would still be false there). `notWhatInstructionsExpect` now says Canager "couldn't confirm" the path is what the instructions describe and gives the causes as "may be", true for an unreadable path too; no sixth reason. Both locales and the Task 5 dialog test updated. |
| 15 | `launcherOnly.description` says the files *are* in the Trash | **Accepted** | Restored "may be" / "可能" as spec §9.2 has it. The old regex already matched either wording, so it was tightened rather than loosened: it now pins `may be in the Trash|可能在废纸篓里`. |
| 16 | Two README sentences overstate | **Accepted** | "moves exactly those" → "moves those paths, plus its installer's download cache"; "the only change … itself" → "… besides saving its own settings". The second bullet's heading "Nothing is moved that isn't what you were shown" also overstated after point 21 (a same-shape replacement before the click is moved): it is now "Only the paths you were shown are moved", which the list comparison guarantees, and it names the shared-folder rule. |
| 17 | Same as 5, and Review Focus 3 generalises it | **Accepted** | Same fix as 5; Review Focus 3 now reads "an item after the first" and says a refusal of the first item leaves the ordinary row. |
| 18 | "This will run:" above "no command runs" | **Accepted** | New key `commandPreview.trashLabel`: en "What Canager will do:", zh "将执行：" (the label spec §6.6's dialog shows, which does not say a command runs). `CommandPreview` picks the label per arm; its test asserts the new label and that "This will run:" is absent (Ruling 22). |
| 19 | `MockTrasher` (whose drop deletes) is compiled into release builds | **Accepted** | `pub mod mock` and `pub use mock::MockTrasher` are now `#[cfg(any(test, feature = "test-support"))]` — the crate's existing feature for test code that touches real state — and banager-core gains a dev-dependency on itself with that feature so `tests/` still see it. Verified in a scratch workspace of the same shape on cargo 1.98.1: `cargo test --workspace` sees the type in unit and integration tests, `cargo clippy --workspace --all-targets -- -D warnings` is clean, release builds lack it. The claims at Global Constraints and in `trash/mod.rs` corrected (Ruling 20). |
| 20 | Task 6 turns uninstall on while the docs say "no uninstall" until Task 8 | **Accepted, broadened** | Not only the "no uninstall" sentences: after Task 6 the trust file's "Files Canager writes" and never-list ("never … moves a file") and the README's "shows its real argv" were false too. All of Task 8's trust-file edits and the README row and safety bullets moved into Task 6 as stage 6g (with the two `what_we_run_test` tests; the commit is 6h); the smoke test's sentences, the ignored-tests paragraph and "plus 3 more" moved to Task 7; Task 8 keeps the backlog, the Language sections and the counts, which only lag (Ruling 23). Task list, File Structure, self-review and delivery note follow. |
| 21 | `Identity`'s doc is false; the plan-to-execute identity comparison is silently dropped | **Accepted** | A re-pointed link is a new link, a new inode; the doc now says so. Deviation 9 states that spec §6.3's comparison with what `plan()` saw is not implemented between preview and click, why (the identities would have to cross IPC in `TrashPaths` or live in the adapter between calls), and where the comparison is made (fresh look → each move). `Fault::PathChanged`'s doc now describes exactly the three findings the code makes. |
| 22 | No test for Ruling 1's "optional path of the wrong shape is refused" | **Accepted** | `test_plan_removal_refuses_an_optional_path_of_the_wrong_shape`: `~/.claude/downloads` as a link and as a file → `NotWhatInstructionsExpect`. Ruling 1 names it. |
| 23 | The executor runs the real-Trash smoke test on the author's Mac, in an FDA terminal | **Accepted** | The development machine's run is now the first step of the author's pre-merge check (spec §9.4 "开发机手动跑一次"); Task 7 Step 3 runs only the skip check and says why. The plan says in (c), Task 7 and the trust file that the terminal and CI runs verify the move, not Put Back without FDA (deviation 24). |

**Also fixed while verifying** (not raised by the review): stage 6e (i) told the executor to add `PlanAction` to `standalone/mod.rs`'s imports a second time after Task 1 had added it — a duplicate import that does not compile; the Detect sentence of the trust file now includes "or by an uninstall that stopped partway"; the Global "Honest outcomes" line names `CanagerFailed(Internal)` and describes `PathChanged` as the code does; the trust file's "Files Canager reads" names the shared folders it resolves; stale "four" counts (Ruling 16, the task list, Task 5 Step 4) and "between items" pause wording (Tech Stack, docs of `trash_gap`, `with_trash_gap`, test helpers, deviation 3) updated; Task 7's commit subject no longer says the real Trash is called "once"; Task 6's heading, Files list (five items appended to `recipe.rs`, the docs, the `Cargo.toml` changes) and test counts (11 in 6c, 21 after 6d) follow the changes.

**Remaining risks after this review:**
- The author's check rests on two things nobody has observed: that `open -n --env HOME=…` sets `HOME` for a LaunchServices-launched Canager, and that `trashItemAtURL:` still uses the real `~/.Trash` under an overridden `HOME`. The check now detects both and stops rather than proceeding, but if either fails it cannot close (a)/(b) as written and needs another harness (a throwaway macOS user, say).
- The pause after the last move is inferred from the spike's 3 s linger, not measured with an immediate exit without FDA; the whole pause is still four runs on one Mac.
- A dangling link moved without FDA is covered only by the author's Finder check; the smoke test (author's terminal, CI) covers it with FDA or on a runner.
- The crate's dev-dependency on itself was verified in a scratch workspace of the same shape, not in the Canager workspace (cargo must not run there during review); the first executor run of stage 6a is the real check.
- Clippy's behaviour can change under CI's floating `stable`; the flagged sites are by value now, but new code in future steps is not.
- Agy's `~/.cache/antigravity` contradicts check 1's never-list; step D must decide it explicitly (backlog).
- Deliberate deviations the author may overrule: a fifth refusal reason (`SharedFolder`) and reworded `outsideHome`/`notWhatInstructionsExpect` copy; `MockTrasher` behind a feature with a self dev-dependency; the 3 s pause after the last move; `commandPreview.trashLabel`. (The preview-to-click identity comparison this revision dropped is restored by the Astra review below; its own risks are listed there.)
- Task 6 is now a single commit of eight stages including the documents; a failure late in it (6g, 6h) means re-running earlier stages' checks before committing.
- The anchors were checked against the worktree at `dcf0e7c`; anything that lands there before this step runs (a follow-up to B, step E's edits to shared files) can move them. The ones by symbol survive that; the quoted ones need a look.

## Review log (Astra, 2026-09-25)

Independent review by GPT-6 Astra (`.superpowers/phase4/astra/review-plan-c.md`, baseline `dcf0e7c`): nine findings and two categories checked clean. The controller ruled on each finding; this revision applies the rulings, and where a ruling left a choice open the choice and its reason are in the row. The worktree was read, never written; the Rust the rulings changed was checked in scratch crates outside it (see "How this revision was checked" below).

| # | Finding (Astra) | Ruling | Applied, and how |
|---|---|---|---|
| 1 | **breaks** — runtime aliases defeat ownership and preservation: `~/.claude → ~/.local/share/claude` moves the settings the preview keeps; `~/.claude → ~/Documents` passes an unrelated `downloads` as the cache (Task 6, 6b/6c) | Ancestry rule; kept paths checked at run time after canonicalization; the counterexamples as tests | **Applied** (Rulings 24, 25). `removal::check_item` requires the resolved folder of every listed path to equal the resolved home joined with the recipe's spelling (every folder between them real), and the item itself a link only for `Expect::SymlinkIntoRoot`; `disturbed`/`kept_places` refuse a listed path that is, holds, or (other than as spelled) lies inside a kept path or what one leads to — at the preview, at the confirmation and in every item's turn. Tests: `test_plan_removal_refuses_a_path_reached_through_a_linked_folder_inside_home` (cache parent aliased to `~/Documents`; a dotfiles-linked `~/.local/bin`), `test_plan_removal_refuses_when_a_kept_path_leads_into_what_it_would_move` (settings folder aliased to the program folder; `~/.claude.json` into it), `test_execute_removal_refuses_a_settings_folder_linked_to_the_program_folder_after_the_preview`. **Decided here:** the ruling's "and vice versa", read literally, refuses Claude Code's own list — its cache lies by design inside the kept `~/.claude` — so the one nesting the recipe spells, inside a real folder, is allowed and every other is refused (Ruling 25); and since none of the five sentences is true of a kept path leading into a moved one, the refusal is a sixth reason, `OverlapsKept`, naming the kept path (Task 5, both locales). |
| 2 | **breaks** — the confirmation discards the preview's identities, so a retargeted home or a replaced cache is moved uninspected (Task 6, 6e Step 3(v)) | Carry the identities (`st_dev`, `st_ino`, kind) with the plan, off the wire; compare in `execute()`; remove the deviation | **Applied** (Ruling 10). `PlanAction::TrashPaths { paths, #[serde(skip)] previewed: Vec<ItemIdentity> }`, filled by `StandaloneAdapter::plan` from `plan_removal`; `execute_removal` compares all of them with a fresh look before anything moves, and each again in its item's turn. **Decided here:** the serde-skipped field, not a table beside `StoredPlan` — a scratch crate on this lockfile's serde 1.0.229 showed the externally tagged variant serialises without the field, deserialises it empty, ignores it in a payload and keeps it through `Clone`, so it rides from `issue_plan` through `StoredPlan` and `submit` to `execute` with `Adapter::execute` unchanged, while a table would need a new path into `execute` (`submit` consumes the `StoredPlan`). The TS mirror is untouched; a plan read back from JSON has no identities and `execute_removal` refuses it as Canager's own bug. The canonical home needs no separate carrier: a retargeted home yields other inodes. Deviation 9 rewritten; the trust file and `PathChanged`'s copy say a self-update between the preview and the click stops the run. Tests at three levels: `test_execute_removal_refuses_what_the_preview_did_not_see`, `test_execute_refuses_a_launcher_the_updater_re_pointed_after_the_preview`, `test_a_self_update_between_the_preview_and_the_click_stops_the_uninstall_before_it_moves_anything` (through `Session`, so it proves the field survives `StoredPlan`); the wire test in `model.rs`. |
| 3 | **breaks** — later items re-check only the inode, so an ancestor renamed out of home and replaced by a link carries the move outside it (Task 6, 6d) | Run the full per-item checks immediately before each trash call, after any pause | **Applied** (Rulings 24, 26). Each item's turn (`take_turn`) takes a fresh look — home resolved, kept paths placed — and runs check 1, the ancestry rule, the kept-path rule, the launcher's probe, ownership, the kind and the identity before its move. Test: `test_execute_removal_checks_an_items_folders_again_after_the_pause` renames `~/.claude` out of the home folder and leaves a link in its place during the first pause, asserts the cache kept its identity, and that the run stops at it with nothing moved there. |
| 4 | **breaks** — the final check and the move are separate pathname operations; `RealTrasher`'s extra `lstat` adds a race (6a/6d) | Not a blocker (accidental-change threat model); narrow the window; drop the extra `lstat`; state the residual gap; a deterministic substitution test | **Applied as ruled** (Ruling 26). The checks and the move run in one closure on the blocking pool; the last check is the item's own `lstat`, whose kind is handed to `Trasher::trash(path, kind)`, and `RealTrasher` no longer looks at the path (building the URL reads nothing). The controller's sentence is in the trust file and in `take_turn`'s doc. Tests with `MockTrasher`: a same-name substitution before an item's turn is caught (`test_execute_removal_catches_a_substitution_before_an_items_check`); one made inside the move itself is what gets moved, pinned as the documented edge (`test_a_substitution_inside_the_move_itself_is_beyond_the_last_check`). The review's object-bound mechanism was not pursued: `trashItemAtURL:` takes a URL, and whether a file-reference URL would bind it to the checked object is untried (remaining risks). |
| 5 | **breaks** — the author's throwaway home at a fixed `/tmp` path can overwrite real settings through a pre-existing link (pre-merge Step 2) | `mktemp -d`; build only inside it; refuse if the app's `HOME` differs | **Applied.** Step 2 runs in a subshell with `set -euC`, makes `D=$(mktemp -d)` and the home inside it only, never reuses a path, and prints `STOP` and exits unless `ps eww` shows exactly that `HOME` and the app's parent is `launchd` (the spike's own proof of a Finder-style launch). No `#` comments inside the block: an interactive zsh, the author's shell, would run them as words. |
| 6 | **wrong** — a partial uninstall can read as `Succeeded` (a launcher chained through `~/.local/bin/claude-current`; a permission error collapsing into absence), with no row to retry from (Task 2, 6e) | (a) Single-hop launcher in detection, coordinated with B's `route.rs`, with a test; (b) "could not tell" is an error, so `Unconfirmed`; tests for cancel-then-retry and a permission error in the post-uninstall probe | **Applied** (Ruling 27). (a) `route.rs`: B's `probe` becomes `probe_strict` (errors kept) plus a one-line `probe` over it; `one_hop` reads the launcher's text from its resolved folder and must land inside the resolved root, in the present and the dangling arm alike. B's two-hop test still passes (its first hop is the root's own `current`), and a launcher dangling through such an inside link is now launcher-only where B said absent — deviation 27. Tests: `test_probe_refuses_a_launcher_that_reaches_the_root_through_a_link_outside_it`, `test_probe_counts_a_launcher_dangling_through_the_roots_own_link_as_launcher_only`, `test_detect_lists_nothing_for_a_launcher_that_reaches_the_root_through_another_link`. (b) `reconcile_after_uninstall` reads presence with `probe_strict` and no version; an `Err` reaches `run_operation`, which already makes it `Unconfirmed`. Tests: Task 2's `test_an_uninstall_whose_reading_cannot_tell_is_unconfirmed_never_succeeded`, `test_probe_strict_says_it_cannot_tell_where_probe_says_absent`, `test_reconcile_after_uninstall_tells_there_gone_and_cannot_tell_apart`, and end to end `test_an_uninstall_whose_last_reading_cannot_tell_is_unconfirmed_not_succeeded` (a `chmod 000` on `~/.local/bin` after the last move; skipped as root) and `test_an_uninstall_cancelled_between_items_is_reported_cancelled_and_a_second_uninstall_finishes` (the retry). **Decided here:** no per-item completion record — with the launcher last and presence read strictly, "gone" after a Canager run means every earlier item moved, unless something outside Canager removed the launcher meanwhile; and B's upgrade reading, which still collapses a probe error into absence (`GoneAfterUpgrade`), is recorded in the backlog, not changed in this step. |
| 7 | **wrong** — "120 s in all, pauses included" is not enforced: settling ignores the budget and a blocking Foundation call can hold a Tokio worker (6d/6g) | `spawn_blocking` per trash call; bound every pause by the remaining budget; check the budget before each item; await a move under way on Cancel; describe it as a stop between items | **Applied** (Ruling 28). Each turn is `spawn_blocking`'d and awaited to its end whatever the token says; each pause is `min(PUT_BACK_SETTLE, budget left)`; the budget is checked before each item; a panicked turn is `Unconfirmed`. Tests: `test_execute_removal_cuts_a_pause_to_what_is_left_of_the_budget`, `test_execute_removal_finishes_a_move_under_way_when_cancel_arrives`. The trust file's row and prose say "Canager stops between items once it is spent". |
| 8 | **gap** — the real Canager build's lack of Full Disk Access is never established (pre-merge Steps 2–3) | The author confirms Canager is not listed / off in System Settings, and the app records what it observes (debug-only), kept out of release builds | **Applied** (Ruling 30). New Step 3: the System Settings check, and `RealTrasher`'s `#[cfg(all(target_os = "macos", debug_assertions))]` line — after each move it tries `read_dir` on the Trash it used and prints the answer — read from the log `open --stderr` captured; all three must say `Operation not permitted`. The record (Step 5) names both. A clean macOS account, the review's preference, is the fallback if `--stderr` shows nothing (remaining risks). |
| 9 | **minor** — unconditional claims survived: Put Back "left"/"ensured"; `mv` "cannot reach the TCC-protected Trash" (6a, delivery note, Task 1) | Put Back as observed and likely, not guaranteed, dragging back always possible; remove the disproved TCC claim; both locales | **Applied** (Ruling 29). Reworded: the `Cargo.toml` comment, `trash/mod.rs` and `real.rs` docs, `PUT_BACK_SETTLE`'s doc, `pause`'s doc, the trust file's Put Back and `rename` bullets, the README bullet, the delivery note; `uninstall.trashPreview` in en and zh-CN ("…until you empty the Trash you can drag them back out, and Finder's Put Back will likely work too" / "……清空废纸篓之前都能把它们拖回来，访达的「放回原处」多半也能用。") with the four test strings that assert it (Tasks 1 and 3); `PlanAction`'s doc now gives `mv`'s real faults — the basename collision and no Put Back record — and says the spike found a rename does reach `~/.Trash`. |
| — | **checked, clean** — Category 4 (the `PlanAction` refactor) and Category 3 (objc2 0.6.4 / objc2-foundation 0.3.2 signatures, the self dev-dependency) | — | Unchanged, with two knock-ons: Task 1's four `TrashPaths` literals gain `previewed: Vec::new()` in stage 6e, and the revised `RealTrasher` body (kind parameter, typed `autoreleasepool` result, debug line) was compiled and linted again against the same crate versions. |

**Also changed while applying the rulings:** Review Focus has eight inputs; Rulings 24–30 and deviations 25–30 are new, and Rulings 10, 11, 16, 21 and deviations 9, 11, 17, 21 are rewritten; `removal::Job` is owned (a `Detected` copy and `'static` recipe data) so a turn can run on the blocking pool, and `Confirmed` bundles the plan's paths with `previewed`, keeping `execute_removal` at seven parameters; `Trasher::trash` takes the kind and `MockTrasher` records it (`kinds()`); `testing::Unreadable` (stage 6c) and Task 7's `LockingTrasher` produce the permission errors; the `PathChanged` doc and copy name the self-update case; the backlog gains two entries (a dotfiles-linked `~/.local/bin`; the upgrade reading's collapse); the pre-merge Step 1 says the smoke test's debug lines from a terminal do not count; stage 6f's `launcherOnly` sentence names `{{source}}`'s official documentation instead of a website, as a test B's review added (`3e19fd9`) now requires, and fixes that test's comment; the Global Constraints, Architecture, Core Interfaces, File Structure, Task 6's Files and Interfaces, stage titles, the stage 6h clippy notes and `git add` list, both commit messages and the delivery note follow.

**How this revision was checked.** A scratch mirror of the crate's module paths (`scratchpad/c-rev-removal`: real `trash/`, `route.rs`, `removal.rs`, the `testing` module and the recipe additions; stubs for `model`, `events`, `AdapterError`, `HostEnv`), with the code exactly as the plan writes it: 63 tests green (30 in `removal`, 27 in `route` — B's 24 as of `cf7a955` and the 3 new — 5 in `trash::mock`, 1 wire test), `cargo clippy --all-targets -- -D warnings` clean, rustfmt-stable. Stage 6c alone compiles and passes its tests; stage 6d's instructions applied to it reproduce the checked file byte for byte after `cargo fmt`; stage 6c's `route.rs` instructions were applied to B's landed `route.rs`, at `dcf0e7c` and again at `cf7a955`, and pass. `RealTrasher` was compiled and linted against objc2 0.6.4 / objc2-foundation 0.3.2 in debug and release (never run: it would move files into this Mac's Trash). The serde skip was checked on serde 1.0.229, and the adapter glue of stage 6e (`match *uninstall`, the `let … else` on `self.recipe.uninstall`, `Confirmed` from the plan's vectors, `probe_strict`'s arms) was compiled against the mirror. Not run: the workspace, the TypeScript gates, anything in `~/dev/Canager-phase4`.

**Remaining risks after this review:**
- Stage 6e's adapter code, B's updated tests, Task 7's end-to-end tests and every TypeScript change were written against the landed files but compiled only in part (the glue above); the executor's first runs of stages 6e and 7 are the real check.
- The residual race of Ruling 26 is by design. A possible hardening, not tried: `NSURL`'s file-reference URLs name an object by its file id rather than its path; whether `trashItemAtURL:` accepts one, what it does for a symbolic link, and whether Finder then records Put Back are unknown and would need a spike of their own.
- The ancestry rule refuses the uninstall for a `~/.local/bin` kept as a link, even inside the home folder, and for a linked `~/.claude` that holds the download cache (backlog) — dotfiles setups do this; the one-hop rule drops the row for a launcher that reaches its root through a link outside it (the Unknown page lists it). Neither layout was observed; both change what B shipped.
- `open --stderr` is assumed to reach the debug line for a Finder-style launch; if the log stays empty, Step 3(b) cannot be read and the fallback is a clean macOS account without Full Disk Access.
- The permission tests (`chmod 000`) skip as root and say so; CI's macOS runner is a normal user. The timing tests (a 1 s budget cut, a 300 ms move) have loose upper bounds, but a badly loaded runner could still stretch them.
- The overlap rule's "as spelled" exception keys on the recipe's spelling of the kept and removed paths; step D's lists (Grok keeps `~/.grok` and moves folders inside it) rely on the same allowance, which the rule gives only while the kept folder is a real folder.
- The step-B review was landing commits in the worktree while this revision was written (`95793e7`, `3e19fd9`, `b93cacd`, `cf7a955` since `dcf0e7c`, and uncommitted edits after them). Their effects on this plan were taken in — `shadow_note`'s new tests, and `launcherOnly`'s sentence, which must now name the official documentation, never a website (stage 6f) — and every other anchor was re-checked against `cf7a955`; anything later needs the same look before this step runs.
