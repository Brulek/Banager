# UI preview in a browser

A development-only way to look at Banager's real front end in an ordinary
browser, with no Tauri window and no backend: every IPC call is answered by
a mock that pretends to be a Mac with every kind of source, update and
notice Banager can show. It exists so the UI can be screenshotted state by
state while it is being redesigned. It is never part of the app.

## Run it

```bash
pnpm install
pnpm dev:mock
```

Then open <http://localhost:1430/>. The page is the same `src/` code
`pnpm tauri dev` runs; edits hot-reload as usual. Stop the server with
Ctrl-C in its terminal (or `kill` the process, if you started it in the
background). Reloading the page starts the pretend Mac over: everything
it remembers -- operations, settings, what was updated or uninstalled --
lives in the page's memory and nowhere else.

## In the app's own window

```bash
pnpm tauri:mock
```

The same mock front end in Banager's real window, for what a browser
cannot show: the title bar drawn over the page, the traffic lights in the
sidebar, dragging the window by its top, the size it opens at and the one
it remembers. It is `pnpm tauri dev` with `src-tauri/tauri.mock.conf.json5`
merged over the app's config: the page is Vite in mock mode on port 1440
(so a preview on 1430 can stay open beside it), and the app has an
identifier of its own, `com.brulek.banager.mock`, so it keeps its window's
size apart from the app's and never reads the app's settings. The Rust
side is the app's own, but none of Banager's commands reach it:
`src/lib/api.ts`, the page's only way to them, talks to the mock. And it
starts nothing by itself -- the one refresh it runs unasked follows a
`brew update` that a refresh left running, and only the page starts a
refresh; the config's comments say more. The first run compiles the app.
Stop it with Ctrl-C in its terminal, or by quitting it (⌘Q): closing the
window only hides it, as it does in the app. Quitting never asks anything
here, even while one of the preview's updates runs: that runs in the
page's mock, not in Rust, and Rust asks before a quit only once the page
has told it that it listens for the question (`src-tauri/src/quit.rs`),
which this page never does.

The menu bar is the app's own too, and so is everything macOS does in
it: About, Hide, Quit, Close Window, the Edit and Window menus. But the
page never talks to Rust, so it never says which language it uses -- the
menu bar stays in the one it was built in, which follows macOS's
language here, this identifier having no settings of its own -- and it
never hears Settings…, the View menu's four pages (⌘1 to ⌘4), Check
Again or Search, which Rust sends only to a page that asked it to listen:
in this window those items do nothing but bring the window back when it
is closed or minimized. Nor does the page
badge Banager's icon in the Dock with its count of updates, as the app
does: it would ask Tauri, and here it asks the stand-in in
`src/dev/mockTauriWindow.ts`, which badges nothing. Nor does the Other
Programs page's Show in Finder reach this Mac's Finder: it asks the stand-in in
`src/dev/mockTauriOpener.ts`, which shows nothing.

## How it works, and why it never ships

