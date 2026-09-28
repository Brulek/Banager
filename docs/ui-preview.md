# UI preview in a browser

A development-only way to look at Canager's real front end in an ordinary
browser, with no Tauri window and no backend: every IPC call is answered by
a mock that pretends to be a Mac with every kind of source, update and
notice Canager can show. It exists so the UI can be screenshotted state by
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

The same mock front end in Canager's real window, for what a browser
cannot show: the title bar drawn over the page, the traffic lights in the
sidebar, dragging the window by its top, the size it opens at and the one
it remembers. It is `pnpm tauri dev` with `src-tauri/tauri.mock.conf.json5`
merged over the app's config: the page is Vite in mock mode on port 1440
(so a preview on 1430 can stay open beside it), and the app has an
identifier of its own, `com.brulek.canager.mock`, so it keeps its window's
size apart from the app's and never reads the app's settings. The Rust
side is the app's own, but none of Canager's commands reach it:
`src/lib/api.ts`, the page's only way to them, talks to the mock. And it
starts nothing by itself -- the one refresh it runs unasked follows a
`brew update` that a refresh left running, and only the page starts a
refresh; the config's comments say more. The first run compiles the app.
Stop it with Ctrl-C in its terminal, or by quitting the window.

The menu bar is the app's own too, and so is everything macOS does in
it: About, Hide, Quit, Close Window, the Edit and Window menus. But the
page never talks to Rust, so it never says which language it uses -- the
menu bar stays in the one it was built in, which follows macOS's
language here, this identifier having no settings of its own -- and it
never hears Settings…, Check Again or Search, which Rust sends only to a
page that asked it to listen: in this window those three do nothing. Nor
does the page badge Canager's icon in the Dock with its count of
updates, as the app does: it would ask Tauri, and here it asks the
stand-in in `src/dev/mockTauriWindow.ts`, which badges nothing.

## How it works, and why it never ships

- `src/lib/api.ts` is the only production module that imports Tauri
  (`invoke` and `Channel` from `@tauri-apps/api/core`, `listen` from
  `@tauri-apps/api/event` for the menu bar's items, and
  `getCurrentWindow` from `@tauri-apps/api/window` for the Dock's badge).
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
  with `[canager-ui-preview-mock]` to the console; to check a build, run
  `pnpm build` and then `grep -r canager-ui-preview-mock dist`, which
  finds nothing.
- The mock is typed against `src/lib/types.ts` and checked by
  `pnpm typecheck` like the rest of `src/`; `src/dev/mockBackend.test.ts`
  (run by `pnpm test`) checks that it answers every command `api.ts` sends
  and that an operation runs the way the real backend reports one.
- The logos are not mocked: the avatars draw from the logo pack built
  into the app (`src/assets/tool-icons/`, read by `src/lib/toolIcons.ts`,
  which asks the backend for nothing), so a tool or a source the pack has
  a logo for shows it here as it does in the app. A cask's app icon still
  comes first: iTerm2 and Visual Studio Code show the generated one
  described below.

The files: `mockTauri.ts`, `mockTauriEvent.ts` and `mockTauriWindow.ts`
(the stand-in modules; the second listens to nothing, and the third
badges nothing), `mockBackend.ts` (the commands),
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
  two models, one with a newer build from a third-party registry.
- **Tools with their own installer**: Claude Code (updates itself, and has
  an update), rustup (an update that cannot be cancelled once it starts),
  Antigravity CLI (a newer version it can only install itself) and Grok
  Build (an update, and a notice that it is not on the PATH).
- **Unknown page**: five programs no source accounts for -- two plain
  files, two links an installer with administrator rights put there (one
  into an app), and a broken link to an app that was deleted.

## What it does

- A refresh takes about a second; the first one runs at startup, as in the
  app, so the page header and the Overview say "Checking…" (the other
  pages "Loading…") for a moment. Check again in the header runs one at
  any time.
- Update and Uninstall show the preview the real adapter would build
  (warnings, what would break, password and "can't cancel" notices, and
  the command behind "Show the command"), then run for about five seconds with the same event sequence
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
  refresh.

## URL switches

Combine them with `&`, for example
<http://localhost:1430/?state=offline&lang=zh-CN&page=updates>. An unknown
value falls back to the default and logs a warning in the console.

| Switch | Values | What you get |
|---|---|---|
| `state` | `full` (default) | The Mac above. |
| | `loading` | The first refresh and the Unknown page's scan never finish. |
| | `error` | Loading what is installed fails. The failure screen appears once the app gives up retrying, about 7 seconds later, and only while the tab is visible. |
| | `refresh-error` | The same failure screen at once: the first refresh fails. |
| | `empty` | No source is set up on this Mac. |
| | `nothing` | Homebrew is set up, with nothing installed. |
| | `uptodate` | Every source answered and nothing needs updating. |
| | `hidden` | The only updates are the skipped and never-remind-me ones. |
| | `stale` | The last refresh could not finish for two sources. |
| | `notices` | Every source notice with a look of its own: Homebrew still downloading its catalogue (its operations wait for it first, and its uninstall previews are refused), npm read-only with an unverified version, Ollama not running (Open Ollama starts it), another `claude` first on the PATH, Grok Build's launcher left without its program. |
| | `offline` | No registry answered: Homebrew's catalogue could not be downloaded, and every other lookup is "could not check". |
| `lang` | `system` (default), `en`, `zh-CN` | Settings' language at startup. |
| `tech` | `1` | Show technical details on at startup. |
| `page` | `overview` (default), `updates`, `installed`, `unknown`, `settings` | The page the window opens on. |
| `outcome` | `succeeded` (default), `failed`, `cancelled`, `unconfirmed`, `attention`, `canager` | How every operation ends. Only `succeeded` changes anything. |
| `scan` | `found` (default), `stopped`, `empty`, `error` | What the Unknown page's scan returns. |
