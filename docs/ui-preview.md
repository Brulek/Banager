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
Again, Search or Help's Check Tool Setup… and Copy Diagnostic Info…, which Rust sends only to a
page that asked it to listen:
in this window those items do nothing but bring the window back when it
is closed or minimized. (The page hears one only when
`window.mockMenu(...)` in the browser's console sends it; see below.) Nor does the page
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
`mockTauriOpener.ts` (the stand-in modules; the second hears nothing by
itself -- `window.mockMenu("copy-diagnostics")` in the browser's console
sends the page what a menu item sends, by the item's id in
`src-tauri/src/menu.rs` -- the third badges nothing, and the fourth shows nothing in Finder
and says in the console which path it was handed), `mockBackend.ts` (the
commands),
`mockData.ts` (the pretend Mac), `mockIcons.ts` (its apps' icons),
`mockPlans.ts` (what each operation would run and print), `scenario.ts`
(the URL switches).

## What the pretend Mac has

Paths are under a generic home folder, `/Users/you`.

- **Homebrew** (`/opt/homebrew`): 26 formulae, 14 of them folded away as
  dependencies, and 4 casks, one of which (Visual Studio Code) updates
  itself. Updates: one formula and one cask to update, a pinned formula,
  one update the user asked never to be reminded about (ffmpeg), one
  version they skipped (gh 2.102.0) and one tool put off for 30 days,
  12 of them left (wget; Settings lists it with its date). Two of the casks are apps (iTerm2
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
  Antigravity CLI (a newer version it can only install itself), Grok
  Build (an update, and a notice that it is not on the PATH) and Codex,
  installed by its own script and listed only, beside npm's @openai/codex.
- **Other Programs page**: six programs no source accounts for -- two plain
  files, two links an installer with administrator rights put there (one
  into an app), a broken link to an app that was deleted, and a link into
  `~/Documents` (`notes-cli`) that is listed by its own name and not
  followed. Under the list, a line says two folders were left unread
  because they are in protected places; with Show technical details on,
  its ⓘ names them.

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
  refresh. Check for updates (Manually, Daily or Weekly) is kept too,
  and moves the next check's time under it, and checks nothing:
  the daily check is a task of the app's Rust side, which the preview
  does not have. Notify me when there are updates turns on without
  asking anything, as where permission is granted, and nothing is ever
  notified: the page's report after each check reaches no Rust. Notify me
  when operations finish turns on the same way; when a run of updates or
  uninstalls ends, the page's report is only checked for its shape
  (`report_finished_run` in `mockBackend.ts`) and posts nothing.
- After each refresh the tools' sizes are measured, as the app measures
  them (`src/dev/mockSizes.ts`): about a second and a half of 「正在计算…」
  in the Installed page's details, then 「占用空间：约312.6 MB」 for node@22,
  and in the row under it 「其他版本：22.22.0」 with 「约298.4 MB」 under the
  version -- git, node@22, python@3.13, gettext, libuv, openssl@3,
  readline and youtube-dl keep other kegs, and each one's versions and
  size agree -- 「612.4 MB以上」 for Visual Studio Code (the
  round's budget ran out) and 「约22.7 MB，部分无法读取」 for pre-commit. A
  tool measured before at the same version shows at once. pip's packages,
  the font and a model get no measured size (a model keeps its own,
  said as 「占用空间 约2 GB」), and the Ollama source's page says
  「2个模型 · Ollama模型共约6.6 GB」 under its title, its tooltip saying that
  files several models share count once.
  The Installed page's sort has 「按大小」 ("By Size"): the two models
  first, then Visual Studio Code and node@22, a tool with no size last;
  while it is on, each row shows its size where the version was
  (「约4.7 GB」, 「正在计算…」, or 「—」).
  It also has 「按安装日期」 ("By Date Installed"):
  the Homebrew formulae and casks newest first -- node@22, gh, ollama --
  each row showing the day where the version was (「8月23日」, last
  year's with the year, youtube-dl's 「2025年4月19日」); then every other
  source's tools by name with 「—」 (@openai/codex, aider-chat, Claude
  Code, pip's packages, the models), since only Homebrew says when a tool
  was installed. The preview clears the day it gave any other source's
  row (`onlyHomebrewDates`), so the order is the app's.
  Sorted 「按来源」, each source's heading says what it takes after its
  count, 「Homebrew · 33个 · 2.6 GB以上」 (the font has no size, so "or more"),
  「Ollama · 2个模型 · 约6.6 GB」 as its models' line says; pip has no number. The
  toolbar says the whole list's, 「58个工具 · 10.6 GB以上」, or one
  source's on its page. No number while the round measures, or while a
  search or the 「显示」 popup narrows a heading's count; while the popup
  shows only some, the toolbar says how many of how many,
  「58个工具中的2个」. A total counts a
  formula's other versions (其他版本), which its row's size leaves out, so a heading
  can be more than its rows add up to; hovering a heading or the subtitle
  with a total shows a tooltip that says so (other versions in, caches out).
- The Installed page's 「显示」 popup also offers 「装了不止一份」: on the
  default pretend Mac, Codex's own install and npm's @openai/codex; with
  `?state=notices`, Claude Code and @anthropic-ai/claude-code as well. With
  `?state=uptodate`, which leaves Codex's own install out,
  「没有发现装了不止一份的工具」.
- The 「显示」 popup's last two choices say how many they show: on the
  default pretend Mac 「终端里找不到（1）」, Grok Build, and
  「Homebrew已停用或弃用（2）」, QuickJot (已停用) and youtube-dl (已弃用).
  With every tool shown, the notices over the list, unfolded, end on
  「2个工具已被Homebrew停用或弃用」 with 查看, which picks that choice and
  puts the focus on its first row. No such line says how many Terminal
  can't find: Grok Build's own notice already says it. A choice that
  hides the selected tool closes its details.
- The Updates page lists 「最近更新」 under its rows, as the App Store
  lists Recently Updated under Pending (scroll to the end of the list):
  seven of the ten records the pretend history holds from earlier
  launches (`src/dev/mockHistory.ts`) -- htop and ripgrep today, then
  prettier, httpie with 「未能更新：网络连接失败」, typescript with
  「结果不符」, wget, and, behind 「还有1个」, gh, which says 「已更新」
  where the successes before it say 「已核实」. httpie and typescript are also rows above:
  after a restart a row does not know the last try did not work. jq's
  failed update is not listed, as no update is offered for jq any more
  (as if updated in Terminal since); nor are an uninstall (yt-dlp) and
  an update older than 30 days (ffmpeg). An update the preview runs keeps its tick in its
  own row until the check after it, then is added at the top of
  「最近更新」; with nothing left to install, 「最近更新」 is at the top of
  the page. 清除 empties the list until the page reloads.
- youtube-dl's details have every row the details can have for a
  Homebrew formula at once, for checking their order: under the
  description, a callout with what Homebrew's 「已弃用」 means and the name
  Homebrew suggests; then the facts (version, 占用空间, 其他版本 with its
  size and an ⓘ, date installed, homepage, and 状态), then 「在终端里输入」,
  and last Homebrew's notes, folded.
- Codex's own install and npm's @openai/codex (default pretend Mac): both
  rows say 「装了两份」; the details say under the description which copy
  typing `codex` runs, that Terminal does not use the other, and, for
  npm's copy, that it can be uninstalled. On the Updates page npm's copy
  says 「终端用另一份」; its uninstall preview says Codex's own copy stays
  and `codex` still works, and ~/.codex's size leaves out
  `packages/standalone` (its ⓘ says so). Codex's own install says
  「只列出」 in its 状态; it would on its row too, without the twin.
- `?state=preview`: the first check's list says once, in a line over it,
  「检查完成后才能卸载」 with an ⓘ; each Uninstall is off with that as its
  tooltip, and no row repeats it.
- An uninstall of an AI coding tool lists what stays after it under
  「卸载后会保留」 ("Stays after uninstalling"), as the app's preview does
  (`src/dev/mockKeptData.ts`): Codex (npm) `~/.codex`, about 38.4 MB;
  Gemini CLI (Homebrew) `~/.gemini` with no size, as for a folder that
  leads into `~/Documents`; Homebrew's ollama `~/.ollama/models`, about
  6.6 GB; and, with `?state=many`, Claude Code from npm `~/.claude` and
  `~/.claude.json`. Each has Copy Path and nothing that deletes. The
  native Claude Code's own list keeps those two under 「保留」 already, so
  its dialog does not repeat them.
- On the Other Programs page, a row's Show in Finder opens nothing: the console
  says which path Finder would have been asked to show. Copy path copies
  where the browser lets the page write to the clipboard, and otherwise
  says it couldn't.
- Help's 「检查工具环境…」 ("Check Tool Setup…"): `window.mockMenu("check-tool-setup")`
  in the console, or 「检查…」 beside 「诊断信息」 in Settings' 关于, opens the
  sheet over any page. On the pretend Mac it says pip is 仅供查看 and uv 没有响应
  (each with 查看 to its page), 1 tool Terminal can't find and 1 installed twice
  (查看 opens Installed on that 「显示」 choice), 2 that Homebrew disabled or
  deprecated, 8 keeping other versions with their measured size, and the disk.
  `?state=notices` names the Intel Homebrew; `?state=preview` shows it while
  the first check runs; `?path=unread&tech=1` adds an unread folder in
  Documents, by path; `?path=default` shows it with Terminal's login settings
  unread.

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
| | `hidden` | The only updates are the skipped, put-off and never-remind-me ones. |
| | `stale` | The last refresh could not finish for two sources. |
| | `notices` | Every source notice with a look of its own: Homebrew still downloading its catalogue (its operations wait for it first, and its uninstall previews are refused), npm read-only with an unverified version, Ollama not running (Open Ollama starts it), another `claude` first on the PATH, Grok Build's launcher left without its program, and a second Homebrew, the Intel one in `/usr/local`, that does not answer (so the sidebar names the two "Apple silicon" and "Intel"). The Updates and Installed pages fold them into one line, the first warning, with "N more issues" at its end to show them all. |
| | `offline` | No registry answered: Homebrew's catalogue could not be downloaded, and every other lookup is "could not check". |
| | `many` | About 800 things installed, as on a Mac that has used Homebrew for a while: the Mac above, every source answering, and 741 more real tools (`src/dev/mockManyNames.ts`) -- 580 Homebrew formulae, 40 of them libraries it installed for the others; 70 casks, 25 of them apps; 40 npm packages, 13 pipx and 12 uv tools, 20 crates and 6 Ollama models -- each one the logo pack and the description tables have. About one in seven has an update: 121 rows on the Updates page have one Banager can install. Each tool's version, install day and update come from a seeded stream of its own, so every run shows the same list. In English, those formulae and casks read "Homebrew package" or "App installed with Homebrew": the preview has no Homebrew catalogue to take their descriptions from. |
| | `preview` | The first refresh lists what is installed and then never finishes checking for updates, as a real launch looks while `brew update` runs: the Installed page lists the Mac above -- less uv, which is not answering and so is never asked for its list -- with every Uninstall off and "Found N tools · Checking for updates…" in its toolbar; the sidebar counts them, the Overview says how many with See Tools, and the Updates page keeps its spinner. (The other states that have tools to list -- all but `loading`, `error`, `refresh-error`, `empty` and `nothing` -- send this list on their first refresh too, 300 ms in, and still answer at 900 ms.) |
| `lang` | `system` (default), `en`, `zh-CN` | Settings' language at startup. |
| `tech` | `1` | Show technical details on at startup. |
| `welcome` | `1` | The welcome sheet of a first launch, over the first page. Closing it saves that it was seen, until the page is loaded again. Without it the preview never shows the sheet. |
| `page` | `overview` (default), `updates`, `installed`, `unknown`, `settings` | The page the window opens on; `unknown` is Other Programs. |
| `outcome` | `succeeded` (default), `failed`, `cancelled`, `unconfirmed`, `attention`, `banager`, `password`, `mixed` | How every operation ends. Only `succeeded` changes anything. `password`: the command stops where `sudo` wanted the Mac's password, as a cask's own step does under Banager; its log shows the command to run in Terminal. `mixed`: the 2nd, 4th, … operation of the session fails and the others succeed -- a batch uninstall with some of it to look at (the Installed page's result block, 「N个未能卸载」 on the operation bar). Whatever this says, Homebrew refuses to uninstall a formula something installed still needs, in its own words (`homebrewRefusal` in `src/dev/mockPlans.ts`), unless nothing ran (`banager`). |
| `scan` | `found` (default), `stopped`, `empty`, `error` | What the Other Programs page's scan returns. |
| `path` | `read` (default), `unread`, `default` | What the last refresh made of the login shell's folders, which Check Tool Setup says: every one read; one in `~/Documents` that couldn't be; or the login shell's settings never read -- the system's four folders, and no command judged. |
| `sizes` | `measured` (default), `pending` | How measuring disk use goes after each refresh: the Installed page's details say 「正在计算…」 ("Calculating…") for about a second and a half, then each tool's size; with `pending` it never finishes. |

## Uninstalling several tools at once

On the Installed page, tick rows (Space ticks the row that has the focus)
and press 「卸载所选（N）…」 in the toolbar. Everything the batch sheet can
say is on the `full` Mac:

- `pipx` with `python@3.13`: both go, `pipx` first, and 「在“pipx”卸载之后再卸载。」
  under `python@3.13`;
- `node@22` with `openssl@3` (under 「另有…个随其他软件安装的组件」): `openssl@3`
  stays, still used by `postgresql@17`, `python@3.13` and `wget`;
- `htop`: Homebrew could not check what needs it, so it goes last;
- Claude Code: what moves to the Trash, and `~/.claude` kept, Copy Path only;
- an Ollama model with Homebrew's `ollama`, or a pipx tool with Homebrew's
  `pipx`: the program stays (the tools need it), and with `pipx` so does
  `python@3.13`;
- rustup: it cannot be cancelled once it starts, so it is left to its row;
- Microsoft Visual Studio Code: a cask with steps of its own, and the
  password note.

`?outcome=mixed` leaves half of a batch not uninstalled, for the result
block over the list; `?state=preview` shows no checkboxes (every Uninstall
waits for the first check); `?state=many` has more rows than one batch takes
(20), which 「全选」 then only clears.

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
