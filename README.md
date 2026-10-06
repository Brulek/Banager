# Banager

**A Mac app for everything you installed from the terminal and then forgot about.**

Banager (formerly Canager) is short for bao-manager: bao is the pinyin of 包, Chinese for
"package". It also sounds like banana.

If you've followed a few tutorials, you probably have Homebrew formulae, a couple of global npm
packages, some Python tools, a Rust binary and an Ollama model or two scattered across your Mac.
Each was installed with a different command. Updating them needs a different command again.
Removing them needs a third. Most people never do either, and the tools quietly rot.

Banager puts all of it in one window: what you have, what has an update, and a button for each.

> **Status: pre-release.** The core and the UI work and are covered by 1697 Rust tests (plus 4 more
> that touch a real Homebrew, the real Trash or AppKit and only run with `--ignored`) and 2510
> front-end tests, but there is no downloadable build yet — v0.1 is being prepared. Nothing here is
> ready to rely on.

<!-- A screenshot belongs here before the first release. -->

## What it manages

| Source | Reads | Installs / updates / removes |
|---|---|---|
| Homebrew — formulae and casks | yes | yes |
| npm — global packages | yes | yes, when the prefix is yours to write |
| pipx | yes | yes |
| uv — tools | yes | yes, but no uninstall while `UV_TOOL_DIR` is set in Banager's environment: removing the last tool, uv would then also delete the folder above that one, with every file in it, when that folder holds no other folder; the row says so |
| pip | yes | **no** — Banager will not drive pip's installer; it points you at pipx or uv |
| cargo | yes | yes, with a warning that it compiles locally |
| Ollama — models | yes | yes |
| Claude Code — the native install, via its own installer | yes | updates yes; install no (the installer is Anthropic's, and Banager never runs it); uninstall yes — its program files, download cache and launcher go to the Trash, and your settings and history stay |
| rustup — the Rust toolchain manager, via its own installer | yes | updates yes (`rustup self update`); install no (the installer is rust-lang's, and Banager never runs it); uninstall yes (`rustup self uninstall -y`), offered only when Rust is in its standard folders (`~/.cargo`, `~/.rustup`) and previewed with everything it removes — permanently, not to the Trash: every toolchain by name, the whole Cargo folder with its settings and saved login, and the programs in its `bin` folder, named where known — by the name the Installed page gives a program `cargo install` recorded (`jj-cli`, not `jj`), else by its file name. Neither can be cancelled once it is running, and the preview says so |
| Antigravity CLI (`agy`) — Google's terminal agent, via its own installer | yes | updates **no** — it installs its updates itself in the background and its own `agy update` is undocumented, so a newer version is listed under "N more can't be updated here", marked "Updates when run", whose ⓘ says to open the tool once — by typing `agy` in Terminal, where typing it runs this copy (Banager looks the newer version up on Apple silicon only: on an Intel Mac the row reads "Can't check" and nothing is sent); install no (the installer is Google's, and Banager never runs it); uninstall yes — the `agy` program, and any `agy.<time>.old` backup its updater left beside it, go to the Trash; its conversations, history and working files in `~/.gemini/antigravity-cli` stay, and so do its staging folder in `~/.cache` and the `PATH` lines its installer added |
| Grok Build (`grok`) — xAI's terminal agent, via its own installer | yes | updates yes (`grok update`, offered when grok's own `update --check --json` says a newer version exists; how `grok update` behaves when nothing can answer a prompt is yet to be recorded on CI); install no (the installer is xAI's, and Banager never runs it); uninstall yes — its downloaded versions, its bundled agents and shell completions, any fallback links its installer made in `~/.local/bin`, and the two links in its `bin` folder go to the Trash (the folder itself, which its installer put on your `PATH`, stays); `~/.grok`'s settings, login, sessions and memory stay |
| Codex — OpenAI's terminal agent, via its own installer | yes, listed only: its version is read from the folder name its installer links to, and no command runs for it, not even a version check; npm's `@openai/codex` and Homebrew's `codex` cask stay those sources' own rows | updates **no** — its newest version is not looked up, so the Updates page lists nothing for it (the row says Codex can update itself when its installer's auto-update file names the release in use, and otherwise that its updates aren't checked); install no (the installer is OpenAI's, and Banager never runs it); uninstall no — the row says "Manual uninstall" |
| opencode — via its own installer | yes, listed only: its version is not known, since its installer leaves no file that names it and no command runs for it | updates **no** — its newest version is not looked up, so the Updates page lists nothing for it (the row says it updates itself, by default); install no (Banager never runs its installer); uninstall no — the row says "Manual uninstall" |

Programs that none of these sources installed — a tool's own installer dropped a binary into
`~/.local/bin`, an app put a helper into `/usr/local/bin`, a link whose target is gone — are
listed, read-only, on the **Other Programs** page, the last row under the sidebar's Sources.
Banager never runs, moves or deletes anything there; `docs/what-we-run.md` says exactly what it
reads. A program a source installed but reported no path for is listed there too (uv's own `uvx`,
for one): the gap is the source's, and the page says what it sees. Cargo reports one program per
crate — the one named after the crate, else the first its record lists — so the other programs of
a crate that installs several (`cargo-binstall`'s `detect-targets`) stay on that page until it can
report them all. The scan never reads the places macOS asks you about before an app reads them —
Desktop, Documents, Downloads, Pictures, Movies, Music, iCloud Drive and other cloud folders, other
apps' data — nor other disks under `/Volumes`, even by way of a link. A folder to scan that is in one
is not read, and a line under the list says how many were left out (with Show technical details on,
its ⓘ names them); a program that is a link into one is listed by its own name, marked **Points into
a protected place**, and the link is not followed.

## What it tells you about each tool

- The **Show** menu in the toolbar of the Updates and Installed pages narrows the list to **AI Tools**:
  Claude Code, Codex, Gemini CLI, Ollama and the other AI tools in a table built into Banager,
  whichever source installed them. On the Installed page it also offers **Installed More Than Once**, **Not Found
  in Terminal**, **Disabled or Deprecated by Homebrew** and **Other Versions Kept** (Homebrew formulae with
  another version kept beside the one in use), each of these four with how many tools it shows.
- A tool's details say, under **In Terminal**, what typing each of its commands runs — this copy,
  another copy or program, or nothing, when the command sits in a folder Terminal doesn't search —
  judged from the Terminal settings read when Banager opened. A tool another source installed too says
  **Installed twice**, and where Banager can tell, its details say which copy Terminal runs.
- A package Homebrew has disabled or deprecated says so, with what that means and, where Homebrew
  gives one, the name it suggests instead. A newer version of a disabled one, which `brew outdated`
  still lists, is held back on the Updates page as **Disabled**, with no Update button. The details also list a formula's other installed versions
  and Homebrew's own notes, in English, folded.
- After each check Banager measures, read-only, how much disk the tools take: a tool's details show its
  size once measured, the Installed page can be sorted **By Size**, and sorted by source a heading says
  what that source's measured tools take in all.
- The Installed page can also be sorted **By Date Installed**, newest first. Only Homebrew says when a
  tool was installed, so the tools from every other source come after Homebrew's, by name, with "—".
- While the first check since launch is still looking for updates, the Installed page already lists
  what it found; uninstalling waits until that check is done.
- On the Updates page, an update to a new major version is marked **Major update**, unless its row
  already says the tool updates itself or that Terminal runs another copy (**Not used in Terminal**).
  **Update History**, under the updates still to install (at the top when there are none), lists the
  updates of the last 30 days: those that succeeded, and those that failed (with the cause where one is known,
  such as **Couldn't update: Connection failed**) or whose result didn't add up, each saying what happened in its own
  words (for example, **Didn't update: same version** where the version read after the update was the one before). A failed one or one whose
  result didn't add up is listed only while the last check still offers that tool an update. Cancelled updates
  and uninstalls are not listed. The list is kept across restarts in `history.json`, until you press Clear History.
- An uninstall's preview lists what stays after it — an AI tool's settings and data folders where the
  table names them, Ollama's models — with how much each takes where it could be measured, and Copy Path; nothing in it
  deletes them.
- A Homebrew package another source runs on can't be uninstalled while that source has tools of its own:
  its preview lists the source under **Software that uses it**, after Homebrew's own dependents — "npm with
  its 4 tools" under the `node@22` npm runs on, "2 tools installed with pipx" under the `python@3.13` their
  environments use, Ollama with its models under `ollama` — keeps Uninstall off and says which tools to
  uninstall first; a batch leaves the package out with the same words. The preview finds them by following
  links, read-only, and runs no command (`docs/what-we-run.md`, "What runs on a Homebrew package"). npm's own
  `npm` row offers no Uninstall.
- On the Installed page, each row whose **Uninstall…** is available has a checkbox. Tick up to 20 and
  **Uninstall Selected** opens one preview of them all: what each one removes, in the order they will
  run; the ones it leaves to their own row, each with why — such as an uninstall that can't be cancelled
  once it starts, one that deletes files permanently, one whose steps can't be known in advance, one still
  used by software you didn't select; what stays afterwards, each path once; and the exact commands
  under **Show Commands** (**Show Commands and Paths** when an uninstall moves files to the Trash).
  Confirming queues each tool's own uninstall, the same one its row runs — a tool whose dependent in
  the batch could not be started is left for later; on one source they start in the order listed.
- When a Homebrew update or uninstall stopped because it needed your Mac's password, its log shows the
  command to copy and run in Terminal, where you can type it.
- With **Check for updates** set to Daily or Weekly, Settings says about when the next check is due,
  while Banager is running. Its **Copy
  Diagnostic Info** — Help's item of that name takes you there — copies a short text about Banager,
  this Mac and its sources to paste to whoever is helping you; it lists your tools only when you tick
  the box, and writes your home folder as `~`.
- **Check Tool Setup…**, in the Help menu and as a button on the Overview and in Settings, says in short lines
  how this Mac's tools are set up — whether Terminal's login settings were read, each source that
  isn't answering, how many tools Terminal can't find or has twice, what Homebrew disabled or keeps
  other versions of, and the disk measured. Each line that counts tools, and each line about a source
  with a problem, has a **Show in Installed** (or **Show in Other Programs**) button that opens that list or that source. It has no score; it is built from what the last check found, and runs nothing.

Banager checks every source when it opens, after each operation, and whenever you press **Check
again** in the header of the Overview, Updates and Installed pages, which also says how long ago the
last check finished, or choose **Check Again** (⌘R) in the menu bar's View menu, on any page; while
a check runs, neither starts another. The **Check Again** on the page Banager shows when it couldn't
load installed tools, and the one on the notice of a Homebrew index Banager couldn't update, run the
same check, and a Homebrew index update left running in the
background starts one on its own when it ends (`ipc::refresh_on_background_change`,
`src-tauri/src/lib.rs:70-73`). Checks run Homebrew's own `brew update`, which updates Homebrew and
its index, and when Homebrew has moved a package you have between a formula and a cask, or renamed
one, can install, move or uninstall Homebrew packages by itself (`docs/what-we-run.md`,
"Homebrew"). After one that succeeded, checks skip it for six hours on the clock (time the Mac spends
asleep counts, and a clock set back to before it ended counts as the six hours gone); after one that
failed, the next check runs it again — or, when a check had stopped waiting for it, the check after
the one its end sets off. With **Check for updates** set to Daily in Settings — it is set to
Manually until you choose
otherwise — Banager also runs the same check once a day (or once a week, set to Weekly) while it is running, installs none of the updates
it finds, and checks nothing after you quit. A daily check in which every source failed — a
Homebrew whose index couldn't be updated counting as failed — doesn't count: the next runs 15
minutes later, and each more that fails in a row doubles the wait (30, 60, 120, 240 minutes) up to
six hours. A check of yours, or a daily one in which not every source failed, counts, and starts
the waits over (`docs/what-we-run.md`, "The daily check"). Turn on **Notify me when there are
updates** under that popup as well, and a daily check that finds an update you haven't been
shown, while another app is in front, not Banager, posts a notification saying how many tools can
be updated. A click on it brings Banager
to the front, and if Banager's window is closed or minimized into the Dock and hasn't been in front
since the notification, the window comes back on the Updates page. Banager isn't told of the click
itself, only that it has come to the front, so until the window has been in front again, anything
else that brings Banager to the front with the window closed or minimized — ⌘-Tab, its Dock icon —
does the same. Closing the window doesn't stop an operation; turn on **Notify me when operations
finish** (off until you do) and a run of updates or uninstalls that finishes while another app is in
front posts one notification saying how it went, such as "Updated 3 tools" — never one you watched
finish in the window. One that finishes with the window closed while Banager is still in front is
posted once you switch to another app (`docs/what-we-run.md`, "The notification when operations finish"). A row's … menu on the Updates page has *Remind Me in 30 Days* between *Skip This Version* and
*Don't Remind Me About This Tool*: it hides every version of that tool, not only the one offered, from
the list, its counts and the Dock badge for 30 days, after which the tool is listed again (not
notified again); Settings lists it until then, with a button to undo it. The Other Programs page's header has *Scan Again* in its place, with how long ago
that page last scanned: it re-runs only that page's scan of your bin folders, against the sources'
last known state — it does not refresh the sources. Settings' header has neither.

The menu bar's View menu opens the sidebar's pages, as Finder's and Mail's open theirs: Overview
(⌘1), Updates (⌘2), Installed (⌘3 — on everything installed, as the sidebar's Installed opens it)
and Other Programs (⌘4). Below them are Check Again (⌘R) and Search (⌘F), which opens the Installed
page with its search box focused, and Settings… (⌘,) is in the Banager menu. With the window closed
or minimized, each of these brings it back first. **Keyboard Shortcuts** in the Help menu lists the
keys Banager answers to, in three groups: the window's (these, Close Window and Quit), the lists' (↑ ↓
and the keys that page or jump to an end, Space to tick a row, Return and Escape for Installed's
details, Tab) and the dialogs' (Return, Escape). **Common Questions**, above it, answers ten questions
in a few plain sentences each — a command Terminal can't find, why some tools can't be updated here,
"Installed twice", the Mac password, what an uninstall leaves, what Banager itself changes, Other
Programs, sizes, major updates and the automatic check — with a button named for the page it opens
(*Show in Installed*, *Show in Updates*…) where the answer can be acted on.

The first time Banager opens, a welcome sheet says in three short points what it does: it lists the
tools you use in Terminal, such as Claude Code, Codex and Gemini CLI, in one place, with Sources in the
sidebar showing how each was installed once the first check finishes (Homebrew, npm, a tool's own installer
and so on); an update or uninstall shows what it will do and starts only when you confirm; and Banager itself
doesn't change your Terminal settings files, collects no usage data and needs no account. (The one exception to
not changing those files is rustup's own uninstall, which removes the line it added to them; its preview says so.) The first check runs behind it. However
you close it — **Get Started**, Return, Escape or a click beside it — its settings file records that
it was shown, and it doesn't open on its own again; **Welcome to Banager** in the Help menu shows it
again at any time.

Closing the window — its red button, or Close Window (⌘W) in the menu bar's File menu — leaves
Banager running, and an operation under way carries on; its icon in the Dock brings the window back
as you left it — or on the Updates page after a notification, as above — without a new check. Quit
Banager (⌘Q) quits it. While an update or uninstall is still queued or running, though, quitting —
⌘Q, Quit in the Dock icon's menu, or logging out, restarting or shutting down — first brings the
window back and asks: *2 operations haven't finished*, since quitting now stops them and a tool that
is being updated can be left half-updated, and it says so of one that has started and can't be
cancelled, such as rustup's self update. *Keep Waiting* leaves Banager running, and *Quit*
quits. Banager answers macOS at once, so a logout, restart or shutdown is called off rather than
kept waiting, and after *Quit* you start it again (`src-tauri/src/quit.rs`). Should the window
be unable to ask — it stopped working, or doesn't show the question within 2 seconds — Banager quits
rather than hold the quit with nobody there to answer. Force Quit still quits at once.

Adding a source is one Rust file implementing one trait, plus a TOML metadata file.

## What makes it safe to point at your machine

This app runs package managers on your behalf, so the boundary matters more than the features:

- **There is no shell.** Every command is built as an argument vector and handed to the OS
  directly. Nothing is ever concatenated into a string a shell would interpret.
- **The window cannot ask for a command.** The UI sends an operation kind and a single-use,
  expiring identifier for a plan the Rust side built itself. There is no general "run this" path,
  so a compromised web view cannot invent one.
- **You see the exact command before it runs.** Every update and uninstall lets you see the exact
  command before it runs, with the variables Banager sets for it — one press on "Show Command"
  in its confirmation, or open from the start with Settings' "Show technical details" on — and
  says whether it may ask for your password; an uninstall that runs no command lists instead the
  exact paths it will move to the Trash. An uninstall also says what it will affect. An update says
  so only when a Homebrew `brew.env` file turns Homebrew's clean-up back on, since Homebrew then
  deletes, after every update, the older versions of that software and of any it updates along
  with it, and stray old downloads, and, whenever
  its periodic clean-up is due, those of all Homebrew software — and, when the file turns its
  autoremove back on too, that periodic clean-up also uninstalls the packages that were installed
  only as dependencies and that nothing needs anymore.
- **Nothing is deleted quietly.** An uninstall that would break other packages says which ones,
  in your language. Banager runs Homebrew with its autoremove off, so a Homebrew uninstall does
  not also uninstall the other packages that were installed only as dependencies and that nothing
  needs anymore; when a `brew.env` file turns autoremove back on, the preview says Homebrew will.
- **A tool with no uninstall command goes to the Trash, not away.** Claude Code's makers document
  its removal as a list of paths. Banager moves those paths, plus its installer's download cache,
  to the Trash itself, with the call Finder uses, so until you empty the Trash you can drag them
  back — and Finder's Put Back will likely work too; the preview lists each path it will move and
  each one it keeps (your settings and history, in `~/.claude` and `~/.claude.json`). Antigravity
  CLI and Grok Build publish no removal instructions at all, so their lists are Banager's own
  reading of how each was installed, and their paths go to the Trash the same way. Moving files to
  the Trash is the only change Banager makes to a file itself besides saving its own settings, its
  history of the updates and uninstalls it ran (`history.json`), and its window's size and position;
  `docs/what-we-run.md` says how, and names every path each list moves or keeps and where it
  comes from.
- **Only the paths you were shown are moved.** Each path must be inside your home folder — never
  directly in it or in a folder other apps share, such as `~/.local` or `~/Library`, and with no
  folder that is a link between where your home folder really is and the path (the home folder
  itself may be reached through a link) — yours, what that tool's uninstall list describes, and
  clear of what it keeps. Banager remembers what each path was when you saw the preview; when you
  confirm, and again right before each path moves, it checks everything once more, and if anything
  differs it stops before moving that path, and the operation log lists anything it had already
  moved.

## What it deliberately does not do yet

Being honest about this is part of the point:

- **No catalogue or search for new tools, and no way to install something new.** The Installed
  page searches the tools you already have by name, command or description; you cannot yet
  discover or add new things through Banager.
- **macOS only.** The core crate is portable and the architecture is cross-platform, but
  everything below the trait boundary assumes Unix today, and only macOS is tested. Windows and
  Linux are roadmap, not "nearly working".

## Building from source

Needs Rust (stable), Node with pnpm, and Xcode's command line tools.

```bash
pnpm install
pnpm tauri dev
```

To look at the UI in an ordinary browser instead, with a mock backend in place of Tauri (for
screenshots; development only, never in a build), run `pnpm dev:mock` and open
<http://localhost:1430/> — [docs/ui-preview.md](docs/ui-preview.md) has the rest. `pnpm tauri:mock`
puts the same mock front end in the app's real window, title bar and all, and Banager runs no
command for it; the same page says why.

Tests — all five must pass before anything is committed:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm test
pnpm typecheck
```

`pnpm typecheck` runs three TypeScript programs. `tsconfig.json` checks the production code under `src/`
with no ambient Node types, so `process`, `Buffer` or a `node:` import in code that will run inside the
WebView is a type error; `tsconfig.test.json` checks the vitest files with `@types/node`, which
`src/i18n/completeness.test.ts` and `src/i18n/no-literal-strings.test.ts` need to read the source tree
through `node:fs`; `tsconfig.node.json` checks `vite.config.ts` (`--composite false`, so it leaves no
`.tsbuildinfo` behind). `pnpm build` runs the same three programs before `vite build`.

`cargo test --workspace` has four `#[ignore]`d tests, all skipped by a plain `cargo test`. Two are
in `crates/banager-core/tests/brew_live.rs`: one only reads the real Homebrew on the machine
running it, the other installs and removes the `hello` formula. The third, in
`crates/banager-core/tests/standalone_uninstall_test.rs`, moves five throwaway items it creates
(named `banager-trash-smoke-…`) into the real Trash of the Mac running it and leaves them there.
The fourth, in `crates/banager-core/src/icon/real.rs`, has AppKit draw Calculator's icon and
only reads. The two that change the machine refuse to touch anything without `BANAGER_LIVE=1`. CI
runs the first three; run them yourself with:

```bash
BANAGER_LIVE=1 cargo test -p banager-core --test brew_live -- --ignored
BANAGER_LIVE=1 cargo test -p banager-core --test standalone_uninstall_test -- --ignored
cargo test -p banager-core --lib icon::real -- --ignored
```

## Language

The interface supports English, Simplified Chinese and Traditional Chinese in Taiwan usage. It follows
the system language unless overridden in Settings. Every label, heading, button and
message frame in the window goes through i18n, and a test keeps the three locales in step — a sentence
a Chinese user cannot read is treated as a bug. The menu bar follows the window's language, Settings'
choice included. Its words are Rust's (`src-tauri/src/menu.rs`), macOS's own for the items every Mac
app has, and a test there keeps its three languages in step too.

Rust's refusals are translated too, not just the frames around them. A plan built against a source
that is read-only, unavailable or gone, an operation Banager can't prepare (a name it won't pass to
a tool, a program that has gone missing, a path on an uninstall list that is outside your home
folder, in a folder other apps share, missing, not yours or not what that list describes), a
preview that has expired or already been used, a settings change it couldn't save, an operation
Banager itself couldn't carry out (the program was removed between the check and the run, say, or a
path changed between the preview and the click), Banager's own remarks in the operation log
(waiting for Homebrew to finish updating, a stream it could no longer read, each item it moved to
the Trash) and its verdicts on a result (the command said it worked but the package isn't there)
each arrive as a small structured payload the front end renders in the user's language.

Three kinds of text are shown as-is:

- **Another program's own words.** Every line `brew` or `npm` prints in the operation log, and
  the last lines of its stderr when an operation fails; the reason macOS gives when it can't start
  a tool, whether Banager is preparing an operation or running one, can't save Banager's settings
  for a cause Banager doesn't recognise, or refuses to move an item to the Trash.
  That is another program's text, and there is no way to translate it. Outside the log it is quoted
  inside a sentence in your language that says what happened.
- **The app framework's own error**, in the one case where the window can't get an answer from the
  rest of Banager at all while loading the list, or refreshing it before any check has found
  anything — its own text is shown untranslated, next to the Check Again button (after that, the
  header says only "Couldn't check"). Short of that, Banager itself never fails a refresh as a whole, but not
  every source with trouble gets a notice of its own. A source that has gone unavailable to Banager (not
  running, unreachable, or refusing to run as root) is reported in your language, through its own
  notice. One that isn't responding also says when it last answered, if Banager has noted that since
  it opened ("…shown as they were when it last responded at 9:12 AM today"), and gives no time otherwise.
  A source that Banager could still reach, but whose software list or update check failed,
  gets no notice of its own: the "Some checks didn't finish" banner names it, and says it didn't
  finish checking this time.
- **A number of technical details that are still Banager's own**, which appear in English inside an
  otherwise translated sentence. This is a known gap, not a design choice, and it is not just the
  one case the wording used to name: with "Show technical details" turned on, whenever a package
  can't be checked for updates Banager's own explanation of why is shown as plain English rather
  than translated — with the switch off you see only a short generic sentence instead. There are
  more than a dozen such explanations: a generic one like "npm outdated -g exited with code 1" (or
  "... did not finish", or the tool's own first line of stderr) from any lookup that runs a
  command, Grok Build's own update check among them (which has a few more of its own: a check
  Banager could not run, an answer that is not grok's JSON, or an error grok itself reported); from
  the six lookups Banager makes over HTTP instead of a command line, that request's own wording —
  pipx's PyPI lookup ("PyPI request failed: ...", "PyPI returned status 503", "could not parse PyPI
  response: ..."), Cargo's equivalent for crates.io, Ollama's for its own registry, Claude Code's
  for its release channel, rustup's for its release file, and Antigravity CLI's for its manifest
  (or, on an Intel Mac, why it made no request); and the two about the installed version: "cannot
  read the installed version now", from the code Claude Code, Antigravity CLI, Grok Build and
  rustup share, and "cannot compare the installed version ... with the published ...", from Claude
  Code, Antigravity CLI and rustup only, since Grok Build's check takes grok's own answer and
  compares no versions. They should all become structured payloads like the refusals above, and
  until they do, what a Chinese user sees there with the switch on is in English.

## Design notes

The design and the reasoning behind it live in [`docs/superpowers/`](docs/superpowers/) — the
spec, the implementation plans, and the review findings that changed them. They are working
documents rather than polished writing, but they record why things are the way they are.

## Logos

The logos Banager shows for tools and sources are trademarks of their owners, shown only to
identify the tool or source each stands for; a tool may show its maker's logo in place of one of
its own. Those drawn in white or near-black on their brand's colour come from
[Simple Icons](https://simpleicons.org/), which is released under CC0 — though, as Simple Icons
says, not every icon in it is: an icon under a license of its own has that license named in
Simple Icons' data. Such a logo keeps its license. Banager ships it unmodified, its path exactly
as Simple Icons has it, and credits it in Settings, under About → Icon credits, with its license
and the addresses of the license's text and of the page Simple Icons took the logo from. The
logos not from Simple Icons are the GitHub avatar of the organization or account behind the
project, or behind its maker. All of them are built into the app, from `src/assets/tool-icons/`,
and showing one makes no network request. `pnpm icons:build` regenerates that folder from
`scripts/tool-icons/mapping.json`, taking Simple Icons' logos from the pinned `simple-icons`
package and downloading the avatars from GitHub. It fails if the mapping names a logo under a
license Banager does not ship — it ships CC0-1.0, MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause,
ISC, CC-BY and CC-BY-SA, and no other: nothing noncommercial, no-derivatives, GPL-family or
custom — and if the folder comes to more than 5 MB, the limit a test holds it to as well. Neither
the app nor the tests run it.

## Descriptions

Under a tool's name, its row says in one line what the tool is: the description the tool's source
gives it, such as Homebrew's for a formula or a cask; where the source gives none, what kind of
thing that source lists ("npm package"); and for a tool with its own installer, a line of Banager's
own, in all three languages. npm, pip, pipx, uv and Cargo give none, so in English a row for an npm, PyPI
or crates.io package says a line in English instead wherever Banager has one: 640 of them,
each rewritten, shorter, from the description the package's own registry gives it. In Chinese, a
row says a line in Chinese instead wherever Banager has one: 3,390 in each Chinese language, for Homebrew's
formulae and casks and for npm, PyPI and crates.io packages, each translated from the description
the tool's own source gives it. The three tables are built into the app, in
`src/assets/tool-descriptions/en.json`, `zh-CN.json` and `zh-Hant.json`. A table loads when needed
for rows or search; Traditional Chinese also loads its Simplified Chinese and English fallbacks.
Showing a description makes no network request. A tool's details show that
one line too, without its source's own description beside it; a tool Banager has no line for in the
window's language reads as it did before.

## License

Not chosen yet. Until a license file is added this repository is "all rights reserved" by
default, so please don't build on it yet — and I can't accept contributions until it's settled.

---

## 中文

**把你在终端里装过、然后忘掉的东西管起来。**

Banager（原名 Canager）：bao-manager，bao 是“包”的拼音；读起来也像 banana。

跟着几篇教程走下来，Mac 上多半散落着一些 Homebrew 软件、几个全局 npm 包、几个 Python 工具、
一个 Rust 编译出来的命令，还有一两个 Ollama 模型。每样都是用不同的命令装的，更新要换一条命令，
卸载又要换一条。大多数人两件都不做，这些东西就在那儿慢慢烂掉。

Banager 把它们放进同一个窗口：装了什么、哪个有更新、每个都配一个按钮。

每次更新和卸载，都能在它运行之前看到确切的命令，连同 Banager 为它设的环境变量：在确认框里点「查看命令」，或者在设置里打开「显示技术细节」，
让它一开始就展开；可能要输入 Mac 密码的，确认框也会先说。不运行命令的卸载，改为列出它要移到废纸篓的每一条路径。

**目前处于发布前阶段**，核心与界面已经可用、有 1697 个 Rust 测试（另有 4 个要连着真实的
Homebrew、真实的废纸篓或 AppKit 才跑，平时是跳过的）和 2510 个前端测试，但还没有可下载的版本，v0.1 正在
准备。现在还不适合依赖它。

界面支持英文、简体中文和台湾用语的繁体中文，跟随系统语言，也可以在设置中选择。
窗口里所有标签、标题、按钮和提示框都走 i18n，三种语言由测试保证同步——
中文用户读不懂的句子算 bug。菜单栏跟着窗口的语言走，设置里选的语言也算。它的文字写在 Rust 里
（`src-tauri/src/menu.rs`），每个 Mac 应用都有的菜单项用 macOS 自己的叫法，那里也有测试保证三种语言同步。

Rust 侧返回的拒绝理由也会翻译，不只是外面那层框。操作所针对的来源只读、连不上或已不存在，操作无法
准备（某个名字 Banager 不肯交给工具、某个程序不见了、卸载清单上的某条路径不在你的个人文件夹里、
放在其它应用共用的文件夹里、不存在、不属于你或者和说明写的不一样），预览已过期或已用过，设置没能保存，
操作因为 Banager 自己这边的原因没能执行（比如程序在检查之后、运行之前被删掉了，或者某条路径在预览之后、
点击之前变了），Banager 自己在操作日志里说的话（等待 Homebrew 更新完毕、某个输出流读不下去了、
把哪一项移到了废纸篓），以及它对结果的判断（命令说成功了，但那个包并不在），
都以一个结构化的小数据传到前端，用你选的语言显示。

有三类文字会原样显示：

- **别的程序自己的话。** brew、npm 在操作日志里打印的每一行，操作失败时它 stderr 的最后
  几行；以及 macOS 无法启动某个工具（不论 Banager 是在准备操作还是在执行操作）、
  或因为 Banager 不认识的原因无法保存设置、或拒绝把某一项移到废纸篓时给出的原因。那是另一个程序自己的文字，没法翻译。日志之外，它会被引用在一句用你的语言说明发生了什么的话里。
- **应用框架自己的报错**，只出现在一种情况：加载列表时，或在还没有任何检查结果时刷新列表，窗口完全联系不上
  Banager 的其余部分——这时它自己的文字会原样显示在“重新检查”按钮旁边（有了检查结果之后，页头只说
  “无法完成检查”）。除此之外，Banager 自己从不会让整次刷新失败，但不是每个出问题的
  来源都有自己的提示。一个来源如果对 Banager 而言已经不可用了（没在运行、连不上、或者因为以 root 身份
  运行而被拒绝），会用你的语言、通过它自己的提示告诉你；没有响应的来源，如果 Banager 打开后记下了它上次响应的
  时间，提示还会说是什么时候（「显示的是它今天09:12响应时的结果」），否则不说时间；一个来源如果本身能联系上，只是软件列表或更新
  检查失败了，就没有自己的提示——只会由“部分检查未完成”横幅点名，说它这次没检查完。
- **还有几处技术细节仍属于 Banager 自己**，会以英文出现在一句已翻译的话里。这是已知的缺口，不是有意
  为之，而且不只是以前说的那一处：打开“显示技术细节”后，只要某个包没法检查更新，Banager 自己给出的
  原因就会原样显示成英文，而不是翻译过的句子——关掉开关时，看到的只是一句简短的通用提示。这样的原因
  有十几处：一类是像“npm outdated -g exited with code 1”这样的通用提示（也可能是“... did not
  finish”，或者工具自己 stderr 的第一行），出自任何要跑命令去检查更新的来源，Grok Build 用它自己的命令检查更新也在其中
  （它还另有几句：Banager 没能运行这个检查、回答不是 grok 该给的 JSON，或者 grok 自己报了错）；另一类来自另外六个改用 HTTP 直接查询的来源——
  pipx 查 PyPI、Cargo 查 crates.io、Ollama 查它自己的软件源、Claude Code 查它的发布通道、rustup 查它的发布文件、
  Antigravity CLI 查它的版本清单（在 Intel Mac 上则是它为什么没发请求）——各自请求失败、返回状态异常、
  解析失败时的原文提示；还有两句关于已安装版本的原文提示：读不到已安装版本，出自 Claude Code、
  Antigravity CLI、Grok Build 与 rustup 共用的代码；已安装版本与发布版本无法比较，只出自 Claude Code、
  Antigravity CLI 与 rustup，因为 Grok Build 的检查直接采信 grok 自己的回答，不比较版本。
  这些都应该像上面的拒绝理由一样改成结构化数据，在那之前，中文用户在开关打开时看到的，就是英文。

每个软件名下那一行简介，默认是它所在来源自己给的说明（比如 Homebrew 给 formula 和 cask 写的那句英文）；
来源没给的，写这个来源列出的是什么（“npm软件包”）；自带安装器的工具，是 Banager 自己写的一句，
英文、简体中文和繁体中文都有。npm、pip、pipx、uv 和 Cargo 都不给说明，所以英文界面里，npm、PyPI、crates.io 上的包只要
Banager 有它的英文说明，就改显示这一句：640 条，每条都由该包在 npm、PyPI 或 crates.io 上自己的说明改写而来，
更简短。中文界面里，只要 Banager 有这个软件的中文说明，就改显示中文：简体中文和繁体中文各 3,390 条，涵盖 Homebrew 的
formula 与 cask，以及 npm、PyPI、crates.io 上的包，每条都译自该软件所在来源自己的说明。这些说明内置在应用里
（`src/assets/tool-descriptions/en.json`、`zh-CN.json` 与 `zh-Hant.json`），列表或搜索需要时才读取，
繁体中文还会读取简体中文和英文作为缺少条目时的备用。显示时不发任何网络请求。软件详情里也只显示这一行，不再附上来源的原文；当前语言下没有
说明的软件，照旧显示原来那一行。

“已安装”页可以按名称、命令或说明搜索已有工具。尚未支持：查找新工具的软件目录与搜索、安装新东西、macOS 以外的平台。

它还会告诉你每个工具的这些事：

- “更新”和“已安装”两页工具栏里的“显示”菜单，可以只列出“AI工具”：Claude Code、Codex、Gemini CLI、
  Ollama 等 Banager 内置表格里的 AI 工具，不管是哪个来源装的。在“已安装”页，它还有“装了不止一份”“终端里找不到”
  “Homebrew已停用或弃用”和“保留了其他版本”（在当前使用的版本之外还留着其他版本的 Homebrew formula），这四项都会写出各有几个。
- 工具详情里的“在终端里输入”，说明输入它的每条命令会运行什么：这一份、另一份或另一个同名程序，或者什么都
  运行不了（命令所在的文件夹不在终端的搜索路径里）——按打开 Banager 时读到的终端设置判断。别的来源也装了
  一份的工具会标“装了两份”，能判断时，详情里说终端运行的是哪一份。
- Homebrew 停用或弃用了的软件会标出来，并说明这意味着什么；Homebrew 给了建议时，也写出建议改用哪个。已停用的软件
  `brew outdated` 仍会列出新版本，更新页把它标为“已停用”，不给“更新”按钮。详情里还列出 formula
  装着的其他版本，以及 Homebrew 自己的英文说明（默认收起）。
- 每次检查后，Banager 以只读方式计算各工具占用的磁盘空间：算好后详情里能看到，“已安装”页可以“按大小”排序，
  按来源排序时，来源标题后写着它算出大小的工具一共占多少。
- “已安装”页也可以“按安装日期”排序，最近装的在前。只有 Homebrew 记录安装日期，其他来源的工具排在 Homebrew 的
  后面，按名称排列，日期处显示“—”。
- 打开 Banager 后的第一次检查还在查更新时，“已安装”页就先列出已找到的工具；要等这次检查完成才能卸载。
- 在“更新”页，跨大版本的更新会标“大版本更新”，除非这一行已经写着它会自行更新，或者终端运行的是另一份
  （“终端用另一份”）。待更新的工具下面的“最近的更新记录”（没有待更新时在最上面）列出 30 天内的更新：成功的，以及未能更新的
  （知道原因时写出原因，例如“未能更新：网络连接失败”）和结果对不上的（写明是怎么回事，例如更新后读到的版本没有变时写“没有更新成功：版本没有变”）；
  后两种只在上次检查仍为这个工具提供更新时列出。
  取消的更新和卸载不列。这个列表重启后仍在（存在 `history.json` 里），直到你按“清除记录”。
- 卸载前的预览会列出卸载后会保留的东西——AI 工具的设置和数据文件夹（内置表格里写了的）、Ollama 的模型——能算出大小的
  写出大小，并可以拷贝路径；预览里没有任何删除它们的按钮。
- 别的来源要靠它运行的 Homebrew 软件，在那个来源还有自己的工具时不能卸载：预览把那个来源接在 Homebrew 自己的依赖者
  后面，列在“依赖此工具的软件”下——npm 靠着运行的 `node@22` 下是“npm及其4个工具”，pipx 工具环境所用的
  `python@3.13` 下是“pipx装的2个工具”，`ollama` 下是 Ollama 及其模型——“卸载”保持不可点，并写明要先卸载哪些工具；
  批量卸载也用同样的话把它留下。预览只顺着链接读取，不运行任何命令（`docs/what-we-run.md` 的
  “What runs on a Homebrew package”）。npm 自己那一行不提供“卸载”。
- “已安装”页里，“卸载…”可用的行前面有复选框。最多勾 20 个，点“卸载所选”，一个预览里列出全部：按实际执行的
  顺序写出每个会删什么；留给它自己那一行单独卸载的，逐个写明原因，例如开始后无法取消的、会永久删除文件的、
  卸载步骤删什么无法事先得知的、还有没选上的软件要用到的；卸载后会保留的东西，每条路径只列一次；确切的命令收在“查看命令”
  里（有工具的卸载会把文件移到废纸篓时是“查看命令和路径”）。确认后，每个工具运行的就是它那一行单独卸载时的那一次
  卸载——批量里依赖它的工具没能开始的，它先不卸载；同一个来源上的按列出的顺序开始。
- “其他程序”页的扫描不读 macOS 在应用读取前会先问你的那些位置——桌面、文稿、下载、图片、影片、音乐、iCloud
  云盘和其他云盘、其他应用的数据——也不读 `/Volumes` 下的其他磁盘，经过链接也一样。要扫描的文件夹在这些位置里时
  不读取，列表下方有一行写出有几个没有读取（打开“显示技术细节”后，它的 ⓘ 里列出是哪些）；指向这些位置的程序
  链接按它自己的名字列出，标着“指向受保护的位置”，不跟进去。
- Homebrew 的更新或卸载因为要输入 Mac 密码而停下时，日志里会给出一条命令，拷贝到终端里运行，就能在那里
  输入密码。
- 把“检查更新”设为“每天”或“每周”后，Banager 运行时，设置里会写出下次检查大约在什么时候。设置里的“拷贝诊断信息”（菜单栏“帮助”里的同名项会
  带你到这里）会拷贝一段关于 Banager、这台 Mac 和各来源的简短文字，可以粘贴给帮你看问题的人；勾选后才包括
  工具清单，个人文件夹的路径写成 `~`。
- 菜单栏“帮助”里的“检查工具环境…”（概览和设置里“工具环境”旁边的“检查…”也一样）用几行短句说明这台 Mac 上的工具
  环境：有没有读取终端登录时的设置、哪个来源没有响应、终端里找不到或装了不止一份的工具有几个、Homebrew
  停用或保留了其他版本的工具、实测占用的空间；数到工具的那几行和说某个来源有问题的那几行都有“查看”，会打开
  对应的列表或那个来源。不打分，
  只用上次检查的结果，不运行任何命令。
- 用 OpenAI 自己的脚本装的 Codex 只列出来：Banager 不为它运行任何命令，连版本检查也不做；npm 的
  `@openai/codex` 和 Homebrew 的 `codex` cask 仍算在各自来源下。用它自己的安装脚本装的 opencode 也只列出来：
  它的安装脚本没留下写着版本的文件，Banager 也不为它运行任何命令，所以版本不知道。

Banager 在打开时、每次操作完成后，以及你按下“概览”“更新”“已安装”三页页头的“重新检查”、或在任一页
从菜单栏选“显示”菜单里的“重新检查”（⌘R）时检查各来源，页头上也写着上次检查是多久以前；正在检查时，
再按也不会多查一遍。没能读取已安装的工具时页面上的“重新检查”，和 Homebrew 软件清单没更新成功时提示里的“重新检查”，做的是同一次检查；
后台运行的 Homebrew 索引更新自行结束时，它也会自己再查一遍（`ipc::refresh_on_background_change`，
`src-tauri/src/lib.rs:70-73`，不需要用户动手）。检查时会运行 Homebrew 自己的 `brew update`，
它会更新 Homebrew 本身和它的索引；Homebrew 把你装的某个软件在 formula 和 cask 之间挪了位置或者改了名时，
它还能自己安装、移动或卸载 Homebrew 软件（见 `docs/what-we-run.md` 的“Homebrew”一节）。
上一次 `brew update` 成功后，六小时内的检查都不再运行它（按时钟算，Mac 睡眠的时间也算在内；
时钟被调回到它结束之前，就当六小时已过）；上一次失败了，下一次检查就会再运行——
如果当时的检查没等它结束，那就是它结束时引发的那次检查之后的下一次。
在“设置”里把“检查更新”设为“每天”后（默认“不自动检查”；设为“每周”就是每周一次），
Banager 开着时还会每天做一次同样的检查，查到的更新都不安装，退出后不检查。
所有来源都失败的那次每天检查——Homebrew 的索引没能更新也算失败——不算数：15 分钟后再查，
之后每连续失败一次，等的时间就翻一倍（30、60、120、240 分钟），最长六小时。你自己检查一次，
或者某次每天检查不是所有来源都失败，就算数，等待也从头算起（见 `docs/what-we-run.md` 的“The daily check”一节）。
再打开“检查更新”下面的“有更新时通知我”，
每天的检查发现你还没看到过的更新、而最前面的是别的应用、不是 Banager 时，会发一条通知，说有几个
工具可更新。点这条通知会把 Banager 切到最前面；如果 Banager 的窗口关着或最小化在程序坞里，
而且发通知以后还没到过最前面，窗口会回来，并打开“更新”页。Banager 收不到点击本身，只知道自己到了
最前面，所以在窗口再到最前面之前，窗口关着或最小化时用别的办法把 Banager 切到前面——⌘-Tab、
点程序坞图标——也会这样。
关掉窗口不会停止正在进行的操作；打开“操作完成时通知”（默认关闭）后，一批更新或卸载在别的应用位于最前面时做完，
会发一条通知说结果，比如“已更新3个工具”——你在窗口里看着做完的不会通知；窗口关着、Banager 仍在最前面时做完的，
等你切到别的应用时再通知。
“更新”页每行“…”菜单里，“跳过此版本”和“不再提醒此工具”之间有“30天内不提醒”：这个工具的所有版本（不只是当前这一版）
30天内都不列出，也不计入数量和程序坞角标，到期后重新列出（不会再发通知）；在那之前“设置”里会列出它，可以撤销。
“其他程序”页（边栏“来源”下的最后一行）的页头换成“重新扫描”和上次扫描是多久以前，
它只属于那一页：只重新扫描那一页看的几个 bin 文件夹，按各来源上次已知的状态判断——并不刷新各来源。
“设置”页的页头两者都没有。
（来源装了却没报路径的程序也会列在那一页，比如 uv 自带的 `uvx`：缺口在来源那边，页面照实说。Cargo
每个 crate 只报一个程序——与 crate 同名的那个，没有就报记录里的第一个——所以一个 crate 装了好几个程序时，
其余的（如 `cargo-binstall` 的 `detect-targets`）会留在那一页，直到它能把全部报出来。）

菜单栏的“显示”菜单像访达和邮件的一样，能打开边栏里的各页：“概览”（⌘1）、“更新”（⌘2）、“已安装”
（⌘3，和点边栏的“已安装”一样，显示全部已安装的工具）和“其他程序”（⌘4）。下面是“重新检查”（⌘R）和
“搜索”（⌘F），后者打开“已安装”页，并把光标放进搜索框；“设置…”（⌘,）在“Banager”菜单里。窗口关着或最小化时，
选这些项会先把窗口叫回来。菜单栏“帮助”里的“键盘快捷键”列出 Banager 里能用的按键，分三组：窗口（上面这些，再加
“关闭窗口”和“退出”）、列表（↑ ↓、翻页和跳到两头的键、用空格键勾选一行、在“已安装”里用回车和 Esc 打开和关闭详细信息、Tab）
和对话框（回车、Esc）。它上面的“常见问题”用几句大白话回答十个常见问题：终端里找不到命令、为什么有的工具不能在这里更新、
“装了两份”、Mac 密码、卸载后留下什么、Banager 自己会改动什么、“其他程序”、占用空间、大版本更新和自动检查；
能动手处理的，旁边有“查看”按钮，直接打开对应的页面或“已安装”里的显示选项。

第一次打开 Banager 时，会出现一个欢迎页，用三条短句说明它做什么：Claude Code、Codex、Gemini CLI 这类在终端里用的
工具都列在一处，第一次检查完成后，侧栏会出现“来源”，按安装方式列出，比如 Homebrew、npm 或工具自带的安装程序；更新或卸载前先写明要做什么，确认后才开始；Banager 自己不改终端的配置文件，不收集使用情况，也不需要
账号。（唯一的例外是 rustup 自己的卸载，它会删掉当初加进这些文件的那一行，预览里会写明。）第一次检查在它背后照常进行。不管怎样关掉它——点“开始使用”、按回车或 Esc、点它外面——设置文件都会记下
已经看过，以后不会自己再出现；随时可以从菜单栏“帮助”里的“欢迎使用Banager”再打开。

关掉窗口——点它的红色按钮，或从菜单栏选“文件”菜单里的“关闭窗口”（⌘W）——Banager 仍在运行，进行中的操作照常
继续；点程序坞里的图标，窗口按你离开时的样子回来（发过通知后照上面说的，改为打开“更新”页），不会重新检查。
选“退出 Banager”（⌘Q）才会退出。不过，还有更新或卸载在排队或进行时，退出——⌘Q、程序坞图标菜单里的“退出”，
或者退出登录、重新启动、关机——会先把窗口叫回来问一句「还有2个操作未完成」：现在退出会中断它们，正在更新的工具
有只更新一半的风险；已经开始、不能取消的（比如 rustup 的自我更新）也会点名。选「取消」，Banager 接着运行；
选「退出」才退出。Banager 当场回答 macOS，所以退出登录、重新启动或关机会被取消，而不是一直等着，选了
「退出」之后要再操作一次（`src-tauri/src/quit.rs`）。窗口要是问不了——出错停了，或者 2 秒内没把这句问话
显示出来——Banager 就直接退出，不会在没人能回答时拦着不退。强制退出仍会立刻退出。
