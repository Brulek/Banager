# Phase 5 step A: the catalog and the Discover page, read-only (2026-10-26)

Step A of `docs/superpowers/2026-09-29-phase-5-proposal.md` §五. The author
accepted the proposal's answers to Q1–Q5 on 2026-10-06 (item R9 of
`Banager/待你拍板的事项.md`: "照提案推荐，合并后开始"), and ⑭ was merged
into main on 2026-10-15, so phase 5 starts here. This step installs
nothing and offers no button that would: it lists the catalog and says,
from the snapshot, which entries this Mac already has.

## What the user sees

A new page, 「发现」 / "Discover", in the sidebar after 「已安装」, and in
the menu bar's View menu with ⌘5 (⌘1–⌘4 keep the pages they open now).
It lists the catalog by category -- Basics (Homebrew), AI assistants
(Claude Code, Codex, Gemini CLI), Local models (Ollama), Common tools
(Node.js, uv, ffmpeg) -- each row with the tool's logo, its name, one line
of what it is, and on the right its state:

| State | Row says | When |
|---|---|---|
| `Installed` | 「已安装」 | any way of having it the catalog knows of is in the snapshot |
| `NotInstalled` | 「未安装」 | not installed, and its source (Homebrew) is there and can install |
| `NeedsHomebrew` | 「需先安装Homebrew」 | not installed, and no Homebrew that can install is there |
| `NeedsMacOS` | 「需要macOS 14」 | not installed, and this Mac's macOS is older than the entry needs |
| `HomebrewUnsupported` | 「Homebrew已不支持这台Mac」 | the Homebrew entry, on an Intel Mac without Homebrew |

Each row's ⓘ says two more sentences: who needs it, and how it is used
once installed. No install button yet (step B); no sheet for installing
Homebrew yet (step C); the recommended model comes with step D.

## Decisions taken on the author's behalf (2026-10-26)

1. **Eight entries now, the ninth with step D.** The proposal's 9th entry,
   "a recommended model", needs the Mac's memory and registry sizes to
   choose; it is step D's, and comes with the Ollama install.
2. **The catalog is Rust data, the words are i18n.** `catalog.rs` holds
   each entry's id, category, what installs it (Homebrew itself, or a
   Homebrew formula or cask by name) and how it is recognised; it holds no
   command (spec §6). The three sentences live in the three locale files,
   keyed by the id, and a test checks every id has them.
3. **Recognised by every known way of having it.** The AI assistants and
   Ollama by their family in `data/ai-tools.json` (`ArtifactFacts::family`:
   Claude Code's npm package, both casks and its own installer all count);
   Homebrew by any Homebrew source; Node.js by Homebrew's `node` or a
   versioned `node@N`, or any npm source (npm runs on a Node); uv by the
   `uv` formula or a uv source; ffmpeg by the `ffmpeg` formula or a
   versioned one. Ollama also by an Ollama source (the app from ollama.com).
4. **Host facts read once, without running anything.** macOS's major
   version (`sysctlbyname("kern.osproductversion")`) and whether the Mac
   is Apple silicon (`hw.optional.arm64`, true under Rosetta too), for the
   two states that depend on them. Ollama's cask needs macOS 14
   (`depends_on macos: ">= :sonoma"`).
5. **Not the front door yet.** The empty-Mac rule (open on Discover) is
   step C's, with the Homebrew sheet; until then Discover opens like any
   page.

## The work

- `crates/banager-core/src/catalog.rs`: `CATALOG`, `CatalogEntry`,
  `CatalogState`, `Host::of_this_mac`, `entries(snapshot, host)`; tests:
  ids unique, names pass `validate_package_name`, every family exists in
  `ai-tools.json`, a table of snapshots to states.
- `src-tauri/src/ipc.rs`: `get_catalog`, read-only, from the current
  snapshot; registered in `lib.rs`.
- Front end: `CatalogEntry` in `types.ts`, `getCatalog` in `api.ts`,
  `useCatalog` in `queries.ts` (refetched as the snapshot changes),
  `DiscoverPage.tsx`, the `discover` page in `store/ui.ts`, `Sidebar.tsx`,
  `App.tsx`, `menu.ts` and `src-tauri/src/menu.rs`, the shortcuts sheet,
  copy in `en.json`, `zh-CN.json` and `zh-Hant.json`, and the mock
  backend's `get_catalog`.
- `docs/what-we-run.md`: a "Discover" section (the catalog is in the
  app; listing it reads the snapshot and two sysctl values, runs nothing,
  connects nowhere); `docs/ui-preview.md`.

Done when the five gates pass, every new field has a reader, and the
Discover page shows the eight entries with the right states in the
browser preview's `full`, `empty` and `notices` states.