- `src/lib/api.ts` is the only production module that imports Tauri
  (`invoke` and `Channel` from `@tauri-apps/api/core`, `listen` from
  `@tauri-apps/api/event` for the menu bar's items,
  `getCurrentWindow` from `@tauri-apps/api/window` for the Dock's badge,
  and `revealItemInDir` from `@tauri-apps/plugin-opener` for the Other
  Programs page's Show in Finder).
- `vite.config.ts` aliases `@tauri-apps/api/core` to
  `src/dev/mockTauri.ts`, `@tauri-apps/api/event` to
  `src/dev/mockTauriEvent.ts`, `@tauri-apps/api/window` to
  `src/dev/mockTauriWindow.ts` and `@tauri-apps/plugin-opener` to
  `src/dev/mockTauriOpener.ts`, in `--mode mock` only, and serves that
  mode on port 1430 (`pnpm tauri dev` keeps 1420, and `pnpm tauri:mock`
  asks for 1440). In every other mode -- `pnpm dev`
  under `pnpm tauri dev`, `pnpm build` under `pnpm tauri build`, and
  vitest -- the config resolves exactly as it did before this mode existed.
  `src/dev/mockBackend.test.ts` checks that those four are every module
  of Tauri's that production code imports: one left out would run for
  real in the preview.
- Nothing outside `src/dev/` imports anything in it, so a production
  build never contains it; `src/dev/mockBackend.test.ts` checks that for
  every module under `src/`. Every page of the preview logs a line starting
  with `[banager-ui-preview-mock]` to the console; to check a build, run
  `pnpm build` and then `grep -r banager-ui-preview-mock dist`, which
  finds nothing.
- The mock is typed against `src/lib/types.ts` and checked by
  `pnpm typecheck` like the rest of `src/`; `src/dev/mockBackend.test.ts`
  (run by `pnpm test`) checks that it answers every command `api.ts` sends
  and that an operation runs the way the real backend reports one.
- Every answer reaches the page in a task of its own, after the one that
  asked (`invoke` in `src/dev/mockTauri.ts`), as the app's come back over
  IPC -- even one the mock knows at once, so that a page awaiting one
  answer after another draws between them here as it does in the app;
  `src/dev/mockTauri.test.ts` holds it.
- The logos are not mocked: the avatars draw from the logo pack built
  into the app (`src/assets/tool-icons/`, read by `src/lib/toolIcons.ts`,
  which asks the backend for nothing), so a tool or a source the pack has
  a logo for shows it here as it does in the app. A cask's app icon still
  comes first: iTerm2 and Visual Studio Code show the generated one
  described below.
- Nor are the tools' lines in Chinese: with `lang=zh-CN`, the rows read
  the table built into the app (`src/assets/tool-descriptions/zh-CN.json`,
  through `src/lib/toolDescriptions.ts`), so git, ffmpeg and jq say what
  they are in Chinese, git's details show Homebrew's own description under
  its line, and a tool the table has no line for keeps what it said:
  iTerm2 its cask's English, TypeScript 「npm 软件包」. Nor those in
  English: in English the rows read the English table
  (`src/assets/tool-descriptions/en.json`), so prettier, tokei and httpie
  say what they are, and TypeScript, which it has no line for, still
  "npm package".

The files: `mockTauri.ts`, `mockTauriEvent.ts`, `mockTauriWindow.ts` and
`mockTauriOpener.ts` (the stand-in modules; the second listens to
nothing, the third badges nothing, and the fourth shows nothing in Finder
and says in the console which path it was handed), `mockBackend.ts` (the
commands),
`mockData.ts` (the pretend Mac), `mockIcons.ts` (its apps' icons),
`mockPlans.ts` (what each operation would run and print), `scenario.ts`
(the URL switches).

## What the pretend Mac has

Paths are under a generic home folder, `/Users/you`.

- **Homebrew** (`/opt/homebrew`): 26 formulae, 14 of them folded away as
  dependencies, and 4 casks, one of which (Visual Studio Code) updates
  itself. Updates: two formulae and one cask to update, a pinned formula,
  one update the user asked never to be reminded about (ffmpeg) and one
  version they skipped (gh 2.102.0). Two of the casks are apps (iTerm2
  and Visual Studio Code): asked for their icon (`artifact_icon`), the
  preview answers with a generated one -- a coloured square with the
  app's initial -- where the app answers with the icon macOS draws; the
  font and the cask with no app have none.
- **npm**: 4 global packages, one update. **pipx**: 2 tools, one update.
  **uv**: 2 tools, but uv did not answer, so its rows and its one update
  are last time's. **pip**: read-only, 5 packages, one update listed.
  **Cargo**: one crate from crates.io with an update that compiles
  locally, one installed from git that can never be checked. **Ollama**:
  two models, one with a new version from a third-party registry.
- **Tools with their own installer**: Claude Code (updates itself, and has
  an update), rustup (an update that cannot be cancelled once it starts),
  Antigravity CLI (a newer version it can only install itself) and Grok
  Build (an update, and a notice that it is not on the PATH).
- **Other Programs page**: five programs no source accounts for -- two plain
  files, two links an installer with administrator rights put there (one
  into an app), and a broken link to an app that was deleted.

## What it does

- A refresh takes about a second; the first one runs at startup, as in the
  app, so for a moment the toolbar says "Checking…", and so do the
  Overview, Updates and Installed pages, under a spinner and over why the
  first check takes a while. Check Again in the toolbar runs one at
  any time.
- Update and Uninstall show the preview the real adapter would build
  (an uninstall's sentence under the tool about what goes and what stays,
  Visual Studio Code's recorded uninstall steps, warnings, what would
  break, password and "can't cancel" notices, and the command behind
  "Show the command"), then run for about five seconds with the same event sequence
  the backend sends: queued, running, the tool's log lines, verifying,
  finished. Operations on the same source run one after another, at most
  three at once.
- A finished update bumps the version and removes the row; a finished
  uninstall removes the package, and a tool with its own installer
  disappears altogether.
- Cancel works on a queued or running operation, except rustup's, which
  refuses once it runs.
- Settings are kept until the page reloads. Turning on Show
  self-updating apps adds the Visual Studio Code update on the next
  refresh. Check for updates every day is kept too, and checks nothing:
  the daily check is a task of the app's Rust side, which the preview
  does not have. Notify me when there are updates turns on without
  asking anything, as where permission is granted, and nothing is ever
  notified: the page's report after each check reaches no Rust.
- After each refresh the tools' sizes are measured, as the app measures
  them (`src/dev/mockSizes.ts`): about a second and a half of 「正在计算…」
  in the Installed page's details, then 「占用空间：约312.6 MB」 for node@22,
  with 「旧版本约298.4 MB」 under it -- node@22, python@3.13, gettext and
  libuv keep older kegs -- 「至少约612.4 MB」 for Visual Studio Code (the
  round's budget ran out) and 「约22.7 MB，部分无法读取」 for pre-commit. A
  tool measured before at the same version shows at once. pip's packages,
  the font and a model get no measured size (a model keeps its own), and
  the Ollama source's page says 「Ollama模型共约6.6 GB」 under its title.
  The Installed page's sort has 「按大小」 ("By Size"): the two models
  first, then Visual Studio Code and node@22, a tool with no size last.
- On the Other Programs page, a row's Show in Finder opens nothing: the console
  says which path Finder would have been asked to show. Copy path copies
  where the browser lets the page write to the clipboard, and otherwise
  says it couldn't.

## URL switches

Combine them with `&`, for example
<http://localhost:1430/?state=offline&lang=zh-CN&page=updates>. An unknown
value falls back to the default and logs a warning in the console.

| Switch | Values | What you get |
|---|---|---|
| `state` | `full` (default) | The Mac above. |
| | `loading` | The first refresh and the Other Programs page's scan never finish. |
| | `error` | Loading what is installed fails. The failure screen appears once the app gives up retrying, about 7 seconds later, and only while the tab is visible. |
| | `refresh-error` | The same failure screen at once: the first refresh fails. |
| | `empty` | No source is set up on this Mac. |
| | `nothing` | Homebrew is set up, with nothing installed. |
| | `uptodate` | Every source answered and nothing needs updating. |
| | `hidden` | The only updates are the skipped and never-remind-me ones. |
| | `stale` | The last refresh could not finish for two sources. |
| | `notices` | Every source notice with a look of its own: Homebrew still downloading its catalogue (its operations wait for it first, and its uninstall previews are refused), npm read-only with an unverified version, Ollama not running (Open Ollama starts it), another `claude` first on the PATH, Grok Build's launcher left without its program, and a second Homebrew, the Intel one in `/usr/local`, that does not answer (so the sidebar names the two "Apple silicon" and "Intel"). The Updates and Installed pages fold them into one line, the first warning, with "N more issues" at its end to show them all. |
| | `offline` | No registry answered: Homebrew's catalogue could not be downloaded, and every other lookup is "could not check". |
| | `many` | About 800 things installed, as on a Mac that has used Homebrew for a while: the Mac above, every source answering, and 741 more real tools (`src/dev/mockManyNames.ts`) -- 580 Homebrew formulae, 40 of them libraries it installed for the others; 70 casks, 25 of them apps; 40 npm packages, 13 pipx and 12 uv tools, 20 crates and 6 Ollama models -- each one the logo pack and the description tables have. About one in seven has an update: 121 rows on the Updates page have one Banager can install. Each tool's version, install day and update come from a seeded stream of its own, so every run shows the same list. In English, those formulae and casks read "Homebrew package" or "App installed with Homebrew": the preview has no Homebrew catalogue to take their descriptions from. |
| | `preview` | The first refresh lists what is installed and then never finishes checking for updates, as a real launch looks while `brew update` runs: the Installed page lists the Mac above -- less uv, which is not answering and so is never asked for its list -- with every Uninstall off and "Found N tools · Checking for updates…" in its toolbar; the sidebar counts them, the Overview says how many with See Tools, and the Updates page keeps its spinner. (The other states that have tools to list -- all but `loading`, `error`, `refresh-error`, `empty` and `nothing` -- send this list on their first refresh too, 300 ms in, and still answer at 900 ms.) |
| `lang` | `system` (default), `en`, `zh-CN` | Settings' language at startup. |
| `tech` | `1` | Show technical details on at startup. |
| `page` | `overview` (default), `updates`, `installed`, `unknown`, `settings` | The page the window opens on; `unknown` is Other Programs. |
| `outcome` | `succeeded` (default), `failed`, `cancelled`, `unconfirmed`, `attention`, `banager`, `password` | How every operation ends. Only `succeeded` changes anything. `password`: the command stops where `sudo` wanted the Mac's password, as a cask's own step does under Banager; its log shows the command to run in Terminal. |
| `scan` | `found` (default), `stopped`, `empty`, `error` | What the Other Programs page's scan returns. |
| `sizes` | `measured` (default), `pending` | How measuring disk use goes after each refresh: the Installed page's details say 「正在计算…」 ("Calculating…") for about a second and a half, then each tool's size; with `pending` it never finishes. |

## Large list

How the two long pages fare on `?state=many` (792 installed, 121 to
update, `lang=zh-CN`): timed in headless Chrome against this preview --
`vite --mode mock`, so React's development build -- in a 1280×800
window on an M5 Pro, the median of three runs. An interaction is from
the input to the next frame drawn (Event Timing); a task is one run of
the main thread. Before is 1eaf5af; after, `VirtualList`
(`src/components/VirtualList.tsx`), `UpdateWatchers` in `src/App.tsx`
and the split memo in `useStartableUpdates`.

| | Before | After |
|---|---|---|
| Scrolling the Installed list to its end (6000 px/s): longest task, longest frame | 28 ms, 33 ms | 9 ms, 17 ms |
| The same with the CPU slowed 4×: tasks over 50 ms, frames drawn | 48, 224 | 0, 390 |
| Scrolling the Updates list: longest task | 13 ms | 5 ms |
| Update All, submitting 121 plans: main thread busy (4× slower CPU) | 0.48 s (2.61 s) | 0.43 s (2.43 s) |

Every interaction took under 100 ms before and after, and about the
same: opening Installed from the sidebar 56 ms, each key of a search
for "py" 32 and 24 ms, a source filter 24 ms, Select all 24–32 ms,
Update All's sheet 24 ms (ready 0.43 s later, 0.4 s of it the mock's
planning). What changed is scrolling, which drew the whole page again
at every step. With the CPU slowed 4×, opening Installed takes 150 ms
and a key or a filter 64–72 ms, most of it React's development checks:
a production build of this preview (`vite build --mode mock`) does them
in 72 ms and 24–32 ms, and scrolls with no task over 8 ms.
