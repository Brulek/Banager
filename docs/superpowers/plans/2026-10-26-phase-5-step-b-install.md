# Phase 5 step B: install from the catalog (2026-10-26)

Step B of `docs/superpowers/2026-09-29-phase-5-proposal.md` §五, after
step A (`2026-10-26-phase-5-step-a-discover.md`). Discover's rows for six
entries get an Install button: Claude Code, Codex, Gemini CLI, Node.js,
uv and ffmpeg, each a Homebrew formula or cask. Homebrew itself is step
C's (Terminal hand-off), Ollama step D's (with a model to start with).

## What the user sees

- A "Not installed" row of one of the six has Install (「安装」) where its
  state was. Clicking it opens a sheet, as an update's or an uninstall's
  does: the tool's logo, name and what it is; "From Homebrew, which
  checks the download before installing it"; "Homebrew also installs
  what it needs, and may update some of what's already installed";
  how to use it once installed; for a cask, that the Mac's password may
  be asked for; and Show Command (`CommandPreview`): the very command
  that runs, with its environment.
- Install starts it; the sheet closes, the row says "Installing…" with a
  spinner, and the operation bar shows it as every operation is shown.
- When it ends, the refresh after it says whether it is there: the row
  then says Installed, and its line says how to use it, with Copy for the
  command to type (`claude`, `codex`, `gemini`, `node`, `uv`, `ffmpeg`).
  An install that failed or could not be confirmed says so in the row,
  as an update's does, and opens its log.

## How it is planned

- `plan_install(catalog_id)` (src-tauri/src/ipc.rs): the window names a
  catalog entry, never a package (proposal §3.4). Rust works out the
  request from the snapshot (`catalog::install_request`): the entry's
  own formula or cask, under the first Homebrew that answered and can be
  written to. Then `Session::issue_plan` plans it as any operation is --
  every gate, the adapter's own command, the store -- and
  `submit_operation` runs it by its plan id.
- Refused before any adapter is asked: an id the catalog does not offer
  from here (`refused`); an entry that is not "Not installed" on this Mac
  now (`catalog_state`, with its state, which the sheet says in the row's
  words).
- `plan_operation` still refuses every `Install`.

## Decisions taken on the author's behalf (2026-10-26)

1. **Six now.** Ollama's install waits for step D, which installs it with
   a model to start with and opens it; Homebrew's is step C.
   `CatalogItem::installs_here` says which.
2. **"Also installs" said in general.** S0 ④ (whether `brew install
   --dry-run` only reads) has not been done, so nothing extra runs for the
   preview: the sheet says Homebrew may install what the tool needs and
   update some of what is installed, without names or a count.
3. **Which Homebrew.** The first of the snapshot's Homebrews that answered
   and can be written to, in detection order (`/opt/homebrew` before
   `/usr/local`); Show Command names its path.
4. **The how-to-use line after installing.** The row's line becomes the
   entry's how-to-use sentence, with Copy for the command to type: a word
   for the user to type, not a command Banager runs.
5. **Passwords.** As for an update: a cask's sheet says the Mac may ask
   for its password (`needs_password`); the three casks the catalog has
   install no pkg.

## The work

- `catalog.rs`: `installs_here`, `CatalogEntry::installs_here`,
  `install_request`, `InstallRefusal`; tests.
- `ipc.rs`: `plan_install`, registered in `lib.rs`; tests: a plan for a
  Not installed entry, refusals for an installed one, an unknown id, one
  not offered from here, and that `plan_operation` still refuses.
- Window: `planInstall` (api.ts), `usePlanInstall` (queries.ts),
  `CatalogEntry.installs_here` (types.ts); `InstallSheet`; Discover's
  Install button, the row's progress and its how-to-use line; words in
  three locales; the mock backend's `plan_install` and its install.
- Docs: `what-we-run.md` "Discover" section; `docs/ui-preview.md`.
