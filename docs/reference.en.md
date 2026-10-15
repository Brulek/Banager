**A Mac app for everything you installed from the terminal and then forgot about.**

Banager (formerly Canager) is short for bao-manager: bao is the pinyin of 包, Chinese for
"package". It also sounds like banana.

If you've followed a few tutorials, you probably have Homebrew formulae, a couple of global npm
packages, some Python tools, a Rust binary and an Ollama model or two scattered across your Mac.
Each was installed with a different command. Updating them needs a different command again.
Removing them needs a third. Most people never do either, and the tools quietly rot.

Banager puts all of it in one window: what you have, what has an update, and a button for each.

> **Status: pre-release.** The core and the UI work and are covered by 2432 Rust tests (plus 11 more
> that only run with `--ignored`: they touch a real Homebrew, the real Trash, AppKit or this Mac's
> disk, or time a large run) and 3351 front-end tests, but there is no downloadable build yet — v0.1 is being prepared. Nothing here is
> ready to rely on. Banager can't update itself yet: a release build makes no updater files and
> publishes no `latest.json`.


## What it manages

Banager updates and uninstalls what you already have; it can't install anything new
(see "What it deliberately does not do yet").

| Source | Reads | Updates / removes |
|---|---|---|
| Homebrew — formulae and casks | yes | yes |
| npm — global packages | yes | yes, when the prefix is yours to write; updates are offered only for packages listed directly in the selected global installation, and nested dependencies are not offered separately |
| pipx | yes | yes |
| uv — tools | yes | yes, but no uninstall while `UV_TOOL_DIR` is set in Banager's environment: removing the last tool, uv would then also delete the folder above that one, with every file in it, when that folder holds no other folder; the row says so |
| pip | yes, even when `pip.conf` turns on quiet output: Banager overrides that setting for its own list commands only, without editing the file; if a pip package index rejects the check and no version is confirmed, the package is marked “Can't check”, and its error details can be copied | **no** — Banager will not drive pip's installer; it points you at pipx or uv |
| cargo | yes | yes, with a warning that it compiles locally |
| Ollama — models | yes, unless `OLLAMA_HOST` is an address Banager can't use | yes, likewise, but not at an address with a login |
| Claude Code — the native install, via its own installer | yes | updates yes, except while its own `~/.claude/settings.json` turns every update off with `DISABLE_UPDATES`, which `claude update` obeys too: then a newer version is listed under "N more can't be updated here", marked "Updates turned off", with no Update button; install no (the installer is Anthropic's, and Banager never runs it); uninstall yes — its program files, download cache and launcher go to the Trash, and your settings and history stay |
| rustup — the Rust toolchain manager, via its own installer | yes | updates yes (`rustup self update`), unless `RUSTUP_UPDATE_ROOT` names another address than the official one: then it is not checked and not updated here, and an update already confirmed is refused; install no (the installer is rust-lang's, and Banager never runs it); uninstall yes (`rustup self uninstall -y`), offered only when Rust is in its standard folders (`~/.cargo`, `~/.rustup`) and previewed with everything it removes — permanently, not to the Trash: every toolchain by name, the whole Cargo folder with its settings and saved login, and the programs in its `bin` folder, named where known — by the name the Installed page gives a program Cargo's records list (`jj-cli`, not `jj`; also one cargo-binstall installed), else by its file name, on a line of its own that says reinstalling Rust won't bring it back. Neither can be cancelled once it is running, and the preview says so |
| Antigravity CLI (`agy`) — Google's terminal agent, via its own installer | yes | updates **no** — it installs its updates itself in the background while its automatic updates are on, and although Google documents `agy update` for existing installations, Banager hasn't verified running it, so a newer version is listed under "N more can't be updated here", marked "Updates when run", whose ⓘ says so and that opening the tool can check for a new version if its automatic updates are on, though not always right away — by typing `agy` in Terminal, where typing it runs this copy (Banager looks the newer version up on Apple silicon only: on an Intel Mac the row reads "Can't check" and nothing is sent); install no (the installer is Google's, and Banager never runs it); uninstall yes — the `agy` program, and any `agy.<time>.old` backup its updater left beside it, go to the Trash; its conversations, history and working files in `~/.gemini/antigravity-cli` stay, and so do its staging folder in `~/.cache` and the `PATH` lines its installer added |
| Grok Build (`grok`) — xAI's terminal agent, via its own installer | yes | updates yes (`grok update`, offered when grok's own `update --check --json` says a newer version exists; how `grok update` behaves when nothing can answer a prompt is yet to be recorded on CI); install no (the installer is xAI's, and Banager never runs it); uninstall yes — its downloaded versions, its bundled agents and shell completions, any fallback links its installer made in `~/.local/bin`, and the two links in its `bin` folder go to the Trash (the folder itself, which its installer put on your `PATH`, stays); `~/.grok`'s settings, login, sessions and memory stay |
| Codex — OpenAI's terminal agent, via its own installer | yes: its version is read from the folder name its installer links to, and no command runs for it, not even a version check; npm's `@openai/codex` and Homebrew's `codex` cask stay those sources' own rows | updates **no** — its newest version is not looked up, so the Updates page lists nothing for it (the row says Codex can update itself when its installer's auto-update file names the release in use, and otherwise that its updates aren't checked); install no (the installer is OpenAI's, and Banager never runs it); uninstall yes — its two links in `~/.local/bin` and the program folder `~/.codex/packages/standalone` go to the Trash; the rest of `~/.codex` (settings, login, sessions) stays, and so does the `PATH` line its installer may have added |
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
apps' data — nor other disks under `/Volumes`, nor network volumes wherever they are mounted, even by way of a link; a network volume your
home folder is on is read, unless it is mounted under `/Volumes`. A folder to scan that is in one
is not read, and a line under the list says how many were left out (with Show technical details on,
its ⓘ names them); a program that is a link into one is listed by its own name, marked **Points into
a protected place**, and the link is not followed.

## What it tells you about each tool

- The **Show** menu in the toolbar of the Updates and Installed pages narrows the list to **AI Tools**:
  Claude Code, Codex, Gemini CLI, Ollama and the other AI tools in a table built into Banager,
  recognized by the sources and package names that table lists for each (a Homebrew tap's formula by
  its full name, so another tap's formula of the same name is not one). On the Installed page it also offers **Installed More Than Once** (an AI tool with more than one copy), **Not Found
  in Terminal**, **Disabled or Deprecated by Homebrew** and **Other Versions Kept** (Homebrew formulae with
  another version kept beside the one in use), each of these four with how many tools it shows.
- A tool's details say, under **In Terminal**, what typing each of its commands runs — this copy,
  another copy or program, or nothing, when the command sits in a folder Terminal doesn't search —
  judged from the Terminal settings read when Banager opened. An AI tool with another copy installed says
  **Installed twice**, and where Banager can tell, its details say which copy Terminal runs. An AI tool's
  Homebrew formula that Homebrew didn't link (another copy's file was in its way, say) still counts as a
  copy, and its details say Homebrew didn't link it where Terminal looks; where its other copy is the one
  Terminal runs, the Updates page says **Not used in Terminal**.
- A package Homebrew has disabled or deprecated says so, with what that means and, where Homebrew
  gives one, the name it suggests instead. A newer version of a disabled one, which `brew outdated`
  still lists, is held back on the Updates page as **Disabled**, with no Update button. The details also list a formula's other installed versions
  and Homebrew's own notes, in English, folded.
- When uv reports tools it could not read, the other tools remain listed, a notice explains the omission and
  gives the command that shows uv's own words, and the check is shown as incomplete.
- After each check Banager measures, read-only, how much disk the tools take: a tool's details show its
  size once measured, the Installed page can be sorted **By Size**, and sorted by source a heading says
  what that source's measured tools take in all.
- The Installed page can also be sorted **By Date Installed**, newest first. Only Homebrew says when a
  tool was installed, so the tools from every other source come after Homebrew's, by name, with "—".
  Sorted by size or by date, a source's packages installed for other software to use, once shown,
  follow the same order under their own line.
- While the first check since launch is still looking for updates, the Installed page already lists
  what it found; uninstalling waits until that check is done.
- On the Updates page, an update to a new major version is marked **Major update**, unless its row
  already says the tool updates itself or that Terminal runs another copy (**Not used in Terminal**).
  For self-updating apps, the displayed version change and major-update warning use the version read
  from the app when available.
  **Update All** leaves such a copy unticked, and no count of updates — the sidebar's, the Dock badge's,
  the Overview's — counts it.
  **Update History**, under the updates still to install (at the top when there are none), lists the
  updates of the last 30 days: those that succeeded (**Done by an earlier update** where, on Homebrew, an update of the
  same Update all and kind (formula or cask) that changed something ended before its turn and it was already at its new version, as Homebrew
  updates a formula's dependencies first; **Already up to date** where it was new before its turn otherwise), and those that
  failed (with the cause where one is known, such as **Couldn't update: Connection failed** or **Couldn't update: The
  app isn't where it was installed**, and the tool's own first error line behind the ⓘ where no cause is known or the
  cause points at it, as for a file in the way, kept across restarts too) or whose result didn't add up, each saying what happened in its own
  words (for example, **Didn't update: same version** where the version read after the update was the one before, below
  its new version). An update installed though a step after it failed — Homebrew's post-install step, say — reads
  **Updated with an error**, with the version it moved to, what that means behind the ⓘ — that the new version
  isn't linked where Homebrew's link step failed, otherwise the tool's own first error line, kept across restarts
  too — and View Log where there is something to show (this launch's log, or what was kept of the steps after it,
  such as old versions the confirmation said would be removed and weren't); its row says the same, with no Retry, and it counts as no update to
  install. A cask's update that fails always reads as failed: Homebrew puts the old version back, and where even that
  fails, nothing says the new app is in place. A failed one or one whose
  result didn't add up is listed only while the last check still offers that tool an update; one updated with an
  error is listed either way, as a success is. Cancelled updates
  and uninstalls are not listed. While the Updates page is open, Update History checks the 30 days again every
  minute and whenever what it lists changes. The list is kept across restarts in `history.json` until you press Clear
  History, within two limits of the file's own: it keeps the last 1,000 updates and uninstalls, so after more than that
  within 30 days the oldest go sooner, and it drops records once they are over 180 days old, long after the list stops
  showing them.
  If that file is there but can't be read (an empty one counts as missing), Banager leaves it as it is and keeps new records only until you quit;
  a warning over the Updates page says so and what to do, whatever the page shows. Clear History hides the records
  listed when Banager takes it, and later ones stay; when it can't be saved, a warning says the cleared records may come
  back after reopening. A history write that failed is tried again after the next update, Clear History, or on quit,
  and never changes an update's result. After **Reload** on the **Couldn't show this page.** screen, an update from
  this launch keeps the date the history saved for it, and its place in the list.
- An uninstall's preview lists what stays after it — an AI tool's settings and data folders where the
  table names them, Ollama's models — with how much each takes where it could be measured, and Copy Path; nothing in it
  deletes them. One line under the list says that settings and data you don't need can be moved to the Trash in Finder
  (and, where a Terminal settings file is listed too, to leave that file where it is) — but not while another copy of
  the tool, or another tool of its family, stays installed, nor where a folder holds another copy's program or another
  tool's or app's data, or may hold some that its measurement stopped short of ruling out, since what stays installed
  may still use it. `~/.codex` and `~/.copilot` are always such folders:
  the ChatGPT, Codex and GitHub Copilot apps keep their own settings and data there too, which a line under them says.
  So is `~/.ollama/models` while Ollama.app or another `ollama` that can run on its own (not a version manager's
  shim) stays beside the Homebrew `ollama` being uninstalled:
  a line under it says another copy of Ollama still uses those models.
- A Homebrew package another source runs on can't be uninstalled while that source has tools of its own:
  its preview lists the source under **Software that uses it**, after Homebrew's own dependents — "npm with
  its 4 tools" under the `node@22` npm runs on, "2 tools installed with pipx" under the `python@3.13` their
  environments use, Ollama with its models under `ollama` unless Ollama.app or another `ollama` stays to run them —
  keeps Uninstall off and says which tools to
  uninstall first; a batch leaves the package out with the same words. The preview finds them by following
  links, read-only, and runs no command (`docs/what-we-run.md`, "What runs on a Homebrew package").
  This protection also covers tools installed in their own environments by pipx or uv: for example, uv installed
  with pipx can't be uninstalled while the uv source Banager runs through it still has tools to update or uninstall.
  npm's own
  `npm` row offers no Uninstall, nor does the row of a pipx or uv that installed itself as one of its own
  tools. Nor does `corepack` where it came with a Node from Homebrew linked by hand
  (`node@22`): uninstalling it would delete that Node's `corepack` command and, where Homebrew installed them,
  its `pnpm` and `yarn` ones. Like that npm, it updates with its Node.
- On the Installed page, each row whose **Uninstall…** is available has a checkbox. Tick up to 20 and
  **Uninstall Selected** opens one preview of them all: what each one removes, in the order they will
  run; the ones it leaves to their own row, each with why — such as an uninstall that can't be cancelled
  once it starts, one that deletes files permanently, one whose steps can't be known in advance, one still
  used by software you didn't select, one copy of Ollama while another goes in the same batch and the models listed on this Mac still need
  one to stay, even when they are kept outside `~/.ollama/models`; what stays afterwards, each path once; and the exact commands
  under **Show Commands** (**Show Commands and Paths** when an uninstall moves files to the Trash).
  Confirming queues each tool's own uninstall, the same one its row runs — a tool whose dependent in
  the batch could not be started is left for later; on one source they start in the order listed.
- When a Homebrew update or uninstall stopped because it needed your Mac's password, its **View Steps**
  shows the command to copy and run in Terminal, where you can type it. When Banager could save its history, after
  it is quit and reopened an update's **View Steps** stays on its row for up to 30 days, while the tool still has an
  update and no later update or successful uninstall of it has taken its place, and in **Update History** unless
  you press **Clear History**; there it prepares that command again, as no log is kept across a restart. The history
  keeps only its last 1,000 records, so View Steps can go sooner.
- With **Check for updates** set to Daily or Weekly, Settings says about when the next check is due,
  while Banager is running. Its **Copy
  Diagnostic Info** — Help's item of that name takes you there — copies a short text about Banager,
  this Mac and its sources to paste to whoever is helping you; it lists your tools only when you tick
  the box, and writes your home folder as `~`, though a home folder outside the Users folder may still
  appear in full. For a source that can't run, ran into an error or isn't
  responding, it includes the error details that source's tool wrote, which can quote its settings
  file, and for a check that didn't finish, the reason it kept — the tool's last five lines at most,
  inside Banager's own words for what failed, also behind **Show Error Details** on the notice that
  some checks didn't finish; user names and passwords that Banager recognizes in web addresses and proxy settings are
  hidden in them
  (`docs/what-we-run.md`, "What a tool prints about a login", says which).
- **Check Tool Setup…**, in the Help menu and as a button on the Overview and in Settings, says in short lines
  how this Mac's tools are set up — whether Terminal's login settings were read, each source that
  isn't answering, how many tools Terminal can't find, how many AI tools are installed more than once, which ones Homebrew didn't link
  where Terminal looks, what Homebrew disabled or keeps other versions of, and the disk measured; a source whose list or check is incomplete counts as a source with a problem. Each
  line that counts tools, but the one that names those Homebrew didn't link, and each line about a source
  with a problem that the last check still lists, has a **Show in Installed** (or **Show in Other Programs**) button that opens that list or that source. It has no score; it is built from what the last check found, and runs nothing.
  When some of its lines are warnings, the Overview's row says how many need attention.

Banager checks every source when it opens, after each operation, and whenever you press **Check
again** in the header of the Overview, Updates and Installed pages, which also says how long ago the
last check finished, or choose **Check Again** (⌘R) in the menu bar's View menu, on any page; while
a check runs, neither starts another. The **Check Again** on the page Banager shows when it couldn't
load installed tools, and the one on the notice of a Homebrew index Banager couldn't update, run the
same check, and a Homebrew index update left running in the
background starts one on its own when it ends (`ipc::refresh_on_background_change`,
`src-tauri/src/lib.rs:70-73`). Checks run Homebrew's own `brew update`, which updates Homebrew and
its index, and when Homebrew has moved a package you have between a formula and a cask, or renamed
one, can install, move or uninstall Homebrew packages by itself without a preview or confirmation
(`docs/what-we-run.md`, "Homebrew"). A Homebrew whose own folders this account can't change gets no
`brew update`, which would only refuse there; it is view only. An `OLLAMA_HOST` Banager can't use gets no
request, not even one to Ollama's default address, and no Ollama models are listed. After a `brew update`
that succeeded, checks skip it for six hours on the clock (time the Mac spends
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
posted once you switch to another app (`docs/what-we-run.md`, "The notification when operations finish").
An update with warnings is counted as needing attention in its completion notification. The result at
the bottom of the window includes every operation in the run, even when older operation records have
been released. A row's … menu on the Updates page has *Remind Me in 30 Days* between *Skip This Version* and
*Don't Remind Me About This Tool*: it hides every version of that tool, not only the one offered, from
the list, its counts, the Dock badge and the update notification for 30 days, after which the tool is
listed again and counted like any other update — and, when the update notification is on, the first daily check
that finds it then reminds you of it with one, even if you'd been shown it before;
Settings lists it until then, with a button to undo it. The Other Programs page's header has *Scan Again* in its place, with how long ago
that page last scanned: it re-runs only that page's scan of your bin folders, against the sources'
last known state — it does not refresh the sources. Settings' header has neither.

The menu bar's View menu opens the sidebar's pages, as Finder's and Mail's open theirs: Overview
(⌘1), Updates (⌘2), Installed (⌘3 — on everything installed, as the sidebar's Installed opens it)
and Other Programs (⌘4). Below them are Check Again (⌘R) and Search (⌘F), which opens the Installed
page with its search box focused, and Settings… (⌘,) is in the Banager menu. With the window closed
or minimized, each of these brings it back first. **Keyboard Shortcuts** in the Help menu lists the
keys Banager answers to, in three groups: the window's (these, Close Window and Quit), the lists' (↑ ↓
and the keys that page or jump to an end, Space to tick a row, Return and Escape for Installed's
details, Tab) and the dialogs' (Return, Escape). **Common Questions**, above it, answers fifteen questions
in a few plain sentences each — a command Terminal can't find, uv tools missing from the list, why some tools can't be updated here,
"Installed twice", the Mac password, what an uninstall leaves, what keeps a tool from being uninstalled, what Banager itself changes, Other
Programs, sizes, major updates, an update's warning, a background service after its update, the automatic check and
settings from the old name Canager — with a button named for the page it opens
(*Show in Installed*, *Show in Updates*…) where the answer can be acted on.

The first time Banager opens, a welcome sheet shows three short titles — see what's installed; you confirm each update and uninstall you choose; no data collected — and nothing more. The first check runs behind it, and its `brew update`, which no one chose, asks nothing, as above. However
you close it — **Get Started**, Return, Escape or a click beside it — its settings file records that
it was shown, and it doesn't open on its own again; **Welcome to Banager** in the Help menu shows it
again at any time. That is remembered only if the save succeeds: if it fails, a line over the page
says the sheet may show again the next time you open Banager.

If Banager's settings file is there but can't be read (an empty one counts as missing), Banager uses the defaults for that launch and
says so before you edit a setting — in a warning over every page, with *Open Settings*, and in full at
the top of Settings: automatic checks and notifications are off, the language follows your Mac, and
saved choices to skip versions or hide or pause update reminders aren't applied. The file is left as it
is, and no setting can be saved until you reopen Banager: quit it, repair access to `settings.json` in
`~/Library/Application Support/com.brulek.banager` or restore a valid copy — or move it to the Trash
to start fresh — then reopen Banager. A settings file that can't be read doesn't bring up the welcome
sheet.

Banager used to be called Canager, and a copy built under that name kept its settings — among them the
versions it was told to skip and the tools it was told not to remind you about — in
`~/Library/Application Support/com.brulek.canager`. Banager doesn't read that folder, so those settings
aren't brought over and those updates can show again. To bring them over, quit Banager and Canager, open
that folder with Go to Folder in Finder's Go menu and copy `settings.json` from it, leaving the original
there, then paste it into `~/Library/Application Support/com.brulek.banager` (create that folder if it
isn't there) — but only if there's no `settings.json` there yet: one that is there holds Banager's own
settings, so don't replace it. To use the old settings in place of Banager's own, first move Banager's
`settings.json` into a new, empty folder somewhere else, so it can be put back, then paste.
**Common Questions** has the same steps. Canager kept no update history, so there is nothing else to
bring over.

Banager saves its settings, its history of updates and uninstalls and its window's size and position in
`~/Library/Application Support/com.brulek.banager`. WebKit, the part of macOS that draws its window, can
also keep data for it in `~/Library/WebKit/com.brulek.banager` and a cache in
`~/Library/Caches/com.brulek.banager`. To remove all of this once you stop using Banager, quit it, then
open each of those three folders with Go to Folder and move it to the Trash, skipping any that isn't there
— those folders only, never the Library folders they're in; your settings and history go with them.
Canager's folders, the same three ending in `com.brulek.canager`, come last: move them to the Trash
only after bringing its settings over as above, or once you don't need them.

Closing the window — its red button, or Close Window (⌘W) in the menu bar's File menu — leaves
Banager running, and an operation under way carries on; its icon in the Dock brings the window back
as you left it — or on the Updates page after a notification, as above — without a new check. Quit
Banager (⌘Q) quits it. While an update or uninstall is still queued or running, though, quitting —
⌘Q, Quit in the Dock icon's menu, or logging out, restarting or shutting down — first brings the
window back and asks: *2 operations haven't finished*, since quitting now stops the ones it can, and
a tool that is being updated can be left half-updated. One that has started and can't be cancelled,
such as rustup's self update, it names apart and asks you to wait for: quitting neither stops nor
waits for it, and its command is left running with nowhere to write its output, so it can fail
partway. One whose result is being checked it
waits for, up to a few seconds. *Keep Waiting* leaves Banager running, and *Quit* quits. Banager answers macOS at once, so a logout, restart or shutdown is called off rather than
kept waiting, and after *Quit* you start it again (`src-tauri/src/quit.rs`). Should the window
be unable to ask — it stopped working, or doesn't show the question within 2 seconds — Banager quits
rather than hold the quit with nobody there to answer. Force Quit still quits at once.

Adding a source is one Rust file implementing one trait, plus a TOML metadata file.

## What makes it safe to point at your machine

This app runs package managers on your behalf, so the boundary matters more than the features:

- **Package-manager operations run directly.** Each uses an argument vector, without a shell.
  At launch, Banager separately runs a bounded interactive login shell to read `PATH` and proxy
  and mirror settings; its startup files may run commands. A failed read is retried on a later
  refresh. [How Banager runs anything](what-we-run.md#how-banager-runs-anything) lists the
  exact shell command, its limits and the settings it takes.
- **The window cannot ask for a command.** The UI sends an operation kind and a single-use,
  expiring identifier for a plan the Rust side built itself. There is no general "run this" path,
  so a compromised web view cannot invent one.
- **A homepage opens only in your default browser.** A tool's `https` homepage in the Installed
  page's details opens only when it is exactly the one the tool's source lists, and Banager names
  the browser — the one macOS has as the default for `https:` — rather than letting the address
  pick an app that claims its domain; when macOS names no default browser, nothing opens. A plain
  `http` homepage is shown to copy.
- **You can preview the operations you choose.** For updates and uninstalls you choose in Banager,
  the confirmation lets you see the exact command before it runs, with the variables Banager sets
  for it (all but two kinds, which `docs/what-we-run.md` lists under "How Banager runs anything":
  the `PATH` and the proxy and mirror settings it takes from your login shell, never shown since a
  proxy setting can hold a password, and `MISE_NOT_FOUND_AUTO_INSTALL=false`, which every command
  gets so that mise does not install a missing version on its own; a tool mise was told to install
  on first use it installs whatever that says, so Banager never starts a version manager's shim
  itself, such as mise's or pyenv's, nor a script whose `#!` line starts one, and a tool it finds
  only through one shows that its program couldn't start, as npm does when a folder in a protected
  place, such as Downloads, comes before `node` in your `PATH`) —
  one press on "Show Command" ("Show Commands" when there is more than one), or open from the
  start with Settings' "Show technical details" on — and
  says whether it may ask for your password; an uninstall that runs no command lists instead the
  exact paths it will move to the Trash. An Ollama whose `OLLAMA_HOST` has a login is view only, since
  `ollama` can't use the login, so Banager does not plan model-changing commands for it.
  This does not isolate subprocess environments: other commands still inherit `OLLAMA_HOST`
  if it was set in Banager's process environment. An uninstall also says what it will affect. A Homebrew
  formula update normally previews a follow-up `brew cleanup <name>` after success: it removes
  that formula's old installed versions, its outdated cached downloads and every unreferenced
  download in Homebrew's cache, including other packages' downloads. It is omitted for pinned
  formulae, unreadable versions or pin records, user settings that disable cleanup, and when
  `brew.env` enables automatic cleanup or its effect cannot be read; the conditions are checked
  again before cleanup. Separately, when `brew.env` turns automatic cleanup back on, the preview
  explains its broader scope: old versions of the updated software and its updated dependencies,
  stray old downloads and, when periodic cleanup is due, those of all Homebrew software. If
  autoremove is also on, periodic cleanup can remove unneeded dependencies. For a tool configured
  to run with brew services, the update confirmation and its saved result remind you to restart
  it if it is still running the old version.
- **Nothing is deleted quietly.** An uninstall that would break other packages says which ones,
  in your language. Banager runs Homebrew with its autoremove off, so a Homebrew uninstall does
  not also uninstall the other packages that were installed only as dependencies and that nothing
  needs anymore; when a `brew.env` file turns autoremove back on, the preview says Homebrew will.
- **A tool with no uninstall command goes to the Trash, not away.** Claude Code's makers document
  its removal as a list of paths. Banager moves those paths, plus its installer's download cache,
  to the Trash itself, with the call Finder uses, so until you empty the Trash you can drag them
  back — and Finder's Put Back will likely work too; the preview lists each path it will move and
  each one it keeps (your settings and history, in `~/.claude` and `~/.claude.json`). Antigravity
  CLI, Grok Build and Codex's own installer publish no removal instructions at all, so their lists
  are Banager's own reading of how each was installed, and their paths go to the Trash the same way. Moving files to
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
<http://localhost:1430/> — [docs/ui-preview.md](ui-preview.md) has the rest. `pnpm tauri:mock`
puts the same mock front end in the app's real window, title bar and all. Its Rust startup still
runs the bounded login-shell environment read described above, including shell startup files;
the mock front end does not run package-manager operations. The browser-only mock does not
start that Rust backend.

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

Apart from the ignored tests below, the tests work in folders of their own, with stand-ins for the home folders, and never look in the tester's.

`cargo test --workspace` has eleven `#[ignore]`d tests, all skipped by a plain `cargo test`:

- Two in `crates/banager-core/tests/brew_live.rs`: one reads the real Homebrew; the other installs
  and removes the `hello` formula.
- One in `crates/banager-core/tests/standalone_uninstall_test.rs` moves five throwaway items it
  creates (`banager-trash-smoke-…`) into this Mac's real Trash and leaves them there.
- One in `crates/banager-core/src/icon/real.rs` has AppKit draw Calculator's icon and only reads.
- One in `crates/banager-core/src/protected.rs` probes this Mac's filesystem using temporary
  files and about a million lookups.
- Six benchmarks: one in `crates/banager-core/src/commands/round_tests.rs` and five in
  `crates/banager-core/tests/scale_test.rs`, for command matching, family assignment, sizes and
  refreshes with large inventories.

The Homebrew install/remove and Trash tests refuse to touch anything without `BANAGER_LIVE=1`.
CI runs the two Homebrew tests and the Trash test; run those or the AppKit test yourself with:

```bash
BANAGER_LIVE=1 cargo test -p banager-core --test brew_live -- --ignored
BANAGER_LIVE=1 cargo test -p banager-core --test standalone_uninstall_test -- --ignored
cargo test -p banager-core --lib icon::real -- --ignored
```

## Language

The interface supports English, Simplified Chinese and Traditional Chinese in Taiwan usage. It follows
the system language unless overridden in Settings. Following the system, dates and times use your Mac's own
formats wherever your Mac's language is one of the three: on a Mac set to English (United Kingdom), "13 Oct" and
14:30. Every label, heading, button and
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

The design and the reasoning behind it live in [`docs/superpowers/`](superpowers/) — the
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
or crates.io package says a line in English instead wherever Banager has one: 641 of them,
each rewritten, shorter, from the description the package's own registry gives it. In Chinese, a
row says a line in Chinese instead wherever Banager has one: 3,395 in each Chinese language, for Homebrew's
formulae and casks and for npm, PyPI and crates.io packages, each translated from the description
the tool's own source gives it. The three tables are built into the app, in
`src/assets/tool-descriptions/en.json`, `zh-CN.json` and `zh-Hant.json`. A table loads when needed
for rows or search; Traditional Chinese also loads its Simplified Chinese and English fallbacks.
Showing a description makes no network request. A tool's details show that
one line too, without its source's own description beside it; a tool Banager has no line for in the
window's language reads as it did before.

## License

Banager's own code is released under the [MIT License](../LICENSE). The logos of tools and sources
in `src/assets/tool-icons/` are not covered by it: they are their owners' trademarks and keep the
terms described under [Logos](#logos).
