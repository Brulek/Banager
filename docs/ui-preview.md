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
Again, Search or Help's Welcome to Banager, Common Questions, Keyboard Shortcuts, Check Tool Setup… and Copy Diagnostic Info…, which Rust sends only to a
page that asked it to listen:
in this window those items do nothing but bring the window back when it
is closed or minimized. (The page hears one only when
`window.mockMenu(...)` in the browser's console sends it; see below.) Nor does the page
badge Banager's icon in the Dock with its count of updates, as the app
does: it would ask Tauri, and here it asks the stand-in in
`src/dev/mockTauriWindow.ts`, which badges nothing. Nor does the Other
Programs page's Show in Finder reach this Mac's Finder: the mock backend's
`reveal_in_finder` shows nothing.

## How it works, and why it never ships

- `src/lib/api.ts` is the only production module that imports Tauri
  (`invoke` and `Channel` from `@tauri-apps/api/core`, `listen` from
  `@tauri-apps/api/event` for the menu bar's items,
  and `getCurrentWindow` from `@tauri-apps/api/window` for the Dock's
  badge).
- `vite.config.ts` aliases `@tauri-apps/api/core` to
  `src/dev/mockTauri.ts`, `@tauri-apps/api/event` to
  `src/dev/mockTauriEvent.ts` and `@tauri-apps/api/window` to
  `src/dev/mockTauriWindow.ts`, in `--mode mock` only, and serves that
  mode on port 1430 (`pnpm tauri dev` keeps 1420, and `pnpm tauri:mock`
  asks for 1440). In every other mode -- `pnpm dev`
  under `pnpm tauri dev`, `pnpm build` under `pnpm tauri build`, and
  vitest -- the config resolves exactly as it did before this mode existed.
  `src/dev/mockBackend.test.ts` checks that those three are every module
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
- Nor are the tools' lines in Chinese: with `lang=zh-CN` or `lang=zh-Hant`, the rows read
  the table built into the app (`src/assets/tool-descriptions/zh-CN.json`,
  through `src/lib/toolDescriptions.ts`), so git, ffmpeg and jq say what
  they are in Chinese, git's details show Homebrew's own description under
  its line, and a tool the table has no line for keeps what it said:
  iTerm2 its cask's English, TypeScript 「npm 软件包」. Nor those in
  English: in English the rows read the English table
  (`src/assets/tool-descriptions/en.json`), so prettier, tokei and httpie
  say what they are, and TypeScript, which it has no line for, still
  "npm package".

The files: `mockTauri.ts`, `mockTauriEvent.ts` and `mockTauriWindow.ts`
(the stand-in modules; the second hears nothing by
itself -- `window.mockMenu("copy-diagnostics")` in the browser's console
sends the page what a menu item sends, by the item's id in
`src-tauri/src/menu.rs` -- and the third badges nothing), `mockBackend.ts` (the
commands; its `reveal_in_finder` shows nothing in Finder and says in the
console which path it was handed, and its `open_homepage` opens no browser
and says in the console which address it was handed),
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
  are last time's. **pip**: read-only, 5 packages, one update listed; its
  page says why over the list, where other pages have 「全选」.
  **Cargo**: one crate from crates.io with an update that compiles
  locally, one installed from git that can never be checked. **Ollama**:
  two models, each with a new version: the third-party registry's one
  says its update downloads up to about 4.7 GB; the size of the update of
  the one from Ollama's own library is not known, so it says only that
  the changed files are downloaded.
- **Tools with their own installer**: Claude Code (updates itself, and has
  an update), rustup (an update that cannot be cancelled once it starts),
  Antigravity CLI (a newer version it can only install itself), Grok
  Build (an update, a notice that it is not on the PATH, and
  「终端里找不到」 on its Installed row) and Codex,
  installed by its own script beside npm's @openai/codex; its Uninstall
  previews the two links in `~/.local/bin` and `~/.codex/packages/standalone`
  going to the Trash, and `~/.codex` and `~/.zprofile` staying.
- **Other Programs page**: six programs no source accounts for -- two plain
  files, two links an installer with administrator rights put there (one
  into an app), a broken link to an app that was deleted, and a link into
  `~/Documents` (`notes-cli`) that is listed by its own name and not
  followed. A row's line names, after its path, the app it belongs to
  and 「属于系统或其他账户」 where another account owns it (aws, docker).
  Under the list, a line says two folders were left unread
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
  can be more than its rows add up to; an ⓘ after a heading or the
  subtitle with a total says so (other versions in, caches out), named
  with what it explains, 「详情：Homebrew · 2.6 GB以上」. In an English
  window at its narrowest, 800 wide (`?lang=en&page=installed`, the
  browser window 800 wide), the toolbar has no room for 「· 10.6 GB or
  more」, which wraps out of sight, and its ⓘ is hidden with it: the line
  reads 「58 tools」 alone. 960 wide, or in Chinese at 800, the line is
  whole with its ⓘ after it.
- The Installed page's 「显示」 popup also offers 「装了不止一份」: on the
  default pretend Mac, Codex's own install and npm's @openai/codex; with
  `?state=notices`, Claude Code and @anthropic-ai/claude-code as well. With
  `?state=uptodate`, which leaves Codex's own install out,
  「没有发现装了不止一份的工具」.
- The 「显示」 popup's last four choices say how many they show: on the
  default pretend Mac 「装了不止一份（2）」, Codex's two copies,
  「终端里找不到（1）」, Grok Build,
  「Homebrew已停用或弃用（2）」, QuickJot (已停用) and youtube-dl (已弃用), and
  「保留了其他版本（8）」, the formulae with an older version kept, components
  unfolded. No line over the list points at that last one; the tool setup
  check's 「8个工具保留了其他版本」 has its 查看.
  With every tool shown, the notices over the list, unfolded, end on
  「2个工具已被Homebrew停用或弃用」 with 查看, which picks that choice and
  puts the focus on its first row. No such line says how many Terminal
  can't find: Grok Build's own notice already says it. A choice that
  hides the selected tool closes its details.
- The Updates page lists 「最近的更新记录」 under its rows, as the App Store
  lists Update History under Pending (scroll to the end of the list):
  seven of the ten records the pretend history holds from earlier
  launches (`src/dev/mockHistory.ts`) -- htop and ripgrep today, then
  prettier, httpie with 「未能更新：网络连接失败」, typescript with
  「没有更新成功：版本没有变」, wget, and, behind 「再显示1条」, gh. Every success says
  「已更新」; where the version was read before and after, its tooltip says so,
  and gh's has none. httpie and typescript are also rows above:
  after a restart a row does not know the last try did not work. jq's
  failed update is not listed, as no update is offered for jq any more
  (as if updated in Terminal since); nor are an uninstall (yt-dlp) and
  an update older than 30 days (ffmpeg). An update the preview runs keeps its tick in its
  own row until the check after it, then is added at the top of
  「最近的更新记录」; with nothing left to install, 「最近的更新记录」 is at the top of
  the page. 清除记录 empties the list until the page reloads.
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
  `packages/standalone` (its ⓘ says so). Codex's own install can be
  uninstalled too: its files go to the Trash.
- `?state=preview`: the first check's list says once, in a line over it,
  「检查完成后才能卸载」 with an ⓘ; each Uninstall is off with that as its
  tooltip, and no row repeats it.
- An uninstall of an AI coding tool lists what stays after it under
  「卸载后会保留」 ("Stays after uninstalling"), as the app's preview does
  (`src/dev/mockKeptData.ts`): Codex (npm) `~/.codex`, about 38.4 MB;
  Gemini CLI (Homebrew) `~/.gemini` with no size, as for a folder that
  leads into `~/Documents`; Homebrew's ollama `~/.ollama/models`, about
  6.6 GB; and, with `?state=many`, Claude Code from npm `~/.claude` and
  `~/.claude.json`. Each has Copy Path and nothing that deletes. Under
  Gemini CLI's and ollama's list one line says what isn't needed can go to
  the Trash in Finder; not under npm's Codex's, nor under Claude Code's
  with `?state=many`, nor under Codex's own install's: another copy of the
  tool stays installed and still uses that folder. The
  native Claude Code's own list keeps those two under 「保留」 already, so
  its dialog does not repeat them.
- The uninstall of a Homebrew package another source runs on lists that
  source under 「依赖此工具的软件」, with Homebrew's own dependents, and offers
  no Uninstall (`src/dev/mockNeededBy.ts`, as the app's preview finds them
  by following links): `node@22`, 「npm及其4个工具」 (npm's own `npm` and
  `corepack` are not counted); `python@3.13`, `pipx` (from Homebrew),
  「pip及其1个工具」 and 「pipx装的3个工具」, whose venvs' Python it is;
  Homebrew's `pipx`, 「pipx及其3个工具」; Homebrew's `ollama`, 「Ollama及其
  2个模型」. The sentence under the list says which tools to uninstall
  first. With `?state=notices`, npm is the one from nodejs.org, and nothing
  runs on `node@22`. npm's own `npm` row says 「无法在此卸载」 where its
  Uninstall would be, and why behind it.
- In the Installed page's details, a tool's homepage -- iTerm2's
  「iterm2.com」 -- is a link, with 「拷贝链接」 under it. In the app it opens
  the page in the default browser; here it opens nothing, and the console
  says which address the browser would have been asked to open. Like the
  app, the preview refuses any address that is not the homepage of a tool
  it lists. With `?state=preview`, while the first check's list is shown,
  the host is plain text, not a link, until the check is done.
- On the Other Programs page, a row's Show in Finder opens nothing: the console
  says which path Finder would have been asked to show. Copy path copies
  where the browser lets the page write to the clipboard, and otherwise
  says it couldn't.
- Help's 「欢迎使用Banager」 ("Welcome to Banager"): `window.mockMenu("welcome")` in the
  console shows the welcome sheet again over any page, with or without `?welcome=1`.
  Closing it saves nothing when it was seen already.
- Help's 「键盘快捷键」 ("Keyboard Shortcuts"): `window.mockMenu("keyboard-shortcuts")` in the
  console opens the sheet over any page: the general keys (pages, Check Again, Search, ⌘W,
  ⌘Q), the lists' and the dialogs', in the window's language. Done, Escape or a click beside it closes it.
- Help's 「常见问题」 ("Common Questions"): `window.mockMenu("common-questions")` in the
  console opens the sheet over any page: ten questions, each answered in a few sentences, in the
  window's language. A question's 查看 closes the sheet and opens where the answer points: Installed
  on 「终端里找不到」, 「装了不止一份」 or sorted by size, the Updates page, Other Programs or Settings.
  The password question and 「此App会改动我的Mac吗？」 have none. Done, Escape or a click beside it closes it.
- Help's 「检查工具环境…」 ("Check Tool Setup…"): `window.mockMenu("check-tool-setup")`
  in the console, or 「查看…」 beside 「工具环境」 on the Overview or in Settings' 诊断, opens the
  sheet over any page. On the pretend Mac it says pip is 仅供查看 and uv 没有响应
  (each with 查看 to its page), 1 tool Terminal can't find and 1 installed twice
  (查看 opens Installed on that 「显示」 choice), 2 that Homebrew disabled or
  deprecated and 8 keeping other versions with their measured size (each with
  查看 to its 「显示」 choice), and the disk.
  `?state=notices` names the Intel Homebrew; `?state=preview` shows it while
  the first check runs; `?path=unread&tech=1` adds an unread folder in
  Documents, by path -- and, as that folder might hold a link to them, the
  tool Terminal can't find becomes one it couldn't check: 「终端都能找到检查过的工具」
  over 「1个工具无法确认终端能否找到」; `?path=default` shows it with
  Terminal's login settings unread.
- The Installed page's search finds a tool by a command it puts on the Mac
  as well as by its name, the command by its start: 「rg」 lists ripgrep,
  「pip3.13」 or 「pip」 python@3.13, 「tsc」 typescript, 「psql」
  postgresql@17, 「adb」 android-platform-tools, 「agy」 Antigravity CLI --
  each row with 「命令：rg」 ("Command: rg") after its description, which a
  row found by what it shows never has. It finds a tool by a word of its
  description too, in the window's language or the other: 「编程」 lists the
  AI coding tools, 「视频」 or "video" ffmpeg and youtube-dl, 「JSON」 jq.
  The field's tooltip says 「按名称、说明或命令搜索」.

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
| | `notices` | Every source notice with a look of its own: Homebrew still downloading its catalogue (its operations wait for it first, and its uninstall previews are refused), npm read-only with an unverified version, Ollama not running (Open Ollama starts it), another `claude` first on the PATH, Grok Build's launcher left without its program, and a second Homebrew, the Intel one in `/usr/local`, that does not answer (so the sidebar names the two "Apple silicon" and "Intel"). uv, which does not answer either, answered 47 minutes before the mock started, so its notice says when (「显示的是它今天08:25响应时的结果」, on the Overview and behind the ⓘ of its line), as does Tool Setup's line for it; the Intel Homebrew, never heard from, says no time. The Updates and Installed pages fold them into one line, the first warning, with "N more notes" at its end to show them all. |
| | `offline` | No registry answered: Homebrew's catalogue could not be downloaded, and every other lookup is "could not check". |
| | `many` | About 800 things installed, as on a Mac that has used Homebrew for a while: the Mac above, every source answering, and 741 more real tools (`src/dev/mockManyNames.ts`) -- 580 Homebrew formulae, 40 of them libraries it installed for the others; 70 casks, 25 of them apps; 40 npm packages, 13 pipx and 12 uv tools, 20 crates and 6 Ollama models -- each one the logo pack and the description tables have. About one in seven has an update: 125 rows on the Updates page have one Banager can install. Each tool's version, install day and update come from a seeded stream of its own, so every run shows the same list. In English, those formulae and casks read "Homebrew package" or "App installed with Homebrew": the preview has no Homebrew catalogue to take their descriptions from. And a second Python of Homebrew's, python@3.11's, beside `python3`: pip lists pip, setuptools and wheel under both, so their rows name the source after the name, 「pip（/opt/homebrew/bin/python3.11）」 and 「pip（/opt/homebrew/bin/python3）」, and wheel has a read-only update in both (search the Installed page for `wheel`). The name stays whole; the source's words give way, cut in their middle to keep the end that tells them apart -- beside the inspector at 800, 「…3.11）」 and 「…on3）」. |
| | `huge` | About 5,000 things installed (4,897), as on a Mac whose owner has several thousand formulae and casks: `many`'s Mac, and about 4,100 more tools made from its names the way sources name a tool's relatives (`src/dev/mockHugeNames.ts`) -- a versioned formula (`hugo@2`), a `-cli` or `-utils` beside it, a library (`libuv`), a cask's `@beta`, an npm `create-` package (`@google/create-gemini-cli`), a `cargo-` subcommand -- 3,893 of them Homebrew's (335 casks), the rest npm, pipx, uv and Cargo. About one in seven has an update. Each new tool puts a command on the Mac, about one in forty in a folder Terminal does not search, so 「终端里找不到」 has rows at this size too. Their names are none the logo pack or the description tables know: the rows show the neutral tile -- with the prompt, but for the casks -- their source's mark on its corner, and their source's line. For timing the long lists ("Large list" below). |
| | `preview` | The first refresh lists what is installed and then never finishes checking for updates, as a real launch looks while `brew update` runs: the Installed page lists the Mac above -- less uv, which is not answering and so is never asked for its list -- with every Uninstall off and "Found N tools · Checking for updates…" in its toolbar; the sidebar counts them, the Overview says how many with See Tools, and the Updates page keeps its spinner. (The other states that have tools to list -- all but `loading`, `error`, `refresh-error`, `empty` and `nothing` -- send this list on their first refresh too, 300 ms in, and still answer at 900 ms.) |
| | `refused` | Two sources Banager did not ask, each saying why: an Ollama whose `OLLAMA_HOST` is an `https://` address ("Connecting to Ollama over https isn't supported", no button, none of its models listed), and a second Python, in `/opt/local`, with no pip ("python3.13 doesn't include pip", a note and no warning; its page in Installed says the same). And two lookups that end the same way every time, shown as Can't check rows under "N more can't be updated here" and not counted as tools that couldn't be checked (no notice, no Check Again): tokei's, which met a certificate Banager does not trust ("Couldn't establish a secure connection to crates.io."), and rustup's, whose release file answered with a redirect Banager does not follow ("Couldn't find its latest version." only). Both have an update to offer without `state`, so the Updates count is two lower than there. |
| | `unchecked` | Nothing to update, and uv not answering, as on the Mac above: the Overview names it, "uv wasn't checked this time; everything else you can update here is up to date" (「uv这次没检查，其余能在这里更新的都已是最新」 -- Codex's own install, which Banager never checks, is there too), with its notice under it. |
| `lang` | `system` (default), `en`, `zh-CN`, `zh-Hant` | Settings' language at startup. |
| `tech` | `1` | Show technical details on at startup. |
| `welcome` | `1` | The welcome sheet of a first launch, over the first page. Closing it saves that it was seen, until the page is loaded again. Without it the preview never shows the sheet. |
| `page` | `overview` (default), `updates`, `installed`, `unknown`, `settings` | The page the window opens on; `unknown` is Other Programs. |
| `outcome` | `succeeded` (default), `failed`, `cancelled`, `unconfirmed`, `attention`, `banager`, `password`, `mixed`, `already` | How every operation ends. Only `succeeded` changes anything. `password`: the command stops where `sudo` wanted the Mac's password, as a cask's own step does under Banager; its log shows the command to run in Terminal. `already`: as the core tells them apart. On Homebrew the first update of a source updates for real and every later one finds its package already at its new version, as an earlier update of an Update all that upgraded it as a dependency leaves it -- done, 「已由前面的更新一并完成」 (one confirmed after the last that changed something ended: 「轮到它时已是新版本」). On npm, pipx, uv, Cargo and the standalone installers, where one update never updates another package, each finds its package new before its turn -- 「轮到它时已是新版本」, with no Homebrew words in its log. A model, whose digests are never compared, and an update with no newer version to aim at update as `succeeded` does. Shown on the operation bar and in 「最近的更新记录」. `mixed`: the 2nd, 4th, … operation of the session fails and the others succeed -- a batch uninstall with some of it to look at (the Installed page's result block, 「N个未能卸载」 on the operation bar). Whatever this says, Homebrew refuses to uninstall a formula something installed still needs, in its own words (`homebrewRefusal` in `src/dev/mockPlans.ts`), unless nothing ran (`banager`). |
| `scan` | `found` (default), `stopped`, `empty`, `error` | What the Other Programs page's scan returns. |
| `path` | `read` (default), `unread`, `default` | What the last refresh made of the login shell's folders, which Check Tool Setup says: every one read; one in `~/Documents` that couldn't be; or the login shell's settings never read -- the system's four folders, and no command judged. With `unread`, a command no folder read leads to has no verdict either, as `commands::judge` leaves it (the unread folder might hold its link): Check Tool Setup says how many tools it couldn't check, the copied diagnostics add 「终端里无法确认」, and the Installed page's 「终端里找不到」 lists none, saying none was found. |
| `sizes` | `measured` (default), `pending` | How measuring disk use goes after each refresh: the Installed page's details say 「正在计算…」 ("Calculating…") for about a second and a half, then each tool's size; with `pending` it never finishes. |
| `accent` | `blue` (default), `purple`, `pink`, `red`, `orange`, `yellow`, `green`, `graphite` | The accent the user picked in System Settings, macOS 27's own value for each, in the light and the dark appearance, darkened by 15% under Increase Contrast (`src/dev/mockAccent.ts`). On `yellow`, `green`, `orange`, `graphite` and `pink` (and `red` in the dark appearance) the words on the accent -- Update All, a sheet's default button, a menu's highlighted item, the focused list's selected row -- are black; on the others, white (decision I21b, `src/lib/accentInk.ts`). It sets `--color-accent` itself, so it shows what the words do on each accent, not whether the window's WebKit computes `AccentColor` from System Settings: that is still to be seen in the real window (I21b in `docs/superpowers/backlog.md`). |

With `?path=unread`, npm's global folder is `~/Documents/npm-global`, a
protected place, so npm's rows are 「仅供查看」 ("View only"), and the ⓘ
says protected places aren't read rather than that the account can't change
the folder (decision I23). pipx's Poetry also has its environment under
`~/Documents/venvs/poetry`. Its command claims were dropped, but the snapshot
retains `commands_unavailable: true`, so it contributes to the couldn't-check
count even with an empty command list. Open Help → Check Tool Setup to see it.

## Uninstalling several tools at once

On the Installed page, tick rows (Space ticks the row that has the focus)
and press 「卸载所选（N）…」 in the toolbar. Everything the batch sheet can
say is on the `full` Mac:

- `ffmpeg` with `x264` (under 「另有…个随其他软件安装的组件」): both go, `ffmpeg`
  first, and 「在“ffmpeg”卸载之后再卸载。」 under `x264`;
- `wget` with `openssl@3`: `openssl@3` stays, still used by `node@22`,
  `postgresql@17` and `python@3.13`;
- `htop`: Homebrew could not check what needs it, so it goes last;
- Claude Code: what moves to the Trash, and `~/.claude` kept, Copy Path only;
- Homebrew's `node@22`, `python@3.13`, `pipx` or `ollama`, alone or with
  anything: it stays, because other sources run on it, said as its own
  confirmation says it (「还有软件要用到它：npm及其4个工具。要卸载它，请先卸载
  npm装的4个工具。」); what only they still need stays with them (`mpdecimal`
  ticked with `python@3.13`);
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

### About 5,000 tools (`?state=huge`)

The same questions on `?state=huge` (4,892 installed, 753 to update), with
more steps, timed by an earlier version of `scripts/perf/bench-huge.mjs`
(kept outside the repository; the one here runs the same scenario) in
headless Chrome against a production build of the preview (`vite build
--mode mock`, served by `vite preview`) in a 1280×800 window on an M5 Pro:
the median of three runs, and one run with the CPU slowed 4×. Before is
8798a5c; after, the commits of the track p3-perf-at-scale. A step set by a
popup (the 「显示」 and 排序方式 choices) is timed from the change to the
next frame; the others are interactions as above. Two rows did not change
beyond what separates one run from the next: opening Installed and ticking
rows. Eight runs of each build, taken in turn, gave the ranges shown, with
medians a few milliseconds apart.

| | Before | After | 4× slower CPU, before → after |
|---|---|---|---|
| Opening Installed from the sidebar: interaction, longest task | about 50–65 ms, 30–48 ms | the same, within run-to-run noise | about 135–160 ms, 120–140 ms, before and after |
| A key of a search for "py" (the first): longest task | 17 ms | 11 ms | 42 → 29 ms |
| 「显示」 back to every tool, to the next frame | 25 ms | 19 ms | 92 → 79 ms |
| 排序方式 By Size, to the next frame | 29 ms | 18 ms | 100 → 60 ms |
| By Date, By Name, to the next frame | 13, 14 ms | 11, 8 ms | 64 → 39, 48 → 31 ms |
| Ticking 20 rows: main thread busy | about 70–280 ms | the same, within run-to-run noise | one run each, not compared |
| 「卸载所选」's sheet for them: longest task | 11 ms | 9 ms | 55 → 37 ms |
| Update All's sheet for 753: longest task, ready after (since then: below) | 56 ms, 0.80 s | 37 ms, 0.47 s | 252 → 132 ms, 1.99 → 0.60 s |
| Update All, starting all 753: main thread busy (since then: below) | 25 s | 3.3–5.9 s (median 4.1), no task over 50 ms | over 120 s (timed out) → 33 s, 24 tasks over 50 ms |
| Scrolling either list to its end: longest frame | 17 ms | 17 ms | 33 → 17–33 ms |

What took the time: Update All's sheet looked each tool up in the whole
installed list at every drawing -- once per plan and once per update
started, 753 × 4,892 each time -- and worded every plan's notes again at
each; and the preview's own backend set every row's AI family once per
plan. The Installed page compared names through `Intl.Collator` at every
sort, search key and 「显示」 choice, built two key ids per comparison
when sorting by size, and went through every row to see which could be
ticked at every drawing. Now each tool is looked up by key, notes are
worded once per plan, the name order is worked out once per check
(`rankedComparator`, src/lib/sortRank.ts) and the tickable rows once per
change of the list. That last change, and the name order, did not make
opening Installed or ticking rows measurably faster.

Not timed at this size: the 「AI工具」 and 「装了不止一份」 choices list few
rows here (8–20 ms), because a tool belongs to a family only when it is
one of the AI tools `crates/banager-core/data/ai-tools.json` names, in
the preview as in the app. So `twinsByArtifact` (src/lib/commands.ts), which compares every pair
of copies within a family, runs over a handful of copies per family and
is not stressed by `?state=huge`. 「终端里找不到」 has about 100 rows.

Still there with this many tools: each page shown fetches the snapshot
again (TanStack Query's refetch on mount, as does a 「显示」 choice that
brings back the line over the list), and taking in about 5,000 tools --
the reply copied, and compared with the one held -- is a task of its own
after the page is drawn: at least 45 ms of it with the CPU slowed 4×, in
a profile. Every interaction stays under 100 ms at full speed; with the
CPU slowed 4×, opening Installed takes about 135–160 ms and the Updates
page 80 ms.

#### Update All with 754 updates (track p5)

Two more steps for Update All on `?state=huge`, which has 754 updates
now, timed by `scripts/perf/bench-huge.mjs` -- the script above,
finding the sheet's Update by its new name 「更新这754个」 and counting the
`list_operations` the preview answers; its header says how to build,
serve and run it -- against production builds of the preview, as above,
and summed up by `scripts/perf/summ.mjs`. Before is 7d1d2724; after, the
commits of the track p5 up to 52a6146a. Eight runs of each build, taken
in turn: the median, and in brackets the range.

| | Before | After | 4× slower CPU, before → after |
|---|---|---|---|
| Update All's sheet: longest task | 34 ms [30–37] | 18 ms [12–20] | 124 ms [121–128] → 47 ms [43–50]; tasks over 50 ms 2 → 0 |
| The same: main thread busy in all | 130 ms [116–144] | 178 ms [165–199] | 0.50 s [0.48–0.52] → 0.99 s [0.94–1.11] |
| The same: Update on after | 0.46 s [0.44–0.47] | 0.44 s [0.42–0.45], within noise | 0.57 s [0.57–0.58] → 0.55 s [0.54–0.57], within noise |
| The same: every tool drawn after | 0.46 s [0.45–0.48] | 0.46 s [0.43–0.47] | 0.60 s [0.59–0.61] → 0.85 s [0.61–1.20] |
| Starting all 754: `list_operations` fetched | 1,514 times [1,511–1,520] | 12 [12–13] | 1,550 [1,547–1,556] → 76 [72–91] |
| The same: main thread busy | 3.18 s [3.08–4.15] | 2.73 s [2.59–3.01] | 23.2 s [22.5–27.4] → 20.1 s [18.6–23.9]; 7 of 8 runs under the fastest before |
| The same: longest task | 11 ms [10–14] | 10 ms [9–12], within noise | 73 ms [70–76] → 74 ms [69–86], within noise; tasks over 50 ms 12 [10–16] → 9.5 [9–13] |

What changed. The operations are fetched once a frame (16 ms) after a
status change or a start asks for them, and no sooner than 250 ms after
the last fetch while asks keep coming (`refetchOperations`,
src/lib/operationsRefetch.ts); each started update used to ask twice --
its submit's answer and its Queued event -- for a list of every
operation so far. Fetching once a frame without the 250 ms was tried
first: about 110 fetches at full speed, but every answer was then taken
in and drawn -- before, each ask cancelled the fetch still on its way
(React Query's `invalidateQueries`) -- and with the CPU slowed 4× the
main thread was busier than before (21.7 s against 20.2 s, three runs
each). The sheet draws its tools in turns
(`useToolsInTurn` in src/components/SheetParts.tsx): 9 with the dialog
and 60 more per transition, and once the plans are back its first 9 in
their final order with their notes at once, the others a turn at a time.
Profiled, the long task as the plans came back was less the drawing than
the layout of all 754 tools, moved and with their notes, which putting
the focus on Update forced at once (80 of 135 ms at 4×). And the sheet's
tools draw their own hairlines (`SheetToolList`'s `rowsSeparate`): with
the list's `> * + *` rule, a tool put in or taken out anywhere but at
the end had Chrome work out the style of every tool after it again --
about 10,000 elements, 35–40 ms at 4× -- at every turn.

What it costs: every turn draws the dialog again, so the sheet keeps the
main thread busy longer in all, in tasks under 50 ms, and at 4× its last
tools come up to 0.6 s later; Update comes on as before. Still there:
each update Update All starts draws the Updates page and the dialog
again -- the batch, the selection and the update's target all change --
which is most of the 2.7 s and the 20 s; in a profile at full speed (of
this track's first build), about a tenth of it is `useStartableUpdates` (src/components/UpdateProgress.tsx)
going through all 754 updates again in each of its callers, as the new
target changes `useUpdateOperationFor`'s lookup at every start.

Traditional Chinese preview: <http://localhost:1430/?lang=zh-Hant&page=settings>.
The picker shows 繁體中文; tool rows load the Taiwan Traditional Chinese descriptions,
falling back to Simplified Chinese and then English if a line is missing.

In a Traditional Chinese window, searching also matches the English descriptions.
Search loads no description table until a query is entered; rows load the tables they need.
