# What Banager Runs

Every command Banager runs, every file it reads, writes or moves to the
Trash, every host it connects to and every environment variable it sets,
for the thirteen sources it manages today: Homebrew, npm, pipx, uv, pip
(read-only), Cargo, Ollama, and six tools with their own installer:
Claude Code, Antigravity CLI, Grok Build and rustup, and Codex and
opencode, which are listed only. Each sentence
describes what the code does now and names the function it describes, so
it can be checked against `crates/banager-core/src/adapters/` rather than
believed. `crates/banager-core/tests/what_we_run_test.rs` checks the parts
a test can: a section per registered source, every host on the https
allowlist, every environment variable Homebrew's and npm's commands are
given, that Cargo's section says every Cargo command is given the
variables in `CargoAdapter::ENV` and shows each one, the three Homebrew
flags this file promises are never passed, that pip's section shows the
`xcode-select -p` it asks before running an interpreter in `/usr/bin` and
says one with no developer tools behind it is skipped, that the
unknown-source scan's section and the section on which copy a command
runs each state the two limits the code enforces and name every place
they never read, that the latter says
it runs no command and which folders it reads, that the sections of the
three tools uninstalled by moving files to the
Trash (Claude Code, Antigravity CLI, Grok Build) name every path those
uninstalls move or keep and their time budget, and the never-list every
path of settings or state they keep, that Grok Build's section shows the
update check it runs on every refresh and says it installs nothing, that
the Trash section names the call and states the pause after each move,
that the app icons section names the call, the size an icon is drawn at,
and that no command runs for it, that this file names each permission of
the opener plugin the window has and the unknown-source scan's section
the call Show in Finder makes, saying it runs nothing else, that the
daily check's section says it is off by default, states how often it
looks, how long after a check it checks again and how long it waits
after checks in which every source failed, says Banager itself runs no
install from it and that `brew update` can install a package Homebrew
moved between a formula and a cask, and names each permission of the
notification plugin the window has, that Homebrew's section keeps
`brew update` out of its read-only table and cites the lines of
Homebrew's own code at which it installs, and that the disk-use section
states the two limits a round of measuring keeps to, names every place it
never looks into and says nothing is written, and that the section on the
data an uninstall leaves behind names every path it looks at, states its
two limits and says nothing is written or deleted.
`src-tauri/src/notify.rs`'s tests check that the section quotes what a
notification says in both languages, and `src-tauri/src/ipc.rs`'s that
the never-list says the window cannot ask for an install.

Throughout, `<brew>`, `<npm>` and so on stand for the absolute path of the
executable the adapter found; `{name}` is the one user-chosen argument a
command can carry.

## How Banager runs anything

**Never through a shell.** Every package-manager command is a fixed argv
array run directly against an absolute program path by `RealRunner::run`
(`crates/banager-core/src/runner/real.rs`): `Command::new(program)` with
the arguments appended one by one. No string is ever handed to `sh`, and
nothing Banager downloads is ever piped into one.

**One shell run, at launch, that runs no command.** An app opened from
Finder starts with a minimal `PATH`, so at startup (`run()` in
`src-tauri/src/lib.rs`) the `fix-path-env` crate runs the user's login
shell once — `$SHELL` (`/bin/zsh` on a Mac when `SHELL` is unset) with the
arguments `-ilc 'echo -n "_SHELL_ENV_DELIMITER_"; env; echo -n
"_SHELL_ENV_DELIMITER_"; exit'`, with `DISABLE_AUTO_UPDATE=true` in its
environment and the home folder as its working directory — reads the
`PATH` that shell exports, and sets it on Banager's own process
(`fix_vars` in `fix-path-env-rs` at the pinned commit `c4c45d5`). That is
the only time a shell is involved, and all it does is print the
environment.

**What a command inherits.** A child gets Banager's own environment — the
`PATH` above, which is the only variable taken from the login shell, and
whatever else Banager itself was started with — plus the variables listed
in each source's section below (`RealRunner::run` adds them with `envs` and
never clears the environment). Opened from Finder or the Dock, Banager
starts with macOS's small default environment, so a variable exported only
in a shell startup file — a proxy such as `https_proxy`, a Homebrew mirror
such as `HOMEBREW_BOTTLE_DOMAIN`, `CARGO_HOME` — does not reach the commands
Banager runs (see "Which Rust" under rustup for why that is deliberate). Its stdin is
`/dev/null`, so a tool that asks a question gets end-of-file rather than a
wait; its stdout and stderr are piped and, for a write command, streamed
line by line into the operation log. Each child runs in its own process
group. Every command has a timeout (listed below; `RealRunner` caps any
timeout at 24 hours); on timeout or cancel the whole group gets `SIGTERM`,
a grace period, and then `SIGKILL` for whatever is left.

**Where the program comes from.** At launch (`run()` in
`src-tauri/src/lib.rs`), at the start of every refresh, when the Open
Ollama button is pressed, and at the start of every Other Programs scan,
`HostEnv::discover`
(`crates/banager-core/src/runner/path_env.rs`) reads `PATH`, `HOME`,
`CARGO_HOME`, `RUSTUP_HOME`, `ZDOTDIR` and `OLLAMA_HOST` from Banager's
environment and the effective user id from the process. Homebrew's
install, uninstall and upgrade previews read six more, four to find its
`brew.env` files and two Homebrew reads from them as well, and uv's inventory and uninstall preview read
`UV_TOOL_DIR` (their sections). Every package manager
but Homebrew finds its executable with `resolve_exe`: the first directory
on that `PATH` containing a regular file of that name, or a link that
leads to one. It is looked for one step at a time (`lstat` and `readlink`,
each from the folder before it, held open; `protected::resolve`): a `PATH` folder in, or a file there that leads
into, one of the places Banager never looks into (Disk use, below: the
one list in `crates/banager-core/src/protected.rs`) is passed over as if
the file were not there, and nothing in it is read. Homebrew is looked
for at three fixed paths instead (its section), and so is a tool with its
own installer: Claude Code at `~/.local/bin/claude`, Antigravity CLI at
`~/.local/bin/agy`, Grok Build at `~/.grok/bin/grok`, rustup at
`$CARGO_HOME/bin/rustup`, Codex at `~/.local/bin/codex` and opencode at
`~/.opencode/bin/opencode` (their sections). The path that was found is
the one previewed and the one run; Codex's and opencode's are never run.
Those fixed paths are followed with `lstat` and `realpath` as they are
(`route::probe_strict`), not one step at a time: if a person has made one
of them, or a folder above it, a link into a place Banager otherwise
never looks into (`~/Documents`, iCloud Drive, `/Volumes`, ...), that
link is followed. So are three folders that are not checked against
that list either: npm's global prefix (what `npm prefix -g` printed),
whose `lib/node_modules`, `lib` or prefix Banager checks with `stat` and
`access(2)`; `$CARGO_HOME`, whose `.crates2.json` it reads; and
`~/.ollama`, whose model manifests it reads (their sections). If one of
them is, or leads into, such a place, Banager looks there.

**What a user-chosen value may look like.** A package name reaches an
argv only after `validate_package_name`
(`crates/banager-core/src/adapters/mod.rs`): `^[A-Za-z0-9@._+/-]+$`, not
starting with `-`, `/` or `.`, no `..` segment, no `.rb` suffix. Two
sources have their own rule for their own shape of input: npm's search
box (`validate_search_query`: once surrounding whitespace is trimmed,
non-empty, not starting with `-`, and at most 200 bytes of UTF-8 — a CJK
character is three of those; npm receives the query untrimmed) and
Ollama's model references, which contain a colon
(`validate_model_reference`). Every other token in every argv below is a
fixed string.

**Root.** Homebrew refuses to run as root, so Banager never runs a `brew`
command when its effective user ID is 0 (`refuse_if_root`); a Homebrew
found under root is listed as refusing, not as missing. No other source
checks.

**Passwords.** Banager never asks for a password and never handles one.
The only thing it does with one is pass `SUDO_ASKPASS` through, unchanged,
to Homebrew cask installs and upgrades when the variable is already set
in Banager's environment (Homebrew's section); it never sets it on its
own behalf. Its commands run with no terminal (stdin is `/dev/null`), so
when a cask's own step runs `sudo`, sudo cannot ask and the operation
fails. Banager recognises sudo's own words for this
(`needsPassword` in `src/lib/failureCause.ts`), and for a password window
that `SUDO_ASKPASS` opened and that got no password or a wrong one
(`passwordNotAccepted`), and the operation's log
shows the command it ran — the confirmation's command, without
`SUDO_ASKPASS` (`src/components/PasswordCommand.tsx`) — with a Copy
Command button, to be run in Terminal, where sudo can ask. Banager runs
nothing more for it and does not retry it on its own.

## When commands run

**A refresh** happens when the window opens (`refreshIntoCache(…,
"initial")` in `src/lib/events.ts`), when the user presses a Retry or
Refresh control (the status bar after a failed refresh, a source notice)
or asks to check again — the page header's Check Again, or Check Again
(⌘R) in the menu bar's View menu, neither of which starts one while one
runs (`useCheckAgain` in `src/lib/queries.ts`) — after every operation
finishes, when the "include self-updating apps" setting changes, after
Ollama is opened from its notice, whenever a `brew update` a refresh
left running in the background ends
(`refresh_on_background_change` in `src-tauri/src/ipc.rs`), and, with
Settings' daily check turned on, once a day while Banager runs (next
section). The window
opens once a launch: closing it only hides it (`src-tauri/src/window.rs`),
and bringing it back starts no refresh. Within a
refresh (`refresh_round` in `crates/banager-core/src/session/refresh.rs`)
every source's detect runs concurrently; then, for each instance found,
under that instance's lock, its inventory is read and then its update
check runs. Everything a refresh runs is in the read-only tables below,
but for Homebrew's `brew update`, which Homebrew's section lists on its
own: it updates Homebrew and its index, and when Homebrew has moved a
package this Mac has installed between a formula and a cask, or renamed
one, it can install, move or uninstall Homebrew packages by itself.
Apart from what `brew update` does, no refresh runs a write command,
moves a file, launches an application or asks for a password.

**An operation** is previewed first: `plan` builds the exact argv — or,
for an uninstall that runs no command, the exact list of paths it will
move to the Trash (the Claude Code, Antigravity CLI and Grok Build
sections) — and the front end shows it: the paths in the confirmation,
the command one press away there ("Show Command") — the variables
the plan sets on top of Banager's environment, as `NAME=value`, then the
argv (`commandText` in `src/components/CommandPreview.tsx`) — open from
the start with Settings' "Show technical details" on (`plan_operation` in
`src-tauri/src/ipc.rs`; the front end never builds an argv and sends back
only the id of a plan Rust issued). The plan can be confirmed for ten
minutes (`PLAN_LIFETIME` in `crates/banager-core/src/session/plans.rs`),
after which it has to be previewed again. The window can ask for the
preview of an upgrade or an uninstall, never of an install: no page
offers one, and `plan_operation_impl` in `src-tauri/src/ipc.rs` refuses
an install before any source is asked. Before a plan is built,
`Session::issue_plan` refuses an operation on a source that is read-only
or not answering, an upgrade or uninstall the tool itself reports it will
refuse (a pinned package), an upgrade of a package Homebrew disabled
(Homebrew's section), an update of a tool that installs its
updates itself and has no update command Banager may run (Antigravity
CLI's section), and an uninstall of a uv tool while `UV_TOOL_DIR` is set
(uv's section) — the buttons the pages hide are backed by that refusal,
not only by the page. On confirmation
`run_operation` (`crates/banager-core/src/ops/mod.rs`) takes the plan's
locks, runs the command (or moves the listed paths to the
Trash), and then re-reads the inventory to check what actually happened;
an upgrade is also preceded by a reading,
so the version before can be compared with the version after. Operations
that need the same lock start in the order they were confirmed: one waits
while an earlier one that needs any of its locks is still waiting
(`run_operation`'s queue in `crates/banager-core/src/ops/mod.rs`). An install
after which the package is not present, an uninstall after which it still
is, and an upgrade that exits 0 with the version unchanged are all
reported as needing attention, never as success. The one case with less
to go on: when the reading before an upgrade was refused — on Homebrew,
while a `brew update` a refresh left running is still going (Homebrew's
section) — there is nothing to compare, and an upgrade that exits 0 is
reported as a success whenever the package is still present afterwards,
whether or not its version moved. A command that was
cancelled or timed out, or that a signal Banager did not send ended
(killed from Activity Monitor, say), is reported as unconfirmed unless
the reading after settles it: an install after which the package is
present, or an uninstall after which it is gone, is reported as
succeeded, and one the user cancelled that did not take effect as
cancelled; an upgrade stopped partway is never settled either way
(`run_plan` in `crates/banager-core/src/adapters/mod.rs`, then
`run_operation`).

**Uninstalling several tools at once** runs nothing a single uninstall
does not. The Installed page's 「卸载所选」 previews each ticked tool exactly
as that tool's own Uninstall does: one `plan_operation` each, at most three
at a time (`PLAN_CONCURRENCY` in `src/lib/batchUninstall.ts`) — so on
Homebrew one `brew uses --installed <name>` per formula or cask (Homebrew's
section), and the same read-only look for what runs on it (What runs on a
Homebrew package, below), which leaves a package another source runs on
out. It lists the tools it will not include and why, and on
confirmation submits one uninstall per included tool, each an operation of
its own through the same queue, a Homebrew formula's ticked dependents
before the formula (`useBatchUninstall` in
`src/components/BatchUninstallSheet.tsx`). Nothing it keeps — a tool's
settings and data — is deleted: the batch has no control that deletes it.

**Quitting while an operation is under way.** Closing the window leaves
Banager and its operations running (`src-tauri/src/window.rs`). Quitting
after the question below — *Quit*, or a question the window never
showed — first cancels every operation that can be cancelled, as the
operation bar's *Stop All* does: one still queued never runs, and a
running command gets SIGTERM, then SIGKILL 5 seconds later for whatever
of it is left, which can leave the tool it was updating or uninstalling
half done. Banager quits once those commands have stopped, 7 seconds
after *Quit* at the most (`quit_now` in `src-tauri/src/quit.rs`);
another quit meanwhile — ⌘Q, the Dock's Quit, a logout — is called off,
and does not cut that wait short.
A running operation that cannot be cancelled — rustup's self update or
self uninstall — is not stopped: Banager sends it no signal, and it runs
in a process group of its own, so its command runs on without Banager.
Its output went to pipes only Banager read, which close as Banager
exits; a write to them after that fails with a broken pipe (EPIPE, or
SIGPIPE, which ends a program that does not ignore it). The question
names such an operation and says to wait for it to finish; its line
about what quitting stops leaves it out. A quit that asks nothing (below)
cancels nothing, and every command still running then runs on the same
way. So on a Mac, while an operation is not done —
queued, running, being cancelled or checking its result — every way of
quitting (Quit Banager, ⌘Q; Quit in the Dock icon's menu; logging out,
restarting or shutting down) first brings the window back and asks:
*N operations haven't finished* (「还有N个操作未完成」), with *Keep Waiting*
(「继续等待」, which has the focus, and which Escape does) and
*Quit* (「退出」), and it names an operation that has started
and cannot be cancelled, such as rustup's self update. Every one of those
quits ends in AppKit's `terminate:`, which asks the application
delegate's `applicationShouldTerminate:`; Banager adds that method to the
delegate as it starts (`guard_quitting` in `src-tauri/src/quit.rs`) and
answers it at once, so a logout, restart or shutdown is called off rather
than kept waiting, and has to be started again after *Quit*.
Nothing asks until the window has loaded and listens for the question,
nor once the page has stopped listening, as it does when an error in
drawing it takes it down; and once asked, the window has 2 seconds to
say that the question is on screen, or Banager quits — a window that
was reloaded or stopped working is not there to answer, and a quit
called off with nobody to ask would never happen. *Cancel* (or
Escape, or the question going away once everything has finished) tells
Banager too, and that 2-second wait then does not quit, even when the
word that the question was on screen did not get through; the window
sends each of the two words once more should it fail. A quit repeated
before the window has said the question is on screen asks the same
question again and starts no second wait. A refresh alone never
holds a quit (`src-tauri/src/quit.rs`, `src/lib/quit.ts`,
`src/components/QuitQuestion.tsx`). Force Quit still quits at once.

## The daily check: off unless turned on

Settings → Updates has a popup, "Check for updates" (「检查更新」):
"Manually" (「不自动检查」), "Daily" (「每天」) or "Weekly" (「每周」),
set to Manually, and so off by default (`Settings::auto_check` and `Settings::auto_check_every` in
`crates/banager-core/src/settings.rs`; a settings.json saved by the
Banager that had a "Check for updates every day" switch, on, reads as
Daily). Set to Manually, the daily check starts nothing. Set to Weekly,
everything below holds with 7 days (`auto_check::WEEKLY_DUE_AFTER_SECS`,
by `CheckEvery::due_after_secs`) in place of the 24 hours after the last
check: the looks, the retries after failed checks and the notification
are the same. Set to Daily or Weekly:

- **When.** A task Banager starts at launch (`check_automatically` in
  `src-tauri/src/auto_check.rs`) looks every 15 minutes the Mac is awake
  (`auto_check::TICK` in `crates/banager-core/src/auto_check.rs`), the
  first time 15 minutes after launch. A look starts a check only when 24
  hours (`auto_check::DUE_AFTER_SECS`) have passed on the Mac's clock since
  the last check ended, whatever started it — the one at launch, Check
  again or ⌘R, the one after an operation, the refresh a finished `brew
  update` sets off, a daily one — or when none has ended since launch
  (`auto_check::tick`, over `RoundLog::last_check_ended`). So a check of
  the user's own moves the next daily one 24 hours on, however it went,
  and a daily one in which only some sources failed counts too: a source
  that keeps failing is not asked again every 15 minutes, and the window
  shows the failure as it does after any check. A Homebrew whose `brew
  update` failed is a source that failed, for this, even when its `brew
  outdated` then answered from the catalogue it had, as it does on a Mac
  that is offline.
- **After a daily check in which every source failed.** It does not
  count (`auto_check::counts_as_check`): the check stays due, but the
  next one waits (`auto_check::retry_after_secs`, over
  `RoundLog::failed_checks`). After the first such check in a row, the
  look 15 minutes after the one that started it checks again; after each
  more, the wait from the look that started the last doubles — 30, 60,
  120 and 240 minutes — up to 360 minutes (six hours,
  `auto_check::RETRY_CAP_SECS`), where it stays. So a Mac on which every
  source keeps failing — one that stays offline, say, or has no source
  but a Homebrew that cannot update — is checked 8 times in the 24 hours
  from the first check that fails and 4 times a day after that, not at
  every look, and the daily check runs `brew update` no more often than
  that. A check that counts, a daily one or any of the user's, ends the
  waits: the next daily check that fails is followed 15 minutes on again.
  The refresh a daily check's `brew update` sets off when it ends is no
  daily check of its own, and adds no wait when every source fails in it
  (`RoundLog::record_daily`).
- **Asleep, and a clock set back.** Time the Mac spends asleep counts
  toward the 24 hours, and toward those waits, which are measured on the
  Mac's clock too, so a Mac that slept for two days checks at the first
  look after it wakes — and, should every source fail then, again 15
  minutes on, then after the waits above, until a check in which not
  every source fails. A look that finds up to a minute of a wait left
  checks all the same: the looks follow the time the Mac is awake and the
  waits its clock, which a time sync can slow or step back by a little
  (`auto_check::RETRY_SLACK_SECS`). When the Mac's clock has been set
  back to a minute or more before the last check ended, the next look
  checks as though 24 hours had passed, and when it has been set back
  that far before the look that started the last daily check that
  failed, the next look takes the wait after it to have passed; a look
  that finds the clock less than a minute before either — a small
  correction of the clock, or a check that ended as the look read the
  time — takes no time to have passed (`auto_check::SET_BACK_SLACK_SECS`).
- **Not while something is under way.** A look that finds a refresh
  running or waiting, or an operation queued, running, being cancelled or
  being verified (`Session::busy`), starts nothing; the next look asks
  again.
- **Only while Banager runs.** When the last check ended, and how many
  daily checks have failed since, are kept in memory, so after Banager is
  quit and opened again, the check at launch is the day's. Nothing checks
  while Banager is not running. Closing the window leaves Banager running
  (`src-tauri/src/window.rs`), and the task with it.

**What it runs** is the refresh Check Again runs, through the same
function (`ipc::refresh_for`), and nothing else: the commands a refresh
runs, in each source's read-only table and Homebrew's `brew update`, and
the requests a refresh makes, to the hosts in "Network". So it does to
the Mac what those commands do: Homebrew's `brew update`, when the check
runs one (Homebrew's section says when), updates Homebrew and rewrites
its local catalogue, and when Homebrew has moved a package this Mac has
installed between a formula and a cask, or renamed one, it can install,
move or uninstall Homebrew packages by itself (Homebrew's section) — and
when the check stops waiting for it, the refresh its end sets off follows
— and Grok Build's update check writes inside `~/.grok` ("Files Banager
writes"). Banager itself runs no install, upgrade or uninstall from it,
and installs none of the updates it finds: every write command runs only
after a preview the user confirmed.

**The notification.** Under the popup is a switch, "Notify me when there
are updates" (「有更新时通知我」), off by default too
(`Settings::notify_updates`), which Settings offers only while the check
is set to Daily or Weekly, and turns off when it is set to Manually. Turning it on asks for permission to
post first (`request_notification_permission` in
`src-tauri/src/notify.rs`), through the Tauri notification plugin's
`request_permission`. The plugin, at the 2.4 line `src-tauri/Cargo.toml`
pins, answers yes on a Mac without asking macOS, so there the switch
always turns on, and whether a notification shows is up to System
Settings → Notifications → Banager. Were the answer no, the switch would
turn back off with "Allow Banager to send notifications in System Settings > Notifications."
(「请在“系统设置”>“通知”中允许Banager发送通知。」) under it.

Each time the window receives a check's result — every daily check's
included, which Rust announces to it even when nothing changed
(`announce` in `src-tauri/src/ipc.rs`) — it tells Rust which updates
Update All would take, as tool-and-version pairs, and which check it was
(`report_update_set`). Rust posts one notification only when that check
was a daily one, or the refresh a daily one's `brew update` set off; both
switches are on; another app is in front, not Banager — macOS shows no
banner for a notification of the app in front, and Rust asks macOS
whether Banager is (`app_active` in `src-tauri/src/notify.rs`); and one of
the pairs has been neither in a notification nor before the user in the
focused window since Banager was opened (`notify_updates::decide` in
`crates/banager-core/src/notify_updates.rs`). A daily check that stopped
waiting for its `brew update` (Homebrew's section) posts nothing itself:
the refresh that update's end sets off is the daily check's too, and its
report decides instead, counting every update offered then — what the
check found and what the new catalogue adds (`RoundLog::awaits_follow_up`)
— so a daily check posts one notification at most. That refresh is the
daily check's whether the update ends before the rest of the check does
or after: which check it belongs to is read as its own round is recorded
(`RoundLog::record_follow_up`), after the check's. It is the window's
instead when a check of the window's reads the new catalogue before it or
shares its round (`RoundLog::record`), and the daily check then posts
nothing. A report that comes while
the window has the focus marks its pairs as seen, and posts nothing. One
that comes while Banager is in front with its window closed or in the
Dock posts nothing and marks nothing, so its updates are still news to
the next daily check that finds them.

The notification is titled Banager and says "N tools can be updated"
(「N个工具可以更新」) in the window's language, N being every update
Update All would take. It is handed to macOS's Notification Center
(`NSUserNotificationCenter`) through notify-rust, the crate the plugin
posts through, on a thread of its own (`post` and `hand_off` in
`src-tauri/src/notify.rs`): no command runs, nothing connects, and
Banager writes no file for it. That thread waits only for macOS to
confirm the delivery, two seconds at most, and learns nothing either way:
Banager is told of no delivery that failed, and hears no click on the
notification. The updates it counts are marked as told once it is handed
over, so one that macOS does not show — System Settings → Notifications
can turn Banager's off — is not posted again for the same updates. Only
when that thread cannot be started is nothing handed over: that is
logged, and the next daily check that finds those updates tries again.
What has been told is kept in memory only, so after Banager is quit and
opened again, nothing has been.

**A click on the notification** brings Banager to the front. Banager is
told only that it has come to the front, not what brought it there: it
watches for AppKit's `NSApplicationDidBecomeActiveNotification` from
launch (`observe_activation` in `src-tauri/src/window.rs`). From its
hand-over, a notification waits on the window until the window is next
in front — brought back by Banager, or given the focus any other way
(`NotificationPending`). When Banager comes to the front while one
waits, with its window closed or minimized into the Dock, it brings the
window back and tells the page to open Updates (`on_activate`, then
`open_updates` in `src-tauri/src/notify.rs`), whatever brought it there:
a click on the notification, ⌘-Tab, or its Dock icon, a click on which
Banager also hears as such and decides the same way (`on_run_event`).
With the window on screen, or nothing waiting, Banager comes to the front
as it always has. No command runs for it, nothing connects, and Banager
writes no file.

The window is given one of the plugin's commands, `is_permission_granted`
(`notification:allow-is-permission-granted` in
`src-tauri/capabilities/default.json`), which the plugin's own script
calls as the page loads, and which answers yes on a Mac. Asking for
permission and posting go through Banager's own commands, so the page
cannot post a notification itself.

## The notification when operations finish: off unless turned on

Settings → Updates has a switch, "Notify me when operations finish"
(「操作完成时通知」), off by default (`Settings::notify_operations`), and
independent of the automatic check. Turning it on asks for permission to
post as "Notify me when there are updates" does, through the same
command (`request_notification_permission`): no permission of its own.

Closing the window does not stop an operation, nor the page, which is
hidden with the window and hears the operation finish (`window.rs`). Once
every operation of a run has finished — an Update All, one update or
uninstall, and whatever was started while those were under way, as the
operation bar groups them (`trackRun` in `src/lib/operations.ts`) — the
page tells Rust how many of it worked, failed or need a look, what kind
they were, and the number of its newest operation (`report_finished_run`
in `src-tauri/src/notify_ops.rs`). Rust posts one notification only when
the switch is on and another app is in front, not Banager — never for a
run that finished while the window had the focus, which the user watched
on the operation bar — and only once for a run
(`notify_operations::decide` in
`crates/banager-core/src/notify_operations.rs`). A run whose every
operation was cancelled posts nothing. A run that finishes while Banager
is still the app in front with its window closed or in the Dock, where
macOS would show no banner, waits: it is posted when another app comes to
the front (`NSApplicationDidResignActiveNotification`,
`notify_ops::on_left_front`), together with any other run that finished
meanwhile, and dropped if the window takes the focus first, since the
operation bar then shows how it went. Where the focus is is asked off the
main thread, so a run just withheld is looked at once more on the main
thread, in order with AppKit telling of Banager leaving the front
(`notify_ops::look_again_on_main_thread`): one that finished just as
Banager left the front is still posted, and never twice.

It is titled Banager and says how the run went in the window's language:
"Updated N tools" (「已更新N个工具」) when every one worked, and otherwise
each way they ended, "N updated, N couldn't be updated"
(「N个已更新，N个未能更新」), with "N need attention" (「N个需要查看」)
for one the tool said worked and Banager could not confirm; uninstalls
say "Uninstalled" and "couldn't be uninstalled" (「已卸载」, 「未能卸载」).
It is handed to macOS the way the update notification is (`notify::post`):
no command runs, nothing connects, and Banager writes no file for it. A
click on it brings Banager to the front, and with the window closed or in
the Dock brings the window back as it was left
(`NotificationPending::set_window` in `src-tauri/src/window.rs`).

## Homebrew

Adapter: `BrewAdapter` in `crates/banager-core/src/adapters/brew/mod.rs`.
Verified against Homebrew 7.0.3 (`adapters/meta/brew.toml`).

**Detect.** Banager checks whether `/opt/homebrew/bin/brew`,
`/usr/local/bin/brew` and `/home/linuxbrew/.linuxbrew/bin/brew` exist
(`BrewAdapter::CANDIDATE_PATHS`) — never a `brew` resolved through `PATH`
— and runs `<brew> --version` (30 s) for each that does. Each is its own
instance, with the prefix two directories up from the executable.

**Environment applied to every invocation** (`BrewAdapter::ENV`),
including `--version`, `update` and every plan:

    HOMEBREW_NO_AUTO_UPDATE=1
    HOMEBREW_NO_AUTOREMOVE=1
    HOMEBREW_NO_ENV_HINTS=1
    HOMEBREW_NO_INSTALL_CLEANUP=1
    NO_COLOR=1

Install and upgrade plans additionally carry `SUDO_ASKPASS` when it is
already set in Banager's process environment (`askpass_fn`, read per
plan). It only has any effect for casks whose installer scripts invoke
`sudo`.

**Autoremove and clean-up.** After every `brew uninstall`, formula or
cask, Homebrew runs its autoremove unless `HOMEBREW_NO_AUTOREMOVE` is set:
it uninstalls the formulae that were installed only as dependencies and
that nothing installed needs any more — any on the system, not only the
uninstalled package's own (Homebrew 7.0.6-70, `cmd/uninstall.rb:129-136`,
`cleanup.rb:1038-1077`). `brew install` and `brew upgrade` both end in
`Install.finish_installation` (`cmd/install.rb:504-509`,
`cmd/upgrade.rb:363-368`, `install.rb:325-329`), which, unless
`HOMEBREW_NO_INSTALL_CLEANUP` is set, cleans up after every install or
upgrade (`Cleanup.install_clean!`, `cleanup.rb:361-389`). For the formula
the command names, and for each dependent Homebrew upgraded with it — not
the dependencies it installed or upgraded on the way
(`cmd/install.rb:437-459`, `cmd/upgrade.rb:744-775`), nor a formula
`HOMEBREW_NO_CLEANUP_FORMULAE` names (`cleanup.rb:340-346`, `:408-415`) —
it deletes its older installed versions that are not linked, pinned or
still needed and its downloads in Homebrew's cache that are outdated or
older than `HOMEBREW_CLEANUP_MAX_AGE_DAYS` days, 120 unless set
(`cleanup_formula`, `cleanup.rb:564-571`, `:736-773`;
`Formula#eligible_kegs_for_cleanup`); for the cask the command names, its
downloads there that are outdated or that old (`cleanup_cask`,
`cleanup.rb:581-588`); and then every download in the cache's `downloads`
folder that nothing in the cache refers to any more
(`cleanup.rb:705-730`). Then, when the last full `brew cleanup` Homebrew
recorded (`$HOMEBREW_CACHE/.cleaned`) is more than
`HOMEBREW_CLEANUP_PERIODIC_FULL_DAYS` days old (30 unless set), it runs
one (`cleanup.rb:418-445`): the same for every installed formula and cask
and the whole cache (`Cleanup#clean!`, `cleanup.rb:448-465`, `:473`), and
the autoremove unless `HOMEBREW_NO_AUTOREMOVE` is set (`cleanup.rb:471`).
`HOMEBREW_NO_AUTOREMOVE=1` and `HOMEBREW_NO_INSTALL_CLEANUP=1` above keep
all of it from running unless a `brew.env` file takes them back. Neither
keeps `brew update` from running `brew cleanup` itself, Homebrew's full
clean-up (`Cleanup#clean!`), when it moves an installed formula to a cask
("The index update", below). `HOMEBREW_NO_AUTOREMOVE=1` reaches that
clean-up — `bin/brew` passes every `HOMEBREW_*` variable it is started
with on (`bin/brew:310`) — so it runs no autoremove unless a `brew.env`
file takes that back.

**`brew.env`.** Homebrew's launcher, `bin/brew`, exports every
`HOMEBREW_*` line of up to three `brew.env` files over the environment it
was started with (`bin/brew:128-180`), so a line in one of them takes
either variable back. Every install, uninstall and upgrade preview reads
those files the way `bin/brew` does (`brew_env::after_brew_env` in
`crates/banager-core/src/adapters/brew/brew_env.rs`):
`/etc/homebrew/brew.env`; then `<prefix>/etc/homebrew/brew.env`; then
`$XDG_CONFIG_HOME/homebrew/brew.env` when Banager's environment sets
`XDG_CONFIG_HOME`, else `$HOMEBREW_XDG_CONFIG_HOME/homebrew/brew.env` when
Banager's environment or one of the first two files sets that, else
`~/.homebrew/brew.env`; and `/etc/homebrew/brew.env` again, last, when
`HOMEBREW_SYSTEM_ENV_TAKES_PRIORITY` is set once that file has been read.
To find them it reads `HOME`, `XDG_CONFIG_HOME`,
`HOMEBREW_XDG_CONFIG_HOME` and `HOMEBREW_SYSTEM_ENV_TAKES_PRIORITY` from
Banager's environment (`env_var_fn`, per preview), and it follows
`HOMEBREW_NO_CLEANUP_FORMULAE` and `HOMEBREW_NO_REQUIRE_TAP_TRUST` there
and in the files too. A file's lines count as
bash reads them: the last line to set a variable wins, and a last line
with no newline after it is not read. Homebrew counts
`HOMEBREW_NO_AUTOREMOVE` as unset when it is empty, only whitespace, or
`0`, `false`, `no`, `off` or `nil` in any case (`env_config.rb:871`,
`:926`), and `HOMEBREW_NO_INSTALL_CLEANUP` only when it is empty or only
whitespace. When the files leave `HOMEBREW_NO_AUTOREMOVE` unset, the
uninstall preview says that Homebrew will also remove other Homebrew
packages that were installed only as dependencies and that nothing needs
any more (`Warning::HomebrewAutoremoves`). When they leave
`HOMEBREW_NO_INSTALL_CLEANUP` unset, the install and upgrade previews say
that after installing or updating, Homebrew deletes the older versions of
this software and of any it updates along with it, and stray old
downloads, and, when its periodic clean-up is due, those of all Homebrew
software (`Warning::HomebrewPeriodicCleanup`); when they
leave both variables unset, the next line adds that the periodic clean-up
also removes those packages (`Warning::HomebrewCleanupAutoremoves`).
After those lines, when `HOMEBREW_NO_CLEANUP_FORMULAE` names a formula —
split at each comma as Homebrew splits it, nothing trimmed — one more line
names what it lists and says what Homebrew leaves out for them
(`Warning::HomebrewNoCleanupFormulae`; Homebrew 7.0.7-9,
`Cleanup.skip_clean_formula?`, `cleanup.rb:409-415`, by name or alias):
their older versions, which neither the clean-up after an install or
upgrade (`cleanup.rb:339-346`) nor the periodic one (`:453-454`) deletes,
and, where the autoremove is back, they and the formulae they need at run
time, which it keeps (`:1051-1055`). Banager changes nothing in those
files.

**The trust list.** After `brew uninstall`, Homebrew deletes the entry its
trust list holds for each package it uninstalled — a cask by its full
name, which has its tap in it unless the cask is Homebrew's own; a formula
by its tap and name — when that package's tap is not on the list itself
(Homebrew 7.0.7-9, `cmd/uninstall.rb:51-69`, `:122-127`;
`Trust.untrust!`, `trust.rb:65-90`), and writes the list only when there
was such an entry. When the list Banager read holds that entry, and no
entry that names the tap or might (one naming a tap by its remote, which
Homebrew matches against the tap's git remote, which Banager does not
read), the uninstall confirmation says Homebrew also removes the package
from its trust list (`Warning::HomebrewForgetsTrust`); when it cannot read
the list, it says nothing of it.

**What an uninstall says it removes.** Under the tool, the uninstall
confirmation says in one sentence what the command removes and what it
leaves (`Warning::UninstallScope`, built with the plan by
`BrewAdapter::uninstall_scope`). A formula's says that only this installed
version and the links to it go, and that config and data kept elsewhere
are not deleted — without "only" when the `brew.env` files bring
autoremove back, beside the line above. A cask's comes from what Homebrew
recorded when it installed the cask, which is what `brew uninstall --cask`
runs, never from `brew info`, which reads the cask's current definition
(`crates/banager-core/src/adapters/brew/cask_receipt.rs`; Homebrew
7.0.6-70, `cask/installer.rb:987-1045`): the caskfile Homebrew saved,
`<prefix>/Caskroom/<token>/.metadata/<version>/<timestamp>/Casks/<token>.json`
(of every version's, the timestamp with the greatest name), with its own
`artifacts` when it has them, else the `uninstall_artifacts` listed in
`<prefix>/Caskroom/<token>/.metadata/INSTALL_RECEIPT.json`, which also
says whether the cask has Ruby that runs before or after its uninstall
(`uninstall_flight_blocks`); a saved `.rb` caskfile is read through that
receipt. The list never holds a `pkg`, an `installer`, `stage_only` or
`generated_script`: they have no uninstall phase (`cask/cask.rb:709-732`),
and nothing but a recorded step deletes what a `pkg` or an installer put
down. So the sentence says Homebrew deletes what it installed for the cask
only when the list holds something Homebrew itself put down or linked: an
app or another artifact it moved into place (`cask/artifact/moved.rb`), a
link (`symlinked.rb`) or completions it generated
(`generated_completion.rb`). When it does, and everything else listed is
an app to quit, a folder removed only once nothing but empty folders is
left in it, a step that changes a path's owner or permissions or ends a
process, a link an install step made, or the `zap` stanza, which runs
only with `--zap`, and the receipt says there is no such Ruby, the
sentence says the cask's settings and data stay. When more is listed, it
says Homebrew deletes the files it placed for the cask — what it moved into
place, linked or generated, and its own copy and records in the Caskroom
(`cask/installer.rb:622-640`, `:642-659`, `:814-835`, `:1049-1061`), not
every file an installer beside them put down — and runs the uninstall
steps it recorded, and that nothing else is deleted: `zap` runs only with
`--zap` and the autoremove is off (`cmd/uninstall.rb:89-136`). (A cask
still installed under an old token its current definition names is
uninstalled first, all but what it shares with this one, and its Caskroom
folder deleted, `cask/installer.rb:988`, `cask/migrator.rb:24-66`,
`:85-119`: again files Homebrew placed for the cask and steps it recorded
for it.) When the `brew.env` files bring the autoremove back, that
sentence ends instead with the cask's other files staying, beside the
autoremove's own line (`UninstallScope::HomebrewCaskStepsAutoremoves`).
When the list
holds nothing Homebrew put down but does hold a step — a cask installed
with a `pkg` or an installer, such as `little-snitch@4`, whose one step
removes its background services — the sentence says Homebrew runs the
uninstall steps it recorded and that the other files its installer put on
the Mac stay. Neither of these two sentences is said when a step's
deletions are ones Banager cannot see: a program the cask names
(`early_script:`, `script:`, an uninstall step of type `run` —
`wireshark-chmodbpf`'s `early_script:` runs its vendor's uninstaller
package), Ruby that runs before or after the uninstall, or an uninstall
step Banager does not name (`move`, `copy` and `write` among them can
replace what is at their target, `install_steps.rb:1001-1215`). The record
says such a step is there, never what it deletes, so the sentence says
Homebrew deletes the files it placed for the cask, when it placed any, and
runs the uninstall steps it recorded, and that Banager can't see what else
some of those steps delete — nothing about what stays, whether the
autoremove is on or off (`UninstallScope::HomebrewCaskStepsUnseen`,
`HomebrewCaskStepsOnlyUnseen`); the step's own line below still names the
program. Three more cases say less than the record shows (Homebrew
7.0.7-9, `Cask::Installer#load_installed_caskfile!`,
`cask/installer.rb:998-1056`). A cask from a tap that is not Homebrew's
own — the receipt's `source.tap`, else the tap in its full name — whose
record is plain says Homebrew deletes the files it placed, and that its
settings and data stay, as does anything its installer put on the Mac
besides: the record cannot show a `pkg` or an installer, and none of
Homebrew's own plain casks has one, but a tap's can
(`UninstallScope::HomebrewCaskPlainThirdParty`). A cask whose saved
caskfile is Ruby (`.rb`, which Homebrew 7 saves only for a cask with Ruby
around its uninstall) is loaded as Ruby; when Homebrew cannot load it, it
rebuilds the cask from the receipt, unless the receipt or the cask's
current definition has such Ruby, and then runs the current definition
instead (`:1046-1055`, `CaskLoader.recover_from_installed_caskfile`,
`cask/cask_loader.rb:879-920`). So its sentence says Homebrew deletes the
files it placed, when it placed any, and runs the cask's uninstall steps,
that what some of them delete can't be seen in advance, and that where
Homebrew can't read the steps it recorded it uses the cask's current
definition (`HomebrewCaskRuby`, `HomebrewCaskStepsOnlyRuby`). A plain Ruby
record (what Homebrew placed and no step, as a version of Homebrew before
7 could save even for one of its own casks) names no step, so its
sentence says only that Homebrew deletes the files it placed, and that
where it can't read what it recorded it uses the cask's current
definition, whose effect can't be seen in advance
(`HomebrewCaskPlainRuby`). And while
Homebrew requires taps to be trusted — unless `HOMEBREW_NO_REQUIRE_TAP_TRUST`
is set (`env_config.rb:632-638`, `:686-695`) — a Ruby caskfile from a tap
that is not Homebrew's own and that Homebrew does not trust is not loaded
at all: Homebrew runs only the recorded artifacts that are not the cask's
`uninstall` stanza, `zap` or its steps (`cask/installer.rb:1010-1043`).
Where the trust list Banager read names neither the cask nor its tap by
name, or it could not read the list, a cask with recorded steps says
Homebrew deletes the files it placed, when it placed any, and runs the
uninstall steps only if it trusts where the cask comes from
(`HomebrewCaskStepsIfTrusted`, `HomebrewCaskStepsOnlyIfTrusted`); a plain
one says what a tap's plain cask says. A tap named on the list by name is
taken to use its usual remote, as `Tap#matches_reference?` requires
(`tap.rb:952-959`). A batch uninstall leaves each of the Ruby sentences out
for a single uninstall, as it does the sentences above that cannot see
everything. Either way, "Notes" lists one line per kind,
with what the record names, the home folder spelled `~`: paths deleted for good
(`delete:`, an `artifact` placed in the home folder, and each path an
uninstall step of type `remove` spells out — from `/` or `~`, or under the
home folder; `install_steps.rb:1049-1070`), files a `remove` step deletes
for good that Homebrew finds only as it runs it (a path under the cask's
staged folder, in each folder Homebrew looks for commands in, relative, or
with a `{{…}}` template: one line that names nothing) — and, for a `remove`
step that records a check, a line of its own that says it, since the step
deletes only the paths that pass: only where a path is a link whose target
contains the text of its `symlink_target_contains`, only where it is a
file whose contents contain the text of its `content_contains`, or where
both hold (`install_steps.rb:1051-1060`; `playdate-simulator`'s
`/usr/local/bin/arm-*` where each is a link whose target contains
`playdate`, `pycharm-edu`'s `charm` where its contents hold one given
line) — paths moved to the Trash (`trash:`), installer packages whose
every file is deleted (`pkgutil:`), programs run (`early_script:`,
`script:`, an uninstall step of type `run`), background services removed
(`launchctl:`) — counted, their labels behind the line's ⓘ, since a
label such as `com.microsoft.VSCode.ShipIt` tells a person nothing; with
no number where a label has a `*` in it, a pattern Homebrew matches
against every running service (`abstract_uninstall.rb:173-181`), so
that `adobe-creative-cloud`'s six labels and `com.adobe.CCXProcess.*`
are not said to be seven services —
kernel extensions (`kext:`), the text whose every
certificate in the keychain goes (an uninstall step of type
`delete_keychain_certificate` runs `security find-certificate -a -c <name>`
with `sudo` and deletes each certificate it lists, every one whose name
contains that text, `install_steps.rb:1179-1210`; one that also names a
`matching_certificate` file deletes only the certificate with that file's
hash, and counts among the other uninstall steps), login items
(`login_item:`), the apps quit (`quit:`, `signal:`), and, naming nothing,
Ruby blocks and other uninstall steps. An app quit is named as Finder
names it ("Visual Studio Code") when Banager finds it: an app the record
puts down (its `app` stanza's target, or its file name), where Homebrew
puts it — at that target when it is absolute or under `~`, else in
`/Applications` or `~/Applications` — whose `Contents/Info.plist` gives
the bundle id the step names (`CFBundleIdentifier`; the file is parsed,
and nothing is opened or run). An app it does not find — one kept in an
`--appdir` of its own, or a bundle id with a `*` in it — is counted
instead, its bundle id behind the line's ⓘ; a line with a `*` bundle id,
a pattern Homebrew matches against every running app (`expand_bundle_id`,
`abstract_uninstall.rb:371-384`), gives no number. When Banager finds no such
list — no Caskroom folder for the cask or one that is a link, no saved
caskfile, a legacy `.internal.json` one, a file that does not parse, or
neither `artifacts` of its own nor a receipt that lists any, when Homebrew
would read the cask's current definition — or the list holds a stanza or
directive it does not read, or it holds neither anything Homebrew put down
nor any step (an empty list, which Homebrew saves for a cask with nothing
to uninstall, `cask/installer.rb:594-607`, whatever the receipt says of
Ruby blocks, since a `.json` caskfile carries none, `:599-600`; or `zap`
alone), the sentence says only that Banager could not read from
Homebrew's records what uninstalling the cask deletes, and claims no
deletion it cannot back: with an empty list Homebrew runs no artifact's
uninstall at all (`:714-761`), and a record Banager does not read can
list anything.

**Read-only commands** (background checks; never need a password):

| Purpose | Argv | Timeout |
|---|---|---|
| Detect a Homebrew install | `<brew> --version` | 30 s |
| List installed formulae + casks (`inventory`) | `<brew> info --installed --json=v2` | 120 s |
| List outdated formulae + casks (`check_updates`) | `<brew> outdated --json=v2`, plus `--greedy` when the "include self-updating apps" setting is on | 120 s |
| Qualify the names `outdated` reported, and read which of them Homebrew disabled (once per `check_updates`) | `<brew> info --installed --json=v2` | 120 s |
| Search by name | `<brew> search {query}` | 30 s |
| Search by name + description | `<brew> search --desc {query}` | 30 s |
| List installed formulae depending on a formula (uninstall preview) | `<brew> uses --installed {name}` | 120 s |

`brew outdated` lists a formula or cask Homebrew disabled like any other —
its JSON has no field for the mark, and neither `Formula#outdated?` nor
`Cask#outdated?` looks at it (Homebrew 7.0.7) — while `brew upgrade` will
not update it: a formula fails, and a cask prints "Not upgrading …, it is
disabled" and exits 0 having changed nothing. The `brew info` reading in
the same check carries the mark (`disabled: true`), so such a row is
listed with Homebrew's word ("Disabled", 「已停用」) and no Update button, says that
Homebrew provides no more updates of it (and the replacement Homebrew
suggests, when it names one), and `Session::issue_plan` refuses its
upgrade (`UpdateBlocked::Disabled`, `BrewAdapter::check_updates`). Nothing
more runs for this, and Banager never passes `--force`. The mark is as
fresh as the last `brew update` that succeeded: when the index update
fails, `brew info` reads the catalogue already on this Mac (the same copy
`brew outdated` read), so a package Homebrew disabled since then is not
marked yet, and one it re-enabled stays held back until an index update
succeeds. When that reading fails, nothing is marked, and an upgrade is
reported as before: failed, or needing attention when the version did not
change.

`brew uses` names only formulae and casks. The uninstall preview of a
formula or cask also looks, read-only and running nothing, for the other
sources that run on it — npm on a `node`, pip and pipx's environments on a
`python@3.x`, Ollama on `ollama` (What runs on a Homebrew package, below).

**The index update** (a refresh runs it; not read-only):

| Purpose | Argv | Timeout |
|---|---|---|
| Update Homebrew and its local package index (`maybe_update`) | `<brew> update` | see below |

`brew update` fetches the newest Homebrew and the newest index of
formulae and casks, and rewrites both on disk. When anything changed, it
then carries out what the new index says has moved or been renamed, for
the packages this Mac has installed (`cmd/update-report.rb:259-261` in
Homebrew 7.0.6). Homebrew does this itself, not Banager, with no
preview:

- A cask that has moved to a formula: unless the formula is installed
  already, it installs the formula, beside the cask (`brew install
  --overwrite`, `cmd/update_report/reporter.rb:257-261`).
- A formula that has moved to a cask: when the cask's tap is on this Mac
  and Homebrew's `Caskroom` folder exists, it unlinks the formula, runs
  Homebrew's clean-up (`brew cleanup`) and installs the cask
  (`:288-295`); otherwise it prints the commands that would do it and
  runs none of them (`:301-307`).
- A formula that has moved to another tap: it taps that tap when
  Homebrew trusts it, and records the formula as that tap's (`:310-314`).
- A renamed formula or cask: it moves what is installed to the new name,
  or, for a cask whose new name is installed already, uninstalls the one
  under the old name (`migrate_formula_rename`, `migrate_cask_renames`,
  `cask/migrator.rb:61-65`).

So a refresh that runs `brew update`, the daily check's included, can
install, move or uninstall Homebrew packages, although Banager runs no
install, upgrade or uninstall of its own from it. That refresh read the
installed packages before `brew update` ran (`inventory` comes first), so
the next refresh is the first to show all it changed.

A refresh runs `brew update` for a prefix only when none is running there
and none has succeeded there in the last six hours on the clock
(`UPDATE_TTL`, counted from when that one ended,
`UpdateRecord::succeeded_at`, by `update_is_fresh`). Time the Mac spends
asleep counts toward them, as it does toward the daily check's 24 hours,
so a Mac that slept through them runs it at the first refresh after it
wakes; a clock set back to before that one ended counts as the six hours
gone, and the update that refresh runs, if it succeeds, starts them again
on the corrected clock. One that failed starts no such wait: the next
refresh runs it again. A refresh waits up
to two minutes for it (`UPDATE_PATIENCE`) and then leaves it running
rather than killing it — a `brew update` stopped halfway can leave
Homebrew's git checkout locked; only after thirty minutes
(`UPDATE_BACKSTOP`) is it stopped. While one is running, `inventory`,
`check_updates` and the uninstall preview do not read the catalogue at
all (`AdapterError::IndexUpdating`): the pages keep the previous answer
and say the index is updating, and refresh again when it ends. When one
that a refresh stopped waiting for fails, the first refresh begun after
it failed — normally the one its end sets off — reports the failure and
runs none (`UpdateRecord::unreported_failure`); the refresh after that
runs it again. So on a Mac that is offline, or whose Homebrew cannot
update (a broken git checkout, say), the six hours never begin, and
checks keep running `brew update`; the daily check spaces its own out
when every source fails ("The daily check"). A `brew update` that failed
is reported as a note on the source (the list may be out of date), not as
a failed source; only the daily check counts that Homebrew as failed,
when it decides whether it has checked ("The daily check"). The search
query passes `validate_package_name`.

**Write commands** (only run after the user reviews and confirms a plan
preview):

| Purpose | Argv | Timeout | Needs a password |
|---|---|---|---|
| Install a formula | `<brew> install --formula {name}` | 1800 s | No |
| Install a cask | `<brew> install --cask {name}` | 1800 s | Sometimes — some cask installers invoke `sudo`; `SUDO_ASKPASS` is passed through when set |
| Uninstall a formula | `<brew> uninstall --formula {name}` | 1800 s | No |
| Uninstall a cask | `<brew> uninstall --cask {name}` | 1800 s | Sometimes — Homebrew runs `sudo`, for example when the cask's recorded uninstall deletes paths (`delete:`), removes a background service (`launchctl:`) or a kernel extension (`kext:`), removes an installer package that is installed (`pkgutil:`), or runs a program the cask marks to run as root |
| Upgrade one formula | `<brew> upgrade --formula {name}` | 1800 s | No |
| Upgrade one cask | `<brew> upgrade --cask {name}` | 1800 s | Sometimes — as for install |

Every one of these argvs is exactly the verb, the kind flag and the name
(`test_plan_never_passes_zap_force_or_ignore_dependencies` in the same
file). Banager never passes `--zap`, `--force` or `--ignore-dependencies`
to Homebrew, and never runs a bare `brew upgrade`: upgrades are one
confirmed artifact per invocation. Before a write command starts,
`execute` waits up to ten minutes (`OP_UPDATE_WAIT`) for a `brew update`
still running in the background; if it is still running after that,
nothing is run and the operation is reported as failed for that reason.

An upgrade started while that `brew update` is still running also gets
no reading before: `inventory` refuses at once with `IndexUpdating`, no
`brew info` runs, and there is nothing to compare the reading after with
— so an upgrade that then exits 0 is reported as a success whenever the
package is still installed afterwards, whether or not its version moved
(the `Unknown` arm of `run_operation`). This is one way an exit-0
upgrade whose version did not move is not reported as needing attention;
Claude Code's section names another.

**Files this adapter reads.** Besides checking that the three candidate
paths exist, the uninstall preview looks at Homebrew's own update lock,
`<prefix>/var/homebrew/locks/update`, to make sure no `brew update` —
Banager's or anyone's — overlapped its `brew uses` read
(`probe_homebrew_update_lock`): the directory is `stat`ed, the file is
opened read-only, without waiting (`O_NONBLOCK`, so a named pipe there
cannot stall the preview), and never created, and when `fstat` says it is
a regular file `fcntl(F_GETLK)` asks whether the lock is held without
taking it; anything else is a lock it cannot look at. Every install,
uninstall and upgrade preview also reads the `brew.env` files named above
(`read_brew_env_file`): each is opened without waiting (links followed,
`O_NONBLOCK`, so a named pipe there cannot stall it), checked with `fstat`
once open, and read only when that says it is a regular file of at most
16 MiB (`read_file::LIMIT`) — otherwise it is skipped as unreadable;
only the lines that set `HOMEBREW_NO_AUTOREMOVE`,
`HOMEBREW_NO_INSTALL_CLEANUP`, `HOMEBREW_XDG_CONFIG_HOME`,
`HOMEBREW_SYSTEM_ENV_TAKES_PRIORITY`, `HOMEBREW_NO_CLEANUP_FORMULAE` or
`HOMEBREW_NO_REQUIRE_TAP_TRUST` are used. A cask's uninstall preview
also reads, under `<prefix>/Caskroom/<token>` — the last part of the
cask's name, and only when that is a folder and not a link
(`read_recorded`) — the names in its `.metadata` folder and in each folder
there, whether `Casks/<token>.json`, `Casks/<token>.internal.json` or
`Casks/<token>.rb` exists in the newest, then `.metadata/INSTALL_RECEIPT.json`
and, when it is the one there, the `.json` caskfile: each is opened
without waiting (links followed), checked with `fstat` once open, and read
and parsed as JSON only when that says it is a regular file of at most
16 MiB. The `.internal.json` and `.rb`
caskfiles are never opened. Every uninstall preview, formula or cask, also
reads Homebrew's trust list, `trust.json` in the folder `bin/brew` takes
for the user's Homebrew config — `$XDG_CONFIG_HOME/homebrew`,
`$HOMEBREW_XDG_CONFIG_HOME/homebrew` or `~/.homebrew`, as for the user's
`brew.env` (`trust::read_trust_list` in
`crates/banager-core/src/adapters/brew/trust.rs`): opened the same way,
without waiting, and read and parsed as JSON only when it is a regular
file of at most 16 MiB; no file is an empty list, as it is to Homebrew,
and only its `trustedtaps`, `trustedcasks` and `trustedformulae` lists
are used. Banager builds `~` from `$HOME`; for `~/.homebrew` Homebrew
takes the account's home from the user database instead
(`Trust.trust_file`, `trust.rb:27-43`), so where `$HOME` points elsewhere
the two read different files, and the uninstall confirmation may say or
leave out the trust list line wrongly. Nothing is run or changed because
of it.

## npm

Adapter: `NpmAdapter` in `crates/banager-core/src/adapters/npm.rs`.
Verified against npm 12.0.2 (`adapters/meta/npm.toml`).

**Detect.** `npm` is the first `npm` on `PATH`. Banager runs `<npm>
prefix -g` (30 s) to learn the global prefix, which is the instance's
identity, and `<npm> --version` (30 s), then asks `access(2)` whether the
current user can write `{prefix}/lib/node_modules` — or, when that does
not exist yet, `{prefix}/lib` or `{prefix}` (`real_prefix_is_writable`).
A prefix this user cannot write (a Node installed from nodejs.org's
package leaves a root-owned one) makes the instance read-only. An npm that
will not answer `prefix -g` is still listed, as not responding. When the
last check this session found exactly one npm at the same executable, and
that npm is not among this check's results, it stays that source, with what
it listed then and the updates hidden for it (`resume_unanswered_npm` in
`session/refresh.rs`); otherwise it is named by its executable, since its
prefix is unknown. No other command runs.

**Environment applied to every invocation** (`NpmAdapter::ENV`):

    NO_COLOR=1
    npm_config_update_notifier=false
    npm_config_fund=false

**Read-only commands:**

| Purpose | Argv | Timeout |
|---|---|---|
| Global prefix | `<npm> prefix -g` | 30 s |
| Version | `<npm> --version` | 30 s |
| List global packages (`inventory`) | `<npm> ls -g --depth=0 --json` | 60 s |
| List outdated global packages (`check_updates`) | `<npm> outdated -g --json` | 60 s |
| Search | `<npm> search --json --searchlimit 20 {query}` | 30 s |

`npm ls` exits 1 for non-fatal reasons (a peer dependency mismatch), so
exit 0 and 1 are both read. `npm outdated` exits 1 whenever it finds
something outdated, so a non-zero exit with findings is a result, and a
non-zero exit with nothing to show is reported as "could not check" for
every package rather than as "everything is up to date" — listing every
package that way takes one more run of `<npm> ls -g --depth=0 --json`, so
a refresh whose `outdated` failed runs the inventory command twice. The
search query passes `validate_search_query`.

**Write commands:**

| Purpose | Argv | Timeout | Needs a password |
|---|---|---|---|
| Install | `<npm> install -g {name}` | 600 s | No |
| Uninstall | `<npm> uninstall -g {name}` | 600 s | No |
| Upgrade | `<npm> install -g {name}@latest` | 600 s | No |

A plan is refused at click time if the prefix has stopped being writable
since the refresh that listed it.

npm's own package, `npm`, is never uninstalled: `<npm> uninstall -g npm`
would remove the npm every other package is updated and uninstalled with.
Its row in the list says so where Uninstall would be
(`UninstallBlocked::SourceProgram`, set by `parse_ls_global`), and both the
gate and `NpmAdapter::plan` refuse it. Its update is offered as any
package's.

Under the package, the uninstall confirmation says that its folder in
npm's global folder and its commands go, that npm runs none of its code,
and that its settings and data outside that folder are not deleted
(`Warning::UninstallScope`) — only when the npm `detect` found reports
version 7 or later (`uninstall_scope` in `npm.rs`): npm 6 ran a package's
own `uninstall` scripts. Nothing more is read or run to say it.

## pipx

Adapter: `PipxAdapter` in `crates/banager-core/src/adapters/pipx.rs`.
Verified against pipx 1.17.3 (`adapters/meta/pipx.toml`).

**Detect.** `pipx` is the first `pipx` on `PATH`; `<pipx> --version`
(30 s). No environment variables are added to any pipx command.

**Read-only commands:**

| Purpose | Argv | Timeout |
|---|---|---|
| Version | `<pipx> --version` | 30 s |
| List installed tools (`inventory`) | `<pipx> list --json` | 60 s |
| List outdated tools (`check_updates`, pipx ≥ 1.16) | `<pipx> list --outdated` | 60 s |

If `pipx list --outdated` exits non-zero, `<pipx> list --json` is run once
more so every installed tool can be listed as "could not check", with the
reason — one more process than the table shows, on that path only.

On a pipx older than 1.16, which has no `list --outdated`, Banager
instead asks PyPI about each installed tool: `GET
https://pypi.org/pypi/{name}/json` (30 s each), the name percent-encoded.
A tool PyPI does not answer for is listed as "could not check", never as
an error for the whole source. pipx has no search command Banager uses.

**Write commands:**

| Purpose | Argv | Timeout | Needs a password |
|---|---|---|---|
| Install | `<pipx> install {name}` | 600 s | No |
| Uninstall | `<pipx> uninstall {name}` | 600 s | No |
| Upgrade | `<pipx> upgrade {name}` | 600 s | No |

A tool pinned in pipx (`pipx pin`) is listed by `pipx list --outdated` as
`name [pinned]: old -> new`; its row has no Update button and gives the
command that releases the pin, `<pipx> unpin {name}`, for the user to run
— Banager never runs it. The row also says that this command unpins the
packages injected into the tool's environment as well (pipx 1.17.3's
`unpin` releases every pinned package in the environment,
`commands/pin.py:75-92`, and has no option for the tool alone). Banager
does not list injected packages (it never passes `--include-injected`),
so it says this on every pinned pipx row and never how many there are.

## uv

Adapter: `UvAdapter` in `crates/banager-core/src/adapters/uv.rs`.
Verified against uv 0.12.17 (`adapters/meta/uv.toml`).

**Detect.** `uv` is the first `uv` on `PATH`; `<uv> --version` (30 s). No
environment variables are added to any uv command, and Banager makes no
network request of its own for uv: `uv tool list --outdated` reaches PyPI
itself, under uv's own configuration.

**Read-only commands:**

| Purpose | Argv | Timeout |
|---|---|---|
| Version | `<uv> --version` | 30 s |
| List installed tools (`inventory`) | `<uv> tool list --show-paths` | 60 s |
| List outdated tools (`check_updates`) | `<uv> tool list --outdated` | 60 s |

If `uv tool list --outdated` exits non-zero, `<uv> tool list --show-paths`
is run once more so every installed tool can be listed as "could not
check", with the reason — one more process than the table shows, on that
path only. uv has no tool-search command Banager uses.

**Write commands:**

| Purpose | Argv | Timeout | Needs a password |
|---|---|---|---|
| Install | `<uv> tool install {name}` | 600 s | No |
| Uninstall | `<uv> tool uninstall {name}` | 600 s | No |
| Upgrade | `<uv> tool upgrade {name}` | 600 s | No |

**No uninstall while `UV_TOOL_DIR` is set.** uv keeps its tools in the
folder `UV_TOOL_DIR` names when it is set and not empty
(`InstalledTools::from_settings`, uv 0.12.17
`crates/uv-tool/src/lib.rs:132-140`). When `uv tool uninstall` removes
the last tool, it deletes that folder, and then the folder above it, with
every file in it, when that holds no folder but ones named `.tmp…`
(`crates/uv/src/commands/tool/uninstall.rs:40-52`,
`crates/uv-fs/src/lib.rs:795-815`). In uv's own layout that is uv's data
folder; under `UV_TOOL_DIR` it is one of the user's. It happens only when
no other uv tool is left: each tool is a folder of its own in the tools
folder (`InstalledTools::tool_dir`, `crates/uv-tool/src/lib.rs:143-144`),
named for its package, which never starts with `.`
(`crates/uv-normalize/src/lib.rs:46-49`), and uv deletes
neither folder while one that is not `.tmp…` is left there
(`uninstall.rs:40-41`, `is_temporary`, `crates/uv-fs/src/lib.rs:863-868`).
So every inventory reads `UV_TOOL_DIR` from Banager's environment, which
every uv command inherits (`tool_dir_fn` in `UvAdapter`), and while it is
set and not empty no uv tool offers Uninstall: each row says why, and
does not send anyone to run the same `uv tool uninstall` in Terminal,
where it does the same (`UninstallBlocked::UvToolDirSet`);
`Session::issue_plan` refuses the uninstall, and `UvAdapter::plan` reads
the variable again and refuses it too. Install and upgrade plan as before.

## pip (read-only)

Adapter: `PipAdapter` in `crates/banager-core/src/adapters/pip.rs`.
Verified against pip 26.2.1 (`adapters/meta/pip.toml`).

**Detect.** For each of `python3.14`, `python3.13`, `python3.12`,
`python3.11`, `python3.10`, `python3` and `python` found on `PATH`
(`PipAdapter::CANDIDATE_INTERPRETERS`), Banager canonicalises the path so
two names for one interpreter count once, and runs `<python> -m pip
--version` (30 s). Every pip instance is read-only by design. No
environment variables are added, and Banager makes no network request of
its own for pip: `pip list --outdated` reaches PyPI itself.

When that command fails, the interpreter is still listed as a source,
and Banager reads the command's error output to say which failure it was
(`says_no_pip_module`): a line that is Python's own `No module named pip`
— the interpreter has no pip module at all — is shown as "this Python
doesn't include pip", a note and no warning, since checking again changes
nothing (the `NoPip` state). Any other failure — a pip that is there but broken
(`No module named pip.__main__`), a crash, a timeout — is shown as not
responding, as before. No other command runs to tell them apart.

**The `/usr/bin` shim.** Banager takes an interpreter found in
`/usr/bin`, or one that leads there, for one of the developer-tool shims
`man xcode-select` lists — on a Mac, `/usr/bin/python3` — which run the
tool of their name from Xcode or the Command Line Tools; with neither
installed, running one opens the system's dialog offering to install the
Command Line Tools instead. So before running it, Banager asks
`/usr/bin/xcode-select -p` (10 s), which only prints the developer
directory the shims use, at most once a refresh (`PipAdapter::detect`).
It runs the interpreter only when that answer names a folder whose
`usr/bin` holds an executable file of the interpreter's name that is not
in `/usr/bin` itself — `xcode-select -p` prints a folder `DEVELOPER_DIR`
names whether it is there or not (`shim_has_tool`). Otherwise the
interpreter is skipped as if it were not on `PATH`: no pip is listed for
it, and nothing says so. The next refresh asks again, so once the tools
are installed, its pip is listed.

**Read-only commands:**

| Purpose | Argv | Timeout |
|---|---|---|
| Which developer directory the `/usr/bin` shims use (`detect`, before running an interpreter there; at most once a refresh) | `/usr/bin/xcode-select -p` | 10 s |
| Version | `<python> -m pip --version` | 30 s |
| List packages (`inventory`) | `<python> -m pip list --format=json` | 60 s |
| List packages nothing else depends on (`inventory`, to tell dependencies apart) | `<python> -m pip list --format=json --not-required` | 60 s |
| List outdated packages (`check_updates`) | `<python> -m pip list --outdated --format=json` | 60 s |

**Write commands: none.** `PipAdapter::plan` refuses every install,
uninstall and upgrade before building an argv, so no pip write command
can be previewed, let alone run; the pages show no such button for a pip
package. pip has no search command Banager uses.

## Cargo

Adapter: `CargoAdapter` in `crates/banager-core/src/adapters/cargo.rs`.
Verified against cargo 1.98.1 (`adapters/meta/cargo.toml`).

**Detect.** `cargo` is the first `cargo` on `PATH`; `<cargo> --version`
(30 s). Banager also looks for `cargo-binstall` on the same `PATH` and
remembers the path found for plans. `CARGO_HOME` is read as cargo itself
reads it: unset or an empty value means the default `~/.cargo`; an
absolute value is the Cargo home; a relative value names a folder
relative to cargo's own working directory, which Banager cannot know, so
Banager then lists no Cargo source rather than guess.

**Environment applied to every invocation** (`CargoAdapter::ENV`),
including `--version` and every plan, cargo-binstall's among them:

    RUSTUP_AUTO_INSTALL=0

On a Mac with rustup, `cargo` is rustup's own binary standing in for
cargo. Before it runs cargo it looks up the active Rust toolchain, and
when that toolchain is not installed it downloads and installs it, unless
auto-install is off — which is what this switch does. Without it, a
refresh could start that download, and so could an install, upgrade or
uninstall whose preview never mentioned it. With it, a `cargo install` or
`cargo uninstall` whose toolchain is not installed stops with rustup's
error and is reported as failed. A `cargo` or `rustc` that cargo-binstall
starts inherits the switch. A cargo that is not rustup's ignores it.

**Read-only reads.** `inventory` runs no command: it reads
`<CARGO_HOME>/.crates2.json`, the file `cargo install` keeps its records
in (a missing file means nothing is installed; the file is opened without
waiting and read only when `fstat` says it is a regular file of at most
16 MiB, and anything else is an error for the source). For each crate it also
records the program the crate installed, `<CARGO_HOME>/bin/<binary>` (the
binary named after the crate when there is one, else the first the record
lists), which the Other Programs page uses to place that program under Cargo
rather than list it. `check_updates` reads the
same file and, for each crate installed from the registry, asks crates.io
once: `GET https://crates.io/api/v1/crates/{name}` (30 s), the name
percent-encoded. Crates installed from a git repository or a local path
are never looked up; they are listed as "could not check" with that
reason. Cargo has no search command Banager uses.

**Write commands:**

| Purpose | Argv | Timeout | Needs a password |
|---|---|---|---|
| Install, cargo-binstall found | `<cargo-binstall> -y {name}` | 1800 s | No |
| Install, otherwise | `<cargo> install {name}` (previewed with a "compiles locally" warning) | 1800 s | No |
| Upgrade, cargo-binstall found | `<cargo-binstall> -y --force {name}` | 1800 s | No |
| Upgrade, otherwise | `<cargo> install --force {name}` (same warning) | 1800 s | No |
| Uninstall | `<cargo> uninstall {name}` | 300 s | No |

`--force` here is cargo's own flag, meaning "reinstall even though a
version of this crate is already installed" — it is how cargo upgrades a
binary. It is the only `--force` Banager passes to any tool, and it never
goes to Homebrew.

## Ollama

Adapter: `OllamaAdapter` in `crates/banager-core/src/adapters/ollama/mod.rs`.
Verified against Ollama 0.34.1 (`adapters/meta/ollama.toml`).

**Detect.** `ollama` is the first `ollama` on `PATH`; `<ollama> --version`
(30 s) — never `ollama list`, which on macOS launches Ollama.app as a side
effect, and a background refresh must never launch an application. The
daemon is asked over HTTP instead: `GET {host}/api/tags` (10 s), where
`{host}` is `OLLAMA_HOST` from the environment, normalised to an absolute
http(s) URL (a bare `host:port` gets `http://` in front; a value that
does not make an http(s) URL is ignored and the default used), or
Ollama's default `http://127.0.0.1:11434` (`DEFAULT_HOST`). Banager also
checks whether `/Applications/Ollama.app` or `~/Applications/Ollama.app`
is a directory: a daemon on this Mac that does not answer while the app
is there is reported as not running, with an Open Ollama button; anything
else that does not answer is reported as not responding, with no button.
No environment variables are added to any ollama command.

One `OLLAMA_HOST` survives that normalisation and is then never asked:
an `https://` `OLLAMA_HOST` is refused by the https allowlist in the
Network section, which exempts `http` only. `detect` checks the URL with
that same allowlist (`https_refused`, calling `host_allowed`) and does not
send the request at all. The notice says it was Banager that refused:
"Connecting to Ollama over https isn't supported", that `OLLAMA_HOST` is set
to an `https://` address, and, if that Ollama also answers over http, to
change `OLLAMA_HOST` to its `http://` address and quit and open Banager
again — with no Check Again and no Open Ollama button, since
both would meet the same refusal (the `HttpsHostRefused` state). Banager
still does not connect to such an address; recorded in
`docs/superpowers/backlog.md`.

**Read-only reads:**

| Purpose | Request or argv | Timeout |
|---|---|---|
| Version | `<ollama> --version` | 30 s |
| Is the daemon answering (detect) | `GET {host}/api/tags` | 10 s |
| List pulled models (`inventory`) | `GET {host}/api/tags` | 30 s |
| Is a model current (`check_updates`, per model) | `GET https://registry.ollama.ai/v2/{namespace}/{name}/manifests/{tag}` with `Accept: application/vnd.docker.distribution.manifest.v2+json` | 30 s |

For each pulled model `check_updates` reads the local manifest file
`~/.ollama/models/manifests/registry.ollama.ai/{namespace}/{name}/{tag}`
(opened without waiting, and read only when `fstat` says it is a regular
file of at most 16 MiB; otherwise the model is "could not check") and
compares its layer digests with the registry's. The three name parts
come out of the daemon's `/api/tags` answer, so before any path is built
each must be a plain path segment (`contained_manifest_path`: nothing
absolute, no `..`), and in the URL each is percent-encoded. The registry
manifest is always fetched from `registry.ollama.ai`, whatever registry
the model was pulled from. Ollama has no search command Banager uses.
From the same two manifests, and nothing else, a model with an update
also gets the most its pull can download: the sum of the `size`s the
registry manifest gives its layers and config whose digests the local
manifest does not name (`changed_blob_bytes`, the candidate's
`download_bytes`), each entry counted — a digest listed twice counts
twice, as Ollama's pull for a model with tensor layers can download it
twice. No other request is made and no other file is read for this
number: it does not use `~/.ollama/models/blobs` (only the size
measurement, under "Disk use" below, looks in that folder, for the models'
total). So a file another model shares, which `ollama pull` skips, still
counts, and the number is an upper bound, which the window words as one
("up to about 4.7 GB", rounded up). It assumes that the files the local
manifest names are on this Mac with their sizes: one missing, or of
another size, can be fetched again, and only reading `blobs` could tell. The number
is left unknown — and the window says what it said before — when either
manifest does not parse, a blob to download has no size or one that is
not a whole number of bytes, one digest is given two different sizes, or
the sum does not fit in 64 bits.

**Write commands:**

| Purpose | Argv | Timeout | Needs a password |
|---|---|---|---|
| Install (pull) | `<ollama> pull {model}` | 3600 s | No |
| Upgrade (pull again) | `<ollama> pull {model}` | 3600 s | No |
| Uninstall | `<ollama> rm {model}` | 3600 s | No |

Writes go through the CLI, not the daemon's HTTP API, so every guarantee
an operation has — the preview, the log, cancel, the check afterwards — is
the same as for every other source. A model reference whose first segment
names a registry other than `registry.ollama.ai` or `hf.co` is previewed
with a warning naming that host; it is never blocked, since `ollama pull`
is what will contact it, under Ollama's own configuration. An upgrade's
preview also says, after that warning, that it downloads the model files
that changed and can take a while (`Warning::DownloadsModelChanges`, from
`OllamaAdapter::plan`); an install's and an uninstall's do not. Where the
check above worked out the most the pull can download, the window adds
it to that note from the update's candidate — the plan itself carries
only the warning.

**The Open Ollama button** runs `/usr/bin/open -a Ollama`
(`open_ollama_app_argv` in `src-tauri/src/ipc.rs`), with its stdin,
stdout and stderr pointed at `/dev/null`, only when the user presses it and only when
Ollama.app was found; it waits up to 20 seconds for `open` to report
whether LaunchServices accepted the request. It is the one launch in the
app that is not a package-manager command, and it never happens during a
refresh.

## Claude Code

Adapter: `StandaloneAdapter` over the `CLAUDE` recipe in
`crates/banager-core/src/adapters/standalone/` (`recipes.rs` is the data,
`mod.rs` the behaviour, `route.rs` the recognition). Verified against
Claude Code 2.1.282 (the version in `adapters/meta/standalone-claude.toml`
and the name of the recorded fixture directory). The first source that is
not a package manager: the row is one tool, installed by its own installer
(`curl -fsSL https://claude.ai/install.sh | bash`, run by the user —
Banager never runs it), and the one item under it is the tool itself.

**Detect.** Banager looks at the fixed path the installer writes,
`~/.local/bin/claude` — never a `claude` found through `PATH`, which on a
Mac with the Homebrew cask earlier on `PATH` would be that copy instead —
and checks with `lstat`, `readlink` and `realpath` that it is a symbolic
link whose own text points into `~/.local/share/claude` (the installer's
`versions/<version>` store) and that resolves there; a `claude` that
reaches that folder only through another link outside it is not the
installer's layout and is not listed (the Other Programs page shows it). A
`claude` there that resolves into a `Cellar`, `Caskroom`, `node_modules`
or `corepack` directory is a package manager's copy (Homebrew's, npm's or
corepack's) and is not listed here; a plain file at that path is not this
route and is not listed either. A dangling link whose own text points
into `~/.local/share/claude` (the program files were removed by hand or
by another tool, or by an uninstall that stopped partway) is listed with
no version and a notice saying so, and its Uninstall moves the link to
the Trash (below). For a link that does resolve
into the root, Banager then runs `<claude> --version` (30 s) with
`DISABLE_AUTOUPDATER=1` in its environment: Anthropic documents that
Claude Code checks for updates on startup, and the variable as stopping
only that background check (so `claude update` is unaffected); whether
`--version` alone triggers the check was not observed, and a refresh must
never start a download, so the variable is set on every version read
regardless. The version is the first token of the first non-empty line
(`2.1.282 (Claude Code)`).

One refresh looks at the launcher twice, once to detect it and once to
list what is installed, and the disk can change in between. If the
launcher or its program files go away between those two looks, that
refresh reports Claude Code as a source it could not finish (the banner
over both pages) and keeps the previous refresh's rows rather than
listing an install that no longer matches its own row; the next refresh
lists what is there. The same happens when the second look cannot see the
launcher at all (a folder Banager may not read, a link that loops back on
itself): that is not taken as the launcher being gone. The update check
that follows in the same refresh runs nothing itself: it compares the
version the second look read.

Banager also asks where `claude` would run from if typed in Terminal (the
first regular file named `claude` with executable bits in Banager's
`PATH`, and where it resolves). When that is this copy there is no
notice. When it is another file, Banager looks on down `PATH` the same
way for a `claude` that resolves to this copy, stopping at the first
that does or at the end of `PATH`, and says so under the source:
another program named `claude` comes first — from Homebrew, from npm or
from somewhere else, by where the first one resolves — when this copy
comes later on `PATH`, or not on `PATH` when no such file is this copy —
also the notice when `PATH` has no executable `claude` at all — whether
typing `claude` then finds nothing or another program with that name.
Where a `claude` resolves does not say what program it is, so the first
of those notices says it may be another copy of Claude Code or a
different program with the same name, and neither calls it another
copy. Both looks are reads (`lstat` and `readlink`, one step at a time;
listed under Files Banager reads); that is a notice, not a command. A
`PATH` folder in one of the places Banager never reads (Which copy a
command runs, below) -- `~/Documents`, iCloud Drive, `/Volumes` and the
rest -- or a `claude` that leads into one, is not looked into: when it
comes before this copy, or could be this copy, there is no notice.

**Environment Banager adds to version reads** (`CLAUDE.version.env`;
upgrade adds no override and inherits ambient variables):

    DISABLE_AUTOUPDATER=1

**Read-only commands and requests** (background checks; never need a
password):

| Purpose | Argv or request | Timeout |
|---|---|---|
| Detect, inventory (whose reading the update check compares), and the reading before and after an update | `<claude> --version`, with `DISABLE_AUTOUPDATER=1` | 30 s |
| Newest published version (`check_updates`) | `GET https://downloads.claude.ai/claude-code-releases/latest` — or `/stable`, when `~/.claude/settings.json` sets `"autoUpdatesChannel": "stable"` | 30 s |

The pointer answers with one version number. An update is listed only when
that number is greater than the installed one, comparing the dot-separated
integers — the `stable` pointer is usually behind `latest`, so "different"
would be wrong. A request that fails, answers anything but 200, or answers
something that is not a version is listed as "could not check", never as
an error for the source, and so is an installed version that cannot be
read at that moment or cannot be compared with the published one (a
version with a suffix such as `-beta`). Claude Code updates itself in the
background when its own updater is on; the update listed is compared with
the version the launcher reported to the same refresh's inventory, so the
Installed and Updates pages show one reading, and a self-update that
lands between the inventory and the check is listed by the next refresh.

**Write commands** (only run after the user reviews and confirms a plan
preview):

| Purpose | Argv | Timeout | Needs a password |
|---|---|---|---|
| Upgrade | `<claude> update` | 1800 s | No |
| Uninstall | none: Banager moves up to three paths to the Trash itself (below) | 120 s; Banager stops between items once it is spent | No |

Banager adds no environment override to `claude update`; the runner
inherits the app's ambient environment. `DISABLE_AUTOUPDATER=1` stops the
background check, and manual updates still work with it set. Immediately
before starting it, Banager looks at `~/.local/bin/claude` once more, the
way Detect does (`lstat`, `readlink`, `realpath`; no command runs): it
must still be one link straight into `~/.local/share/claude` that
resolves there. If it has gone, dangles, is a plain file, or now points
elsewhere — at a Homebrew or npm copy, say, after a reinstall another way
since the preview — the update is not started, and the operation reports
the launcher as changed since the preview. A link Claude Code's own
updater has re-pointed at a newer version inside that folder is still
the native install, and the update runs. Anthropic's
install script stages its download under `~/.claude/downloads`, checks it
against the release manifest's checksum, and only then runs the new
binary's own `install`, which sets up the launcher (install.sh, read
directly); `claude update` itself is a compiled program whose steps were
not read, so Banager assumes nothing about what a run stopped partway
leaves behind, and its preview promises nothing. Cancel: allowed
(`KillThenReconcile`) — the runner stops the process group, Banager reads
`<claude> --version` again, and the operation is reported as unconfirmed
regardless of that reading (the same rule as every stopped upgrade). If
it exits 0 but afterwards the launcher is dangling, its version cannot
be read, or Banager cannot look at it (a folder it may not read, a link
that loops back on itself — never taken as the launcher being gone),
verification fails and the outcome is also unconfirmed. If it
exits 0 and the version did not move (Claude Code already updated itself,
or reports "up to date"), the operation is reported as needing attention
whenever a version before it could be read, as for every source. When
none could (`--version` did not answer just before the update), there is
nothing to compare, and an update that exits 0 is reported as a success
if a version can be read afterwards — even when `claude update` found
nothing to install. There is no install: the installer is Anthropic's,
not Banager's.

**Uninstall.** Claude Code has no uninstall command. Anthropic's own
instructions ("Uninstall Claude Code → Native" on
code.claude.com/docs/en/setup) are two `rm` commands; Banager runs
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
refuses the uninstall, while a `~/.claude` that is a link leaves the
download cache inside it where it is, and the preview says so; the path
must belong to the user
Banager runs as; it must be what the list describes — the
program files and the download cache real folders, the launcher one
symbolic link straight into `~/.local/share/claude`; and moving it must
not take `~/.claude` or `~/.claude.json` along (of `~/.claude`, only
`downloads` lies inside it, as listed), nor — following every link —
what either leads to, or any link or folder on the way there: a
`~/.claude.json` that is a link to a link inside
`~/.local/share/claude`, which leads on to settings kept elsewhere,
refuses the uninstall. If a
check fails on a path the list requires, the whole uninstall is refused,
in the user's language, and nothing is moved; an optional path that is
there but that Banager cannot confirm is the tool's — the wrong kind of
thing, a link elsewhere, a folder on the way that is a link — stays,
and the preview lists it among what is kept. Not yours, or would take a
kept path along, refuses whether the path is optional or not. The
preview also records what each path is — its
device, inode and kind, from `lstat` — and Banager keeps that with the
plan it issued, never sending it to the window. When the preview is
confirmed the list is built again from the disk
(`removal::execute_removal`): if a check now fails, if the list is not
the one the preview showed, or if any path is no longer the one the
preview recorded, nothing is moved — so if Claude Code updated itself
between the preview and the click (its updater re-points the launcher),
the uninstall stops and asks for a fresh look at the preview. Then,
right before each path is moved — after the pause that follows the move
before it — every check runs again on that path, and it is compared once
more with what the preview recorded; if anything differs the uninstall
stops before moving it (`Fault::PathChanged`, naming the path), and the
operation log lists every path already moved. Before the launcher, the
last, is moved, Banager also looks for every other listed path once
more: one that is there again — the program files recreated during a
pause by a Claude Code still running, say — stops the uninstall before
the launcher (`Fault::PathChanged`, naming that path), so the row stays
and a fresh preview lists what came back. Banager checks each item
immediately before moving it; a program running as you that swaps the
item in that instant could still race it. The launcher is last, so a
stop partway — macOS refusing an item (its own words are shown), Cancel,
or Banager stopping between items once the 120 s budget is spent (a move
already under way is always finished first) — always leaves it: a stop
before the first move changes nothing, and the row stays as it was; once
the program files are in the Trash, the next refresh shows the
launcher-only row, and its Uninstall lists them as already gone and
moves the rest. A Claude Code still running can put its program files or
its cache back after the launcher has gone to the Trash, and with the
launcher gone no row would show them. So once the pause after the last
move is over, Banager looks for every other path on the list once more
(`removal::left_behind`): each one that is there is named in the
operation log (`LogNote::BackAfterUninstall`) and left where it is, and
the uninstall is reported as needing attention
(`Attention::BackAfterUninstall`) — quit Claude Code, then uninstall it
again if it is still listed, or move what came back to the Trash
yourself. Then Banager looks for the launcher again and, when it is
gone, for every other path on the list (`reconcile_after_uninstall`):
the uninstall is reported as succeeded only when all of them are gone,
and as unconfirmed when Banager cannot tell (a folder it may not read,
say). Neither look counts a path the preview's own rule keeps as not
Claude Code's and that the uninstall never moved.

## Antigravity CLI

Adapter: `StandaloneAdapter` over the `AGY` recipe in
`crates/banager-core/src/adapters/standalone/` (`recipes.rs` is the data,
`mod.rs` the behaviour, `route.rs` the recognition, `removal.rs` the
uninstall). Verified against Antigravity CLI 1.2.11 (the version in
`adapters/meta/standalone-agy.toml` and the name of the recorded fixture
directory). The row is one tool, installed by Google's own installer
(`curl -fsSL https://antigravity.google/cli/install.sh | bash`, run by the
user — Banager never runs it), and the one item under it is the tool
itself.

**Detect.** Banager looks at the fixed path the installer writes,
`~/.local/bin/agy` — never an `agy` found through `PATH` — and checks with
`lstat` and `realpath` that it is a regular file: the installer copies the
binary there, and a link of that name is somebody else's (the Homebrew
cask's `agy` is a link into its Caskroom, and is Homebrew's row). There is
no launcher-only state: the file *is* the program. Banager then runs
`<agy> --version` (30 s) with `AGY_CLI_DISABLE_AUTO_UPDATE=true` in its
environment, the switch Google documents for its background updater. On
the recorded version (1.2.11, 2026-09-26), `--version` alone did not reach
the updater at all — no new log file under
`~/.gemini/antigravity-cli/log`, `updater/update_status.json` untouched, no
updater process, checked around the very read the fixture records — so
the switch is a belt on top of that; a run with a prompt is what writes a
log and starts the updater. The version is the first token of the first
non-empty line (`1.2.11`).

Banager also asks where `agy` would run from if typed in Terminal, as it
does for Claude Code, and says so under the source. That is a notice, not
a command.

**Environment Banager adds to version reads** (`AGY.version.env`):

    AGY_CLI_DISABLE_AUTO_UPDATE=true

**Read-only commands and requests** (background checks; never need a
password):

| Purpose | Argv or request | Timeout |
|---|---|---|
| Detect, and inventory (whose reading the update check compares) | `<agy> --version`, with `AGY_CLI_DISABLE_AUTO_UPDATE=true` | 30 s |
| Newest published version (`check_updates`), on Apple silicon only | `GET https://antigravity-cli-auto-updater-974169037036.us-central1.run.app/manifests/darwin_arm64.json` — the manifest the installer and the updater read; its top-level `version` | 30 s |

On an Intel Mac, or when Banager itself runs under Rosetta (it then
reports `x86_64`), no request is made: only the Apple-silicon manifest
has been fetched. The row says only that Banager could not check it for
updates; why (the check is not yet verified on Intel Macs) is shown, in
English, with "Show technical details" turned on in Settings. An
update is listed only when the manifest's version is greater than the
installed one, comparing dot-separated integers; a failed request, a
non-200 answer or a body that is not such a manifest is "could not check",
never an error for the source.

**Write commands**: none. Antigravity CLI installs its updates itself in
the background (at most every 15 minutes, by Google's documentation and
this Mac's own log), unless `AGY_CLI_DISABLE_AUTO_UPDATE=true`, the
switch Google documents for turning that off, is set where it runs; and
its `agy update` subcommand is undocumented, has no options and has never
been run — so Banager offers no Update button: a newer version is listed
with the badge "Updates when run" and a sentence that says to open the tool
once and quit it, after which it installs the new version unless its
automatic updates have been turned off. Where typing `agy` in Terminal
runs this copy (Which copy a command runs, below — judged from the folders
already read, nothing more), the sentence names `agy` as what to type;
otherwise it does not say how, and the launcher's path is shown with Show
Technical Details on. Banager does not look for that
switch, so the sentence cannot say whether it is set.
`Session::issue_plan` refuses the upgrade as well, and so does the
adapter.

**Uninstall** (only after the user reviews and confirms a preview; no
command runs): Banager moves to the Trash, in this order, any backup copy
`agy.<time>.old` the updater left in `~/.local/bin` (a regular file with
that name shape, each listed in the preview), then `~/.local/bin/agy`
itself — through the same call and the same checks as Claude Code's
uninstall ("Moving files to the Trash"). If the updater writes a new
backup or a new launcher file between the preview and the click, nothing
is moved and the uninstall asks for a fresh preview (`Fault::PathChanged`).
It keeps, and the preview says so when they exist:
`~/.gemini/antigravity-cli` (the tool's own root, where its conversations,
history, builtin skills, cache and updater state live together — no vendor
list says which of them could go alone, and the Homebrew cask's `zap`
treats it as one folder; `~/.gemini` itself is shared with Gemini CLI and
is never touched), `~/.cache/antigravity` (the installer's download staging
folder, usually empty: it sits directly in `~/.cache`, one of the folders
Banager never moves anything out of), and `~/.zshrc` and `~/.zprofile`,
where the installer adds its `PATH` line (Banager never edits a startup
file, and does not read these to find the line). The whole uninstall has
120 s, as Claude Code's does. There is no vendor uninstall document; the
list is the installer script's own path plus the cask's `zap`, and the
fixture README says so.

## Grok Build

Adapter: `StandaloneAdapter` over the `GROK` recipe in
`crates/banager-core/src/adapters/standalone/`. Verified against Grok
Build 1.0.41 (the version in `adapters/meta/standalone-grok.toml` and the
name of the recorded fixture directory). The row is one tool, installed by
xAI's own installer (`curl -fsSL https://x.ai/cli/install.sh | bash`, run
by the user — Banager never runs it), and the one item under it is the
tool itself.

**Detect.** Banager looks at the fixed path the installer writes,
`~/.grok/bin/grok`, and checks with `lstat`, `readlink` and `realpath` that
it is a symbolic link whose own text names a place inside `~/.grok` and
which resolves there — the installer's layout, a relative link,
`../downloads/grok-<version>-macos-aarch64` (`bin/agent` is a second link
to the same file). A `grok` there that resolves into a `Cellar`,
`Caskroom`, `node_modules` or `corepack` directory is a package manager's
copy and is not listed here; the Homebrew cask `grok-build` puts its links
in `/opt/homebrew/bin` and is Homebrew's row; the Homebrew *formula* named
`grok` is an unrelated library. A dangling link whose own text points into
`~/.grok` (the downloads folder was removed — by an uninstall that stopped
partway, or by hand) is listed with no version and a notice saying so, and
Uninstall removes what is left. For a link that resolves, Banager runs
`<grok> --version` (30 s) with no added environment (none is documented).
Whether `--version` runs grok's launch-time updater, and whether that
updater installs or only checks, are both unverified; on the recorded
version (1.0.41, 2026-09-26) `--version` left `~/.grok/bin`,
`~/.grok/downloads` and `~/.grok/version.json`'s timestamp unchanged and
wrote nothing under `~/.grok`, as the fixture README records around the
very read it holds. The version is the second token of the first
non-empty line (`grok 1.0.41 (4220f3b224a6)`).

Banager also asks where `grok` would run from if typed in Terminal and
says so under the source, as it does for Claude Code. The `grok` of the
formula above, and that of npm's package `grok-cli` (a third-party
wrapper), are not Grok Build. Banager tells where a `grok` resolves — a
Homebrew directory, an npm one or anywhere else — not which program it
is, so the notice calls a `grok` that comes first another program with
that name, which may or may not be Grok Build, and never another copy.
That is a notice, not a command.

**Read-only commands** (background checks that never need a password.
Grok's `--help` says its own check installs nothing, though the check
writes inside `~/.grok`, below; whether `--version` reaches grok's
launch-time updater, and whether that updater installs or only checks,
are both unverified, above):

| Purpose | Argv | Timeout |
|---|---|---|
| Detect, inventory (the version the update check lists as current), and the reading before and after an update | `<grok> --version` | 30 s |
| Newest published version (`check_updates`) | `<grok> update --check --json` — grok's own check; its `--help` describes `--check` as "Check for updates without installing" | 60 s |

Grok's own check prints one JSON object; Banager believes its
`updateAvailable` and shows its `latestVersion`, comparing nothing itself
(the channel is the tool's own, "Native"). A check that exits non-zero,
prints something that is not that JSON, does not finish in 60 seconds, or
answers with a non-null `error` field (grok could not find out — say,
offline) is "could not check" with a short reason, never "up to date" and
never an error for the source. The reason quotes grok's `error` text when
it gave one, and says so when Banager could not run the check, when it did
not finish in 60 seconds, or when it did not print that JSON; any other
end than exit code 0 is worded as every other lookup that runs a command
words it: the first line of grok's stderr or, when there is none, how the
check ended (that `grok update --check --json` exited with code 1, say).
Banager makes no network request of its own for grok; the check's
connection is grok's, under grok's
configuration (`~/.grok/config.toml`, which Banager does not read). The
check writes inside `~/.grok` each time it runs, so every refresh causes
those writes — grok's, not Banager's ("Files Banager writes"). On the
recorded run (2026-09-26) it replaced `~/.grok/version.json`, whose
`checked_at` became the time of the check; added two lines to grok's own
log, `~/.grok/logs/unified.jsonl`, recording that it loaded its saved
login (`~/.grok/auth.json`, which Banager never reads); and touched the 27
files of the user guide grok ships, `~/.grok/docs/user-guide` (their
modification times moved; no file was added or removed). Whether grok
installs updates on its own (`auto_update = true` means "check for updates
on launch") is unverified, so the row is not described as self-updating.

**Write commands** (only run after the user reviews and confirms a plan
preview):

| Purpose | Argv | Timeout | Needs a password |
|---|---|---|---|
| Upgrade | `<grok> update` | 1800 s | No |
| Uninstall | none: Banager moves up to eight paths to the Trash itself (below) | 120 s; Banager stops between items once it is spent | No |

`grok update` downloads the new version into `~/.grok/downloads` and
re-points the `bin/` links, leaving the old download in place (the
installer's layout; the update's own steps were not read). Immediately
before starting it, Banager looks at `~/.grok/bin/grok` once more, the way
Detect does (no command runs): it must still be one link straight into
`~/.grok` that resolves there; if it has gone, dangles, is a plain file or
now points elsewhere, the update is not started, and the operation reports
the launcher as changed since the preview. Cancel: allowed
(`KillThenReconcile`) — the runner stops the process group, Banager reads
`<grok> --version` again, and the operation is reported as unconfirmed
regardless of that reading. An update that exits 0 with the version
unchanged is reported as needing attention, as for every source. **How
`grok update` behaves when nothing can answer a prompt (Banager gives it
no terminal and a closed stdin) has not been observed by this project**;
the author records it on a CI runner before this step merges, and this
paragraph then says what was seen.

**Uninstall** (only after the user reviews and confirms a preview; no
command runs): Banager moves to the Trash, in this order,
`~/.local/bin/grok` and `~/.local/bin/agent` when the installer made them
(it does so only when `~/.grok/bin` was not on `PATH`; they go first,
while every folder their link text could pass through is still there —
what that text says has not been checked on a Mac that has them),
`~/.grok/downloads` (the program: every downloaded version),
`~/.grok/bundled` and `~/.grok/completions` (the vendored agents and shell
completions, when present), `~/.config/fish/completions/grok.fish` (when
present), then the two links the installer put in `~/.grok/bin`:
`~/.grok/bin/agent` (when present) and last `~/.grok/bin/grok`, the
command itself. The folder `~/.grok/bin` is not moved: the installer put
it on your `PATH`, so a script of your own may be in it, and it stays
inside `~/.grok`, empty unless you put something there. Each path passes
the checks Claude Code's section describes, and the three links besides
`~/.grok/bin/grok` pass one more: `~/.grok` is also the folder this
uninstall keeps, so pointing into it does not make a link grok's. Each
must lead to grok's program — its own text pointing into
`~/.grok/downloads` or at `~/.grok/bin/grok` or `~/.grok/bin/agent`, and,
if it still leads somewhere, leading into `~/.grok/downloads` or to the
very file `~/.grok/bin/grok` runs. An optional path Banager cannot
confirm is grok's own — a `~/.local/bin/agent` that belongs to another
program, say, or a link of yours to a plugin's or a skill's program
inside `~/.grok` — stays and the preview says so. The launcher is last: once
`~/.grok/downloads` is in the Trash, a run that stops leaves a
launcher-only row that a second Uninstall finishes, as for Claude Code.
The whole uninstall has 120 s, as Claude Code's does. It keeps `~/.grok`
itself — `config.toml`, `auth.json` (the login), `sessions/`, `memory/`,
`skills/`, `plugins/` — and `~/.zshrc`, where the installer wrote its
marked block. A `/usr/local/bin/grok` or `/usr/local/bin/agent` is outside
your home folder, so Banager never touches it: when it is a link into
`~/.grok` that leads nowhere once the paths above are in the Trash — the
installer's fallback, to grok's download or through `~/.grok/bin/grok`,
or one that leads nowhere already — the preview says it becomes a dead
link; when it is something else (Homebrew's `grok-build` link on an Intel
Mac, another program's `agent`, or a link to a plugin's program in the
`~/.grok` this uninstall keeps, which still works afterwards), the preview
says nothing about it. There is no vendor uninstall document and no
`grok uninstall`; the list is grok's own README ("File Locations") plus
its install script, and the fixture README says so.

`~/.grok` is also the folder the table names for Grok Build
(Data an uninstall leaves behind, below). This preview names it once, in
its own list; the preview of uninstalling Homebrew's cask `grok-build`
names it there instead, measured by names and sizes alone and without
`~/.grok/downloads`, this install's program.

## rustup

Adapter: `StandaloneAdapter` over the `RUSTUP` recipe in
`crates/banager-core/src/adapters/standalone/` (`recipes.rs` is the data,
`rustup.rs` what its uninstall does, when Banager may offer it, and what to
say about it). Verified against rustup 1.29.1 (the version in
`adapters/meta/standalone-rustup.toml` and the name of the recorded fixture
directory). The Rust toolchain installer, installed by its own script
(`curl … https://sh.rustup.rs | sh`, run by the user — Banager never runs
it); the one item under it is rustup itself. The toolchains it manages, and
the programs `cargo install` installs, are not rows of this source: the
first are outside phase 4, the second are Cargo's.

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
keeps the rows it has until the operation ends (`Session::refresh_round`).
The check is made once, at the start of a refresh; an operation that
starts in the seconds after it may overlap one version read that was
already under way.

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
says so; the operation bar offers no Stop; while it is still queued it
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
`~/.cargo` and `~/.rustup`, `~/.cargo` is a real folder and not a link,
`~/.rustup` is a real folder, not a link, or not there yet, and nothing
directly inside either folder is a link. Any other layout — a custom
folder, a relative variable, a linked folder, a link at the top of one —
gets no Uninstall button and a badge saying it cannot be uninstalled here:
rustup's uninstall deletes both folders whole, wherever they point, and it
reaches `~/.rustup/toolchains/<name>`, `~/.rustup/update-hashes/<name>` and
`~/.cargo/bin/<name>` through their parent folder, so a link at one of
those three names would have it delete the contents of wherever the link
leads; every other link at the top of either folder it unlinks without
following, and Banager refuses at any of them rather than keep a list of
the names rustup follows. Banager will not ask it to delete a place the
preview did not name. The same question is asked of the disk again right
before the command is started (`StandaloneAdapter::execute`): a folder
that has since become a link, or been replaced, or a link that has since
appeared at the top of one, stops the run before anything is spawned, and
the operation log names it. Read from rustup 1.29.1's source (`uninstall()` in
`src/cli/self_update.rs`, lines 924–1032 at tag `1.29.1`), it removes,
**permanently — nothing goes to the Trash**: every installed toolchain;
`~/.rustup` entirely; the line it added to your shell startup files
(below); everything in `~/.cargo` except `bin/` — the registry and git
caches, `.crates2.json`, and also Cargo's own `config.toml` and
`credentials.toml` (the crates.io login) and `env`; everything in `bin/`
whose name is not rustup's or one of its thirteen links' — that is,
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
`~/.cargo/.crates2.json` records — the same file the Cargo source reads —
each recorded one by its crate's name, the one its row has on the
Installed page, and the others by their file names);
that rustup will edit your shell startup files; and each startup file that
will still speak of Cargo's env file afterwards. It is not cancellable once
running, for the same reason as the update, and holds the same two locks
(it deletes the record the Cargo source's inventory reads). Afterwards
Banager looks for `~/.cargo/bin/rustup` again and reads no version: an
exit 0 with it gone is reported as succeeded, an exit 0 with it still there
as needing attention, and a run stopped by the timeout is judged by the
same look — gone is succeeded, still there is unconfirmed.
`--no-modify-path` is not passed: rustup removing its own line beats
leaving one that makes every shell reading that file print an error.

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
the Cargo home is not `~/.cargo`), from `~/.profile`, `~/.bash_profile`,
`~/.bash_login`, `~/.bashrc`, `$ZDOTDIR/.zshenv` and `~/.zshenv`, in that
order, and then the two lines rustup wrote before version 1.23 from
`~/.bash_profile`, `~/.profile`, `$ZDOTDIR/.zprofile` and `~/.zprofile`
(`shell.rs` and `unix.rs` under `src/cli/self_update/`, tag `1.29.1`). Each
visit removes the first line that matches byte for byte, newline included,
by rewriting in place the file the visited name leads to (`utils/raw.rs`,
lines 86–98); when `ZDOTDIR` is your home folder the same file is visited
twice and two copies go. It never visits `~/.zshrc` or fish's
`config.fish`. So before the uninstall Banager reads those eight files —
`~/.zshenv`, `~/.zprofile`, `~/.zshrc`, `~/.bash_profile`, `~/.bash_login`,
`~/.bashrc`, `~/.profile`, `~/.config/fish/config.fish` — and, when
`ZDOTDIR` names a folder other than your home, that folder's `.zshenv`,
`.zprofile` and `.zshrc` as well (each named by its own path,
`~/.config/zsh/.zshrc` for one), replays rustup's removals on copies in
memory — one copy per file, however many of those names lead to it, so
when two names lead to one file (`~/.zshrc` a link or a hard link to
`~/.zshenv`, say, or a `ZDOTDIR` that is a link to your home) a line
removed through one name is gone under the other, and a visit through
each name removes one copy — and names each file that still speaks of
Cargo's env file, under every one of those names that leads to it. The
preview does not say which shells read which file, only what a shell that
reads it will meet: "will print an error" when what is left is a line in
the exact form rustup itself writes (in a file rustup does not edit —
`~/.zshrc`, unless it is another name for a file rustup visits; a second
copy of its line; its line last in the file with no newline after it) and
every line above it stands alone. Banager reads each of those lines with
sh's quoting and lets it stand alone only as a whole command that ends on
that line: no quote, `(`, `{`, `$(` or `${` left open, nor a `)` or `}`
that does not match the innermost one still open on it; no `<<` outside
quotes (a here-document, whose body is the lines below); no `\` at its
end; no `\` inside single quotes (sh and fish read it differently). The
rest of the check reads the line with its quote marks and escaping
backslashes left out, so a quoted word counts as the word: no `(` and `)`
with only blanks between them, as a function definition has; no `[[`
without a `]]` after it, nor a `]]` without a `[[` before it; no `|`,
`&&` or `|&` at its end, nor `and`, `or`, `not` or `!` as its last word;
and none of `if`, `then`, `elif`, `else`, `fi`, `case`, `esac`, `for`,
`select`, `while`, `until`, `do`, `done`, `repeat`, `foreach`,
`function`, `coproc`, `begin`, `end`, `switch`, `return`, `exit`,
`logout`, `bye` or `exec` as a word anywhere on it — the words of
conditionals, loops, functions, blocks and coprocesses, and of the
commands that end the file or the shell. In that check the rest of a
line from a `#` that follows a space or tab, outside quotes and outside
`${…}`, is a comment and is not read. This is a small reader, not a
shell: it looks only for what is listed here; a line that does not stand
alone — even a block that closes before rustup's line — makes every line
below it "may"; and what a command above the line does when it runs,
such as a file it loads, a string it evaluates, or an alias or option it
sets, is not followed. "May" is for everything else that mentions the env
file and that rustup will not remove: rustup's own line inside an `if`, a
function or a here-document, or below a line that does not stand alone; a
guarded line such as `[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"`;
an `echo`; another spelling such as `source ~/.cargo/env`; a
`$CARGO_HOME/env`. A line that is only a comment counts for nothing.
rustup learns `ZDOTDIR` by asking `zsh` when your login shell is not zsh;
Banager runs nothing and reads only the variable it was started with, so
a `ZDOTDIR` set only inside a zsh startup file is not modelled, and a zsh
whose files live under such a `ZDOTDIR` is not read.

## Codex

Adapter: `StandaloneAdapter` over the `CODEX` recipe in
`crates/banager-core/src/adapters/standalone/` (`recipes.rs` is the data,
`release_link.rs` the version read, `route.rs` the recognition). Listed
only: **no command runs for it, ever** — not `codex --version`, not an
update check, not an update, not an uninstall. The row is Codex installed
by OpenAI's own script (`curl -fsSL https://chatgpt.com/codex/install.sh |
sh`, run by the user — Banager never runs it); what the recipe expects
was read from that script as text on 2026-10-01 (the fixture README,
`adapters/fixtures/standalone-codex/install-script-2026-10-01/README.md`,
names the lines). npm's `@openai/codex` and the Homebrew cask `codex` are
other paths and stay npm's and Homebrew's rows; the AI Tools filter puts
all three in one family.

**Detect.** Banager looks at the fixed path the installer writes,
`~/.local/bin/codex` — never a `codex` found through `PATH` — and checks
with `lstat`, `readlink` and `realpath` that it is a symbolic link whose
text and final target are both inside `~/.codex/packages/standalone` (the
script links it to `…/packages/standalone/current/bin/codex`). A link into
`node_modules` or Homebrew's `Caskroom` is not this row. Only the default
folders are looked at: `CODEX_HOME` and `CODEX_INSTALL_DIR` are not read,
because a Mac app started from the Finder inherits no variable from your
shell except the `PATH` Banager asks your login shell for, so a
`CODEX_HOME` set in `~/.zshrc` is invisible to it. A Codex installed under
another `CODEX_HOME` is not listed here (the Other Programs page lists its
launcher instead). Nothing else in `~/.codex`, with your settings, login
and sessions, is looked at to list this row or read its version, and no
file of yours there is ever opened. Only the preview of uninstalling a
Codex -- npm's `@openai/codex`, Homebrew's `codex` -- walks `~/.codex` to
say how much it takes, by names and sizes alone (Data an uninstall leaves
behind, below).

**Version, with no command.** `readlink` and `realpath` of
`~/.codex/packages/standalone/current`, which the installer points at
`~/.codex/packages/standalone/releases/<version>-<target>`: the version is
that folder's name less `-aarch64-apple-darwin` or `-x86_64-apple-darwin`.
A missing, dangling or unexpected link gives no version (the row is
listed with its version unknown, and is not marked as not responding,
since nothing was asked). Then one small file,
`~/.codex/packages/standalone/auto-update-version` (at most 256 bytes,
only when it is a regular file; opened without waiting and without
following a link, then checked with `fstat`): the installer writes the release's name
there when it installs the latest release, and its scheduled updates run
only while that file names the release in use. When it does, the install
follows Codex's latest release and the row says Codex can update itself
(whether Codex's own updater is running is not something Banager reads);
otherwise it says nothing about updates.

Banager also asks where `codex` would run from if typed in Terminal, as it
does for Claude Code, and says so under the source and in the details'
"In Terminal" group — that is reading `PATH` folders, not a command.

**Read-only commands and requests**: none. Codex's newest version comes
from `releases.openai.com` or GitHub, which are not hosts Banager
connects to, so its updates are not checked at all: the Updates page lists
nothing for it, and the Installed page does not call it up to date.

**Write commands**: none. No Update button (Banager must not re-run the
installer's `curl | sh`), and no uninstall: the row says "Listed only"
(「只列出」). Moving `~/.local/bin/codex`,
`~/.local/bin/codex-code-mode-host` and `~/.codex/packages/standalone` to
the Trash, keeping the rest of `~/.codex`, is a decision for the author
(D5) and is not built.

## opencode

Adapter: `StandaloneAdapter` over the `OPENCODE` recipe in
`crates/banager-core/src/adapters/standalone/` (`recipes.rs` is the data,
`route.rs` the recognition). Listed only, like Codex: **no command runs for
it, ever** — not `opencode --version`, not an update check, not an update,
not an uninstall. The row is opencode installed by its own script (`curl
-fsSL https://opencode.ai/install | bash`, run by the user — Banager never
runs it); what the recipe expects was read from that script as text on
2026-10-01 (the fixture README,
`adapters/fixtures/standalone-opencode/install-script-2026-10-01/README.md`,
names the lines). npm's `opencode-ai` and the Homebrew formula `opencode`
are other paths and stay npm's and Homebrew's rows; the AI Tools filter
puts all three in one family.

**Detect.** Banager looks at the fixed path the script writes,
`~/.opencode/bin/opencode` — never an `opencode` found through `PATH` —
with `lstat` and `realpath`: the script moves one executable there, so a
regular file is this row, and a link at that path is not.

**Version: not read.** The script writes nothing that names the version
(it learns an installed version only by running `opencode --version`,
which Banager does not do), so the row is listed with its version unknown,
and not marked as not responding, since nothing was asked. The
`package.json`, `package-lock.json` and `node_modules` opencode itself may
keep in `~/.opencode` name a plugin package, not the program, and are not
read. opencode's documentation says it downloads its updates itself when
it starts, unless its `autoupdate` setting turns that off; Banager does not
read that setting, so the row says it updates itself by default.

Banager also asks where `opencode` would run from if typed in Terminal, as
it does for Claude Code, and says so under the source and in the details'
"Typed in Terminal" group — that is reading `PATH` folders, not a command.

**Read-only commands and requests**: none. opencode's newest version comes
from GitHub, which is not a host Banager connects to, so its updates are
not checked at all: the Updates page lists nothing for it, and the
Installed page does not call it up to date.

**Write commands**: none. No Update button (Banager must not re-run the
script's `curl | bash`), and no uninstall: the row says "Manual uninstall".

## Unknown-source scan (phase 4, step F): read-only, no command runs

The *Other Programs* page -- the last row under the sidebar's *Sources* --
lists command-line programs that none of the sources above installed.
Producing that list runs no command at all. `scan_unknown`
(`crates/banager-core/src/scan/mod.rs`) reads directory entries and file
metadata and nothing else:

| It looks at | How |
|---|---|
| `~/.local/bin`, `~/bin`, `/usr/local/bin`, `~/.cargo/bin` (and `$CARGO_HOME/bin` when that variable is set), `~/go/bin`, `~/.bun/bin`, `~/.deno/bin`, plus every `PATH` entry under your home folder (`candidate_dirs`) | where the folder leads, found one step at a time from `/` (`protected::resolve`), then the folder listing (`readdir`), one level deep, from a descriptor held open on that very folder (`dirfd`) — a subdirectory is never entered; a directory that does not exist, or that cannot be reached or listed, is skipped silently; two names for one directory are read once (`scan_dirs`) |
| each entry | `lstat` and `readlink` of the entry, asked of that held folder; where a link leads, found one step at a time as `realpath` would; and the size, date and permissions of what it leads to (`examine`): what kind of file it is, where a link points, its size and date, who owns it. A file with no execute bit is not listed. Nothing's *contents* are read, and `file(1)` is not run. A broken link, while a source's own executable is a link that leads nowhere too, also gets `lstat` and `readlink` (each asked of the folder it is in, held open) of the folders and links its text leads through, to see where it would lead (`dead_end`) |

Like the command check and the disk-use measurement, this scan never
reads into `~/Desktop`, `~/Documents`, `~/Downloads`, `~/Pictures`,
`~/Movies`, `~/Music`, `~/Library/Mobile Documents` (iCloud Drive),
`~/Library/CloudStorage`, `~/Library/Containers`,
`~/Library/Group Containers` or `/Volumes` (every other disk),
whatever case spells them -- ASCII letters in either case, and the
characters a Mac's disk takes for ASCII letters: `ſ` (long s) for `s`,
`K` (Kelvin sign) for `k`, `ß` and `ẞ` for `ss`, and the ligatures `ﬀ`
`ﬁ` `ﬂ` `ﬃ` `ﬄ` `ﬅ` `ﬆ` for their letters (`protected::AS_ASCII`, found
by asking the disk for every character); any other letter only as
spelled -- and also when spelled from `/System/Volumes/Data`
(`protected::DATA_VOLUME`): it is the same list
(`crates/banager-core/src/protected.rs`), so scanning never makes macOS
ask for permission. Nothing in these places is listed, `lstat`ed, read as
a link or resolved, as named or where it leads (`protected::resolve`):
each step on the way to a folder to scan, to where an entry leads and to
a source's own folder is checked against them before it is taken, from
the folder before it, held open; an entry of a scanned folder that is
itself one of these places (`Documents` in a home folder that is on
`PATH`, `Containers` in a scanned `~/Library`) is passed over without even
an `lstat`; and each scanned folder is listed, and
its entries looked at, from a descriptor held open on it, so a folder
replaced by a link while it is read is never followed.

- A folder to scan that is in one of these places, or leads into one (a
  `PATH` entry in `~/Documents`, a `~/bin` that is a link to the Desktop,
  a `$CARGO_HOME` on another disk), is not read. The page says how many
  folders it left unread (「有2个文件夹在受保护的位置，没有读取。」), and,
  with Show technical details on, names them behind an ⓘ
  (`UnknownScan.protected_dirs`).
- A program in a scanned folder whose link leads into one of these places
  is listed by its own name, and what it leads to is not followed -- so
  nothing tells whether it is a program at all: a link into a project
  folder or a data file there is listed too, where the same link outside
  these places would be dropped. The page says 「指向受保护的位置」
  ("Points into a protected place") where a size and a date would be, shows
  no path, and its Show in Finder is off (`EntryKind::ProtectedSymlink`).
  The link's own text is kept, and used only to find the app it points
  into.
- Which source a program belongs to is still decided for a path that
  leads into these places, by name alone: the path as far as the links
  outside them lead, the rest as written (a `..` in it folded by name),
  compared with each source's own
  executable, the paths it reported installing and the folders it owns,
  which are found the same way and never entered either (`Known::index`).
  So a Homebrew installed on another disk (`/Volumes/<disk>/homebrew`)
  keeps its programs: a link into its `Cellar` is Homebrew's, by name, and
  is not listed. A path that a `..` takes back out of a protected place
  (`~/Documents/../.local/...`, where `~/Documents` may itself be a link
  elsewhere) is compared with nothing, and the program is listed.
- A folder that cannot be listed (one locked with no permissions) is
  skipped, not reported as read, and left as it is.

It stops after 2000 entries or 10 seconds (`ScanBudget::default`) and
says so on the page, with the number it stopped at. It never runs, opens,
moves or deletes anything it finds. It takes no lock and is not part of a
refresh (`Session::scan_unknown` in
`crates/banager-core/src/session/scan.rs`): it runs when the page opens
(from the sidebar, or Other Programs, ⌘4, in the menu bar's View menu),
again when the sources' state changes while the page is open, and when
you press *Scan Again* — always against the sources' last known state —
and its result is not stored.

A program is *not* listed when a known source accounts for it
(`Known::claimant`): it is a source's own executable, or resolves to the
same file one does (`~/.cargo/bin/cargo` and rustup's other proxies all
resolve to `rustup`), or — a link that leads nowhere — would lead to the
same missing file as a source's own executable that leads nowhere too
(Grok Build's `~/.grok/bin/agent` beside its launcher once
`~/.grok/downloads` is gone: the Uninstall that finishes that state moves
both); it resolves under a path a source reported installing (a file or
a directory: a uv or pipx tool's shim resolves into that tool's
environment, and a Homebrew cask's command in `<prefix>/bin` — `code`,
`docker` — resolves into the `.app` the cask moved into `/Applications`,
which `brew info --installed --json=v2` names beside the cask's `app`
stanza); or it resolves under a directory a source owns
(`owned_roots`: Homebrew's `Cellar`, `Caskroom` and `opt`; npm's
`lib/node_modules` under its global prefix; Ollama's `~/.ollama`; Claude
Code's `~/.local/share/claude`; Antigravity CLI's
`~/.gemini/antigravity-cli`; Grok Build's `~/.grok`; Codex's
`~/.codex/packages/standalone`; opencode's `~/.opencode`; and uv's
`~/.local/share/uv/python`, where `uv python install` puts the Pythons it
manages, so its `~/.local/bin/python3.12` and the like are uv's, not
listed). uv's folder is the default its docs give
(docs.astral.sh/uv/reference/storage, "Python versions"):
`UV_PYTHON_INSTALL_DIR` or `XDG_DATA_HOME` would move it, but Banager is
handed only the shell's `PATH`, so a Python installed elsewhere is listed.
A regular file in a tool's own bin directory whose name is one of the
backup patterns that tool's recipe declares — `agy.<time>.old` in
`~/.local/bin`, the copies Antigravity's updater leaves — is that tool's
while the tool is installed (`Recipe.backup_globs`, rule 4); once the tool
is gone the pattern goes with it and such a file is listed.
Everything else is listed, with where a broken link pointed, the app a
program runs inside, and whether an installer with administrator rights
put it there.

Banager reads one path per cask, the first `app` stanza's, and also the
links the cask's `binary` stanzas put on the disk (the absolute `target`
`brew info --installed --json=v2` writes beside each stanza): an entry
that *is* one of those links, and resolves into the file the stanza
names, the cask's folder in `Caskroom`, or that `.app`, is the cask's
(the same rule as "Which copy a command runs") — so a command inside a
second `.app` of the same cask, or of a cask whose `app` entry carries no
absolute `target`, is not listed. A command that no `binary` stanza
names and that lives neither inside that `.app` nor under `Caskroom`
(one a `pkg` put on the disk) is still listed here although Homebrew
installed it, and so is a link of a stanza's name that leads somewhere
else.

Each row's ⋯ menu has *Show in Finder* and *Copy Path*. Show in Finder
asks Finder to show the program and runs nothing else: no command runs
for it. The window hands the path the scan resolved for that row
(`UnknownEntry.resolved`, every link followed) to Banager's own
`reveal_in_finder` command (`revealInFinder` in `src/lib/api.ts`,
`src-tauri/src/reveal.rs`), which accepts only a path the newest scan
handed to the window resolved, and refuses any other before anything is
read. It passes that path to the Tauri opener plugin's
`reveal_item_in_dir` function (tauri-plugin-opener 2.5.5, the version in
`Cargo.lock`), which resolves it again (`realpath`) and makes one call,
`NSWorkspace activateFileViewerSelectingURLs:`, with which Finder opens a
window on the program's folder with the program selected. So for a link
Finder shows the file the link points to; a broken link's is gone, and on
its row the item is off, as it is on the row of a link into a protected
place, which the scan did not follow. The window may call none of the
plugin's commands itself (Network, below). Copy Path puts the path the row
shows, `~` and all, on the clipboard (`useCopyCommand` in
`src/lib/clipboard.ts`), and does nothing else.

## Which copy a command runs: read-only, no command runs

A tool's details on the Installed page say, for each command it puts on
the Mac, what typing that name in Terminal runs: this copy, another file
that comes first on `PATH`, or nothing of this copy's because the folder
its command is in is not on `PATH` (`ArtifactFacts.commands`). Working
that out runs no command. Every refresh does it beside the sources'
inventories, on a background thread: the folders are read while the
inventories run (`commands::start_reading`), and the answer made once
they are in (`commands::finish`), both in
`crates/banager-core/src/commands.rs` and called from `Session::refresh`.
It reads:

| It looks at | How |
|---|---|
| every `PATH` folder, in `PATH`'s order; the `bin` and `sbin` folders of every Homebrew prefix and the `bin` folder of every npm prefix | where the folder leads, then `read_dir`, one level deep: each folder once, however many entries name it. An empty or relative `PATH` entry is skipped, and so is a folder that does not exist or that no shell could reach. A `PATH` folder that is there but cannot be listed is kept in its place, unread, as a protected one is (`read_folders`) |
| each entry in a Homebrew or npm prefix's `bin` (and Homebrew's `sbin`) | where it leads: which formula's folder in `Cellar`, or which package's in `lib/node_modules` |
| each command a source's own answer names: a cask's `binary` link (`brew info --installed --json=v2`), a pipx app and `~/.local/bin/<its name>`, a uv tool's executable (`uv tool list --show-paths`), a Cargo crate's binaries in `<CARGO_HOME>/bin` (`.crates2.json`), a tool with its own installer's launcher and the commands its installer puts beside it (Grok Build's `agent`, rustup's proxies) | where it leads: whether into that tool's own folder, and whether to a file with an execute bit |
| in each `PATH` folder, the entry of each name some tool provides, however the folder spells it that a Mac's disk takes for the same name (typing `node` runs `NODE`; `protected::same_name`) | where it leads, and whether to a file with an execute bit, in `PATH`'s order |

"Where it leads" is found as `realpath` would find it, but with only
`lstat` of each folder and link on the way and `readlink` of each link --
each asked of the folder before it, held open, and the next folder opened
from it with `O_NOFOLLOW` and checked to be the one seen, so a folder
replaced by a link meanwhile is never followed (a folder is listed the
same way, from `/`, with no link followed) -- each step checked against the places below before it is taken
(the rules of `protected::resolve`, the walk the disk-use measurement
uses, which judging follows through `protected::Round`).
Within one refresh, each name on the way is `lstat`ed, and each link
read, at most once as it is spelled (`protected::Round`), and a folder
opened a second time is kept open until the answers are made (64 at
most), so that a later path below it starts there rather than at `/`;
all of it is let go once the answers are made, never kept into the next
refresh. The plain names at the end of a path -- no `..`, none of them in
a protected place -- can be asked of the folder the walk has reached in
one `fstatat` that follows no link anywhere on the way
(`AT_SYMLINK_NOFOLLOW_ANY`; the kernel looks each folder among them up
again); a link there, on the way or at the end, or a folder that may not
be searched, is then taken one step at a time, as above. A folder kept open follows its folder if another program renames
it, so each time one opened earlier is used again it is first asked
where it now is (`fcntl` with `F_GETPATH`, which looks up no name and
reads nothing in it); one that is now inside a protected place is not
used, and everything that refresh kept is let go and the path walked
from `/` again.
Unlike `realpath`, it keeps each name as `PATH` or the link's text spells
it, so two paths are compared as a Mac's disk compares names -- ASCII
case aside, and the characters it takes for ASCII letters taken for them
(`protected::same_name`): `~/.CARGO/bin` on `PATH` is `~/.cargo/bin`
(`protected::same_path`).

A folder in `~/Desktop`, `~/Documents`, `~/Downloads`, `~/Pictures`,
`~/Movies` or `~/Music`, in iCloud Drive or another cloud folder
(`~/Library/Mobile Documents`, `~/Library/CloudStorage`), in another
app's data (`~/Library/Containers`, `~/Library/Group Containers`) or on
another disk (`/Volumes`), whatever case spells them -- ASCII letters in either case, and the
characters a Mac's disk takes for ASCII letters: `ſ` (long s) for `s`,
`K` (Kelvin sign) for `k`, `ß` and `ẞ` for `ss`, and the ligatures `ﬀ`
`ﬁ` `ﬂ` `ﬃ` `ﬄ` `ﬅ` `ﬆ` for their letters (`protected::AS_ASCII`, found
by asking the disk for every character); any other letter only as
spelled -- and also when
spelled from `/System/Volumes/Data` (the same folders, through macOS's
firmlinks; `protected::DATA_VOLUME`), is not read at all,
as named or where it leads (`protected::resolve`): macOS asks you before
an app looks there, and a network disk that went away does not answer. It
is the same list the disk-use measurement keeps out of
(`crates/banager-core/src/protected.rs`). On `PATH`, such a folder is
kept in its place, unread, and nothing is said about a name it could
hold before another copy; a bin folder there is skipped. A link that leads
into one of these places -- a `PATH` folder that is a link to iCloud
Drive, an `npm link` of a project in `~/Documents` -- is followed only as
far as the place, never into it: what it leads to is not known, so it is
not counted as any tool's command, and nothing is said about a name it
could be before another copy.

Nothing's contents are read, nothing found is run or changed, and no
lock is taken. Reading the folders stops after 20000 entries or 5
seconds, and working out the answer after 5 seconds more
(`CommandBudget::default`); that refresh then says nothing about which
copy runs for what the inventories listed (a row kept from an earlier
refresh, because its source did not answer this time, keeps what was
said then). A read that has not come back a second after its limit is
no longer waited for, and no new one starts while it is still running.
The answer is judged against the `PATH` Banager has: the login shell's,
restored at launch (How Banager runs anything, above). When restoring it
failed, the shell says so (`Session::note_login_path` in `run()`,
`src-tauri/src/lib.rs`): this check reads none of the `PATH` folders, and
nothing is said about which copy runs (the notice under a tool with its
own installer still looks its one command up on the `PATH` Banager has,
as that tool's section says). An alias, a shell function, or a `PATH` that
only a new terminal window or an editor's terminal sets is not seen; the
details say that an alias, a new window or an editor's terminal may
differ. Nothing is said about a Homebrew formula installed as a
dependency, and a keg-only one is never said to be missing from Terminal
(Homebrew keeps it off `PATH` on purpose); one linked by hand (`brew link
--force`) has its links in `<prefix>/bin`, and which copy runs is said of
them as of any formula's. The folder of a
command Terminal cannot find can be copied (*Copy Path*, in
`CommandsGroup` in `src/components/CommandFacts.tsx`, through
`useCopyCommand`), `~` and all; nothing edits a shell file.

## App icons: read through macOS, no command runs

The window asks for the icon of the app a Homebrew cask installed, the
icon Finder shows for it (`artifactIcon` in `src/lib/api.ts`, through
`useArtifactIcon` in `src/lib/queries.ts`), when it draws that cask's
avatar (`ToolAvatar` in `src/components/ToolAvatar.tsx`), and shows it
in place of the cask's logo, if it has one (Network, below). Getting an
icon runs no command, and Banager reads nothing else for it:

- The window sends the row's key — which source, which kind of package,
  which name — and nothing else (`artifact_icon` in
  `src-tauri/src/ipc.rs`). Banager looks that key up in the sources' last
  known state (`Session::artifact_icon` in
  `crates/banager-core/src/session/icon.rs`) and goes on only for a cask
  whose path is absolute and ends in `.app` (`cask_app_bundle` in
  `crates/banager-core/src/icon/mod.rs`): the app that Homebrew's own
  inventory, `brew info --installed --json=v2` (Homebrew's section), names
  beside the cask's `app` stanza (`parse_info_installed` in
  `crates/banager-core/src/adapters/brew/parse.rs`). No part of the key is
  ever read as a path. A formula, a font, a cask with no app, and a key
  the last known state has no row for get no icon, and nothing is read
  for them.
- Banager `lstat`s that path, each time the window asks: an icon is drawn
  only for a folder, never for a link to one — an app Homebrew recorded as
  a link gets no icon rather than one with Finder's alias arrow — and the
  folder's modification time, device and inode tell whether the icon drawn
  for it before is still its own (`AppIcons::bundle_icon`).
- For a folder with no icon drawn yet, or one that has changed since, it
  makes one call, `NSWorkspace iconForFile:`, through the `objc2-app-kit`
  crate, and has AppKit draw that icon 128 × 128 pixels and encode it as
  PNG (`RealIconRenderer` in `crates/banager-core/src/icon/real.rs`).
  macOS finds the icon itself, in the app or in its own icon cache;
  Banager opens no file in the app.
- The PNG goes to the window as a `data:image/png;base64,…` URL. Banager
  keeps each icon in memory, one per app folder, until it quits
  (`AppIcons`), and the window does not ask for it again for an hour
  (`useArtifactIcon`). Nothing is written to disk and no connection is
  made. The window's content security policy already allowed `data:`
  images (`img-src 'self' data: asset: https://asset.localhost` in
  `src-tauri/tauri.conf.json`) and was not changed for this.

An `#[ignore]`d test in `crates/banager-core/src/icon/real.rs` draws
Calculator's icon (`/System/Applications/Calculator.app`) with the real
call and checks it is a 128 × 128 PNG drawn across the whole square; it
reads that icon and writes nothing. Run it with `cargo test -p
banager-core --lib icon::real -- --ignored`; CI does not.

## Disk use: measured read-only, no command runs

The Installed page's details say about how much disk a tool takes
(「占用空间：约312 MB」, "Space used: About 312 MB"), and for a Homebrew
formula the other versions Homebrew keeps beside it, under those versions
(「其他版本：3.6.3」, 「约120 MB」);
the page of the Ollama source says how much its models take together.
Measuring runs no command, and the one file it opens is
`<CARGO_HOME>/.crates2.json`, which Cargo's inventory reads already. After
each refresh has finished — the sources' state committed and the
refresh's locks released — a thread of its own (`SizeMeter` in
`crates/banager-core/src/size.rs`, started by
`Session::refresh_recording`) looks at these folders and files with
`lstat`, the folder listing (`readdir`) and `readlink`, and nothing else.
Each is asked of a folder held open rather than by a path: a folder is
opened (`openat` with `O_NOFOLLOW`, never following a link) from the
folder it is in, checked to be the very folder seen there, and what is
in it is listed and looked at from that descriptor (`fstatat`,
`readlinkat`), so a folder another program replaces with a link while
Banager is looking -- a link to `~/Documents`, say -- is never followed,
and the size is shown as partial. A folder that is moved there, rather
than linked, is a folder like any other there and is measured:

| For | It measures |
|---|---|
| a Homebrew formula | `<prefix>/Cellar/<name>/<version>`; the names in `<prefix>/Cellar/<name>`, and each other version's folder there, as its other versions |
| a Homebrew cask with an app | the `.app` Homebrew names for it (Homebrew's section, `brew info --installed --json=v2`) and `<prefix>/Caskroom/<token>` |
| an npm package | `<prefix>/lib/node_modules/<name>` |
| a pipx or uv tool | its environment, the folder its own listing names |
| a Cargo crate | each program `<CARGO_HOME>/.crates2.json` says it installed, in `<CARGO_HOME>/bin` (that file is read again for this) |
| Claude Code, Antigravity CLI, Grok Build, rustup, Codex, opencode | the program file its launcher leads to |
| Ollama's models | `~/.ollama/models/blobs`, once for all of them, when the Ollama Banager asks is on this Mac |

Nothing else is measured: not pip's packages, not a cask with no app (a
font, a `pkg`), not a tool's settings, caches or downloads. On the way to
each folder, every folder above it is `lstat`ed and a link among them read
(`readlink`), so that where it leads is known before anything there is
looked at; the folder before each step is held open, so a folder already
checked is never looked up by its path again. A model's own size is the one Ollama reports; the models
together are their folder's, each layer once, since models share layers.
That folder is all of `blobs`, so a layer no model uses any more (left
by a removed model, or by a download that stopped) counts in it too.

How it counts: a symbolic link is never followed — the link itself counts,
not what it points at; a folder on another volume is never entered; a file
counts the blocks the disk holds for it (`st_blocks`), and a file with
several hard links counts once. A folder that cannot be read is skipped and
the size is shown as partial (「部分无法读取」). One round looks at
300,000 entries and spends 30 seconds at most (`SizeBudget::default`) --
every entry a listing names counts, also one that then cannot be looked
at, and the time is checked before each one; a
size it stopped short of is shown as "or more" (「…以上」), and a tool
it did not reach before the budget ran out shows no size that round. One
that measured 0 -- only links, as npm's `corepack` under Homebrew's node --
shows no number either, though By Size still sorts it as measured. The
next round measures first what no round has measured yet, and only then
again what an earlier round stopped short of or could not read in full,
showing the earlier number, marked as it was, meanwhile. Every number
is shown as "about" (「约」): an APFS clone (uv builds its tools'
environments that way from its cache) counts in full though it shares its
blocks.

It never looks into these places, nor follows a link into them, so
measuring never makes macOS ask for permission: `~/Desktop`,
`~/Documents`, `~/Downloads`, `~/Pictures`, `~/Movies`, `~/Music`,
`~/Library/Mobile Documents` (iCloud Drive), `~/Library/CloudStorage`
(apps that keep files in the cloud), `~/Library/Containers` and
`~/Library/Group Containers` (other apps' data), and `/Volumes` (every
other disk), whatever case spells them -- ASCII letters in either case, and the
characters a Mac's disk takes for ASCII letters: `ſ` (long s) for `s`,
`K` (Kelvin sign) for `k`, `ß` and `ẞ` for `ss`, and the ligatures `ﬀ`
`ﬁ` `ﬂ` `ﬃ` `ﬄ` `ﬅ` `ﬆ` for their letters (`protected::AS_ASCII`, found
by asking the disk for every character); any other letter only as
spelled -- and also as spelled from the
volume that holds them, `/System/Volumes/Data` (`/System/Volumes/Data/Users/<you>/Documents`
is `~/Documents`, through the firmlinks macOS keeps; `protected::DATA_VOLUME`).
A tool kept in one of them shows no size (`Protected`). It is the same list the command check keeps out
of (`crates/banager-core/src/protected.rs`).

Nothing is written: the sizes stay in Banager's memory until it quits, and
what was already measured in full at the same version, from the very
same folders and files (a Cargo crate that gained a program is walked
again), is not walked again. They
are not part of what a refresh reports, and measuring takes no lock an
operation or a refresh waits on; a newer refresh stops a round still
running and starts another. The window asks for the result with
`get_sizes` (`src-tauri/src/ipc.rs`), which takes nothing from it, and
hears that it moved through the event `SizesChanged`.

## Data an uninstall leaves behind: read-only, no command runs

No source's uninstall, as Banager runs it, removes the folders an AI
coding tool keeps its settings and data in, nor the models Ollama
downloaded. That holds because Banager never runs `brew uninstall --zap`
(`test_plan_never_passes_zap_force_or_ignore_dependencies` in
`crates/banager-core/src/adapters/brew/mod.rs`): a cask's `zap` stanza
may name such a folder, and it runs only with `--zap`. So the uninstall
preview names them (「卸载后会保留」, "Stays after uninstalling"):
`Session::issue_plan` (`crates/banager-core/src/session/kept.rs`) adds a
line for each of these that is there, for an uninstall of a tool of that
family on any source (`crates/banager-core/src/kept_data.rs`):

| Tool | Paths looked at |
|---|---|
| Claude Code | `~/.claude`, `~/.claude.json` |
| Codex | `~/.codex`, measured without `~/.codex/packages/standalone` |
| Gemini CLI | `~/.gemini`, measured without `~/.gemini/antigravity-cli` |
| Qwen Code | `~/.qwen` |
| Kimi Code | `~/.kimi-code`, `~/.kimi` |
| iFlow CLI | `~/.iflow` |
| CodeBuddy Code | `~/.codebuddy` |
| Qoder CLI | `~/.qoder`, measured without `~/.qoder/bin/qodercli` |
| opencode | `~/.local/share/opencode`, `~/.config/opencode` |
| Crush | `~/.local/share/crush`, `~/.config/crush` |
| Amp | `~/.config/amp` |
| Kilo | `~/.local/share/kilo`, `~/.config/kilo` |
| GitHub Copilot CLI | `~/.copilot`, measured without `~/.copilot/pkg` |
| Auggie | `~/.augment` |
| Factory Droid | `~/.factory` |
| Cursor CLI | `~/.cursor/cli-config.json` |
| Aider | `~/.aider`, `~/.aider.conf.yml`, `~/.aider.model.settings.yml`, `~/.aider.model.metadata.json` |
| Goose | `~/.local/share/goose`, `~/.config/goose` |
| Mistral Vibe | `~/.vibe` |
| OpenClaw | `~/.openclaw`, `~/.clawdbot` |
| Ollama (Homebrew's formula `ollama`, cask `ollama-app`) | `~/.ollama/models` |
| Antigravity CLI | `~/.gemini/antigravity-cli` |
| Grok Build | `~/.grok`, measured without `~/.grok/downloads` |

The paths come from the bundled table of AI coding tools
(`data/ai-tools.json`, `data_paths`) and, for Ollama, its FAQ. Each is
the folder or file that tool's own docs or source name for its settings,
logins, sessions or history on macOS, read as text and never run; the
module doc of `crates/banager-core/src/families.rs` gives the source of
every one. A path the preview already names is not named twice: Claude
Code's own installer's uninstall lists `~/.claude` and `~/.claude.json`
among what it keeps (Claude Code, above), Antigravity CLI's lists
`~/.gemini/antigravity-cli` (Antigravity CLI, above), and Grok Build's
lists `~/.grok` (Grok Build, above); for Grok Build, the line is in the
preview of an uninstall of Homebrew's cask `grok-build`.

Each is the default place. A tool may let a shell move its folder with a
variable (`KIMI_CODE_HOME`, `KIMI_SHARE_DIR`, `IFLOW_HOME`,
`CODEBUDDY_CONFIG_DIR`, `QODER_CONFIG_DIR`, `CRUSH_GLOBAL_CONFIG`,
`CRUSH_GLOBAL_DATA`, `COPILOT_HOME`, `GOOSE_PATH_ROOT`, `VIBE_HOME`,
`OPENCLAW_STATE_DIR`, `GROK_HOME`, `XDG_DATA_HOME`, `XDG_CONFIG_HOME`);
Banager reads none of them -- a Mac app started from the Finder inherits
nothing from your shell but the `PATH` Banager asks your login shell for
-- so a folder moved that way is not named.

opencode's two are the folders its own docs name for macOS
(opencode.ai/docs/troubleshooting, "Storage": sessions, `auth.json`
and logs in `~/.local/share/opencode`; opencode.ai/docs/config: global
settings in `~/.config/opencode`), and the two its own `opencode
uninstall` keeps on `--keep-data` / `--keep-config`; it always removes
its cache and state folders, which are not named here. They are named
in the preview of an uninstall of npm's `opencode-ai` or Homebrew's
formula `opencode`. opencode's own install (`~/.opencode`, opencode,
above) has no uninstall in Banager, and neither folder is inside it.
Kilo, built from opencode, keeps its data and settings the same way, in
`~/.local/share/kilo` and `~/.config/kilo`; its cache and state folders
are not named either.

Notes by tool, with what is not named and why:

- Kimi Code: `~/.kimi` is the older Python Kimi CLI's folder (PyPI's
  and Homebrew's `kimi-cli`), which Kimi Code's migration reads and never
  changes; both folders are named for any member of the family.
- CodeBuddy Code: not `~/.local/share/codebuddy`, where its own installer
  keeps its program's versions. Homebrew's `codebuddy` cask, Tencent's
  CodeBuddy editor, lists `~/.codebuddy` among what its `--zap` trashes,
  so the editor may keep files there too, and the size counts them.
- Crush: not `~/.cache/crush`, nor the sessions it keeps in each
  project's own `.crush` folder. Amp: not `~/.local/share/amp`, which its
  program defines but no Amp doc describes.
- Cursor CLI: only its own settings file is named, since `~/.cursor` is
  the Cursor editor's folder too; no Cursor doc names the CLI's other
  files there.
- Aider: not the chat and input history it writes in each git
  repository.
- Goose: not `~/.local/state/goose`, its logs, which it deletes itself
  after two weeks.
- Ollama: not the rest of `~/.ollama` besides its models.
- OpenClaw: `~/.openclaw` may hold more than settings and data. Its own
  `install-cli.sh` installs a Node and a copy of OpenClaw under
  `~/.openclaw/tools` (in a folder named after the Node version) and
  `~/.openclaw/bin`, and `tools/` also holds what its skills download.
  The size counts all of it: a fixed folder name cannot pick the install
  out, so nothing there is left out.

`~/.gemini` is shared: it is the folder Gemini CLI's docs name, and
Antigravity CLI keeps everything of its own in `~/.gemini/antigravity-cli`
(on the author's Mac, 99 % of `~/.gemini`). What is left is not shown to
be Gemini CLI's alone: on the author's Mac it was only `config/`, `tasks/`
and `users/` (about 7.4 MB), none of Gemini CLI's documented files such as
`settings.json`. Banager only says which part is Antigravity CLI's and
leaves it out; it does not claim the rest. So Gemini CLI's line leaves
another tool's path that the table puts inside its folder out of the size
(`kept_data::others_inside`): the walk skips that entry -- it neither
counts nor enters it, nor spends the budget on it -- and, when it met it,
the line says whose it is and that it is not counted (「…是Antigravity
CLI的数据，不算在内」, "… is Antigravity CLI's data and isn't counted
here").

How: during the uninstall preview only, each path is looked at as disk
use measures a tool's folder (`size::look_at`: `lstat`, `readdir` and
`readlink`; no file is opened -- each asked of a folder held open, opened
with `O_NOFOLLOW`), with a budget of 100,000 entries and 1
second for all of them together (`kept_data::BUDGET`). A size it stopped
short of is shown as "or more" (「…以上」), and a path it did not reach,
or could not read, or that measured 0, is named with no size. A path that leads into one of
the places disk use never looks into (Disk use, above: the one list in
`crates/banager-core/src/protected.rs`, spelled in any case or with any
of the characters a Mac's disk takes for ASCII letters, as there) is
named with no size, and nothing there is read -- also when the link is a
folder on the way (`~/.ollama`, for `~/.ollama/models`): the path is then
named without Banager knowing whether it is there inside. A path that is not there,
or a link that leads nowhere, gets no line. Four folders inside these
hold another copy of the tool's program rather than its data, which
uninstalling the copy a source manages leaves where it is: the size
neither counts nor enters them (`kept_data::LEFT_OUT`, `size::look_at`),
and the line says so behind an ⓘ. They are `~/.codex/packages/standalone`,
Codex's own install (Codex's own install, above), beside npm's
`@openai/codex`; `~/.qoder/bin/qodercli`, where Qoder CLI's install script
puts its program's versions, beside npm's `@qoder-ai/qodercli`;
`~/.copilot/pkg`, the copies of the program GitHub Copilot CLI's updater
downloads; and `~/.grok/downloads`, Grok Build's own install (Grok Build,
above), beside Homebrew's cask `grok-build`.

Nothing is written, and nothing is deleted: the preview has no button or
command that removes these paths. The one action beside each is Copy
Path, which puts the path, as it is shown (`~` and all), on the clipboard.

## What runs on a Homebrew package: read-only, no command runs

`brew uses --installed` (Homebrew, above) names the formulae and casks
that need a package, and nothing else. Other sources can run on a Homebrew
package as well, and Homebrew does not refuse to uninstall it: npm, whose
`npm` and every global package run on the `node` a Homebrew `node` or
`node@22` put on `PATH`; pip, which runs in a Homebrew `python@3.x`; a
pipx or uv tool whose own environment's Python is a Homebrew `python@3.x`;
Ollama, which runs Homebrew's `ollama` (or the app of its cask); and pipx,
uv and Cargo themselves, when a Homebrew `pipx`, `uv` or `rust` is what
Banager runs for them. So the uninstall preview of a Homebrew formula or
cask looks for every source of those kinds -- npm, pip, pipx, uv, Cargo
and Ollama (`HOSTED` in `crates/banager-core/src/needed_by.rs`) -- that
runs on it (`Session::issue_plan`, `crates/banager-core/src/session/needed_by.rs`).

What it looks at, during the uninstall preview of a formula or cask only:

- the package's own folder: a formula's `<prefix>/Cellar/<name>` (its
  `opt` link and the aliases Homebrew keeps in `opt` lead there too), or a
  cask's `<prefix>/Caskroom/<token>` and the app Homebrew put in place for
  it;
- each such source's program, the one Banager runs for it (found on
  `PATH` when the source was detected), every link followed;
- for npm, the `node` first on the `PATH` of the last refresh, found as
  Banager finds every program it runs (`resolve_exe`), every link
  followed: npm's `bin/npm-cli.js` begins `#!/usr/bin/env node`, so that
  `node` is what npm and its packages run on;
- for pipx and uv, `bin/python` in the environment each tool has of its
  own (the folder pipx's `app_paths` and uv's `--show-paths` name), every
  link followed.

A source runs on the package when its program, or npm's `node`, leads
into the package's folder: then every tool it lists needs the package,
but for what comes with its program -- npm's `npm` and `corepack`, and the
`pip`, `setuptools` and `wheel` Homebrew's Python formulae install
themselves -- and what pip installed for another package. Some of a pipx's
or uv's tools need it when their environment's `bin/python` leads into
it. A source with no such tool is not named: a Node.js with only its own
npm left can be uninstalled. A `node@22` that is keg-only and not linked
is nobody's `node`: nothing Banager runs leads into it, and nothing is
said of it. An Ollama whose `OLLAMA_HOST` names another machine is not
looked at: its models are kept and run there, and the `ollama` on this
Mac is only a client.

Each source with any tool that needs the package is listed under 「依赖此
工具的软件」 ("Software that uses it"), after what Homebrew names --
「npm及其4个工具」, 「pipx装的2个工具」 -- Uninstall stays off, and the
sentence under the list names the tools to uninstall first (pip's, which
Banager does not uninstall, in Terminal). Whatever the
window sends, `Session::submit` refuses such a preview
(`UninstallBlocked::NeededBySource`), and a batch leaves the package out
with the same words.

How: read-only, as the command check is (Which copy a command runs,
above): each path is followed one step at a time (`protected::resolve`:
`lstat` and `readlink`, each asked of the folder before it, held open),
and never into the places macOS asks about first nor onto another disk --
the same places the command check never reads. Only folders are opened,
to follow each link: no file's contents are read, nothing is written, and
no command runs. At most 2,000 paths and 1 second for one
preview (`needed_by::BUDGET`), and the preview waits one second more at
most for a step that does not answer at all (a folder on a disk that
stopped answering), then goes on without it; a look that did not finish
says so (「无法确定还有哪些软件要用它。卸载前请自行确认。」, "Couldn't check what
else needs this. Check yourself before you uninstall.",
`Warning::DependentsUnknown`), never that nothing runs on it.

## Diagnostic info: read-only, no command runs

Settings' About has Copy Diagnostic Info (「拷贝诊断信息」); the Help menu's
Copy Diagnostic Info… (「拷贝诊断信息…」) only opens Settings on that button,
focused, so the copy is always the button's click. It puts a short plain text on the clipboard, in the window's
language, for the user to paste to whoever helps them: Banager's version,
macOS's version and the chip, the window's language; each source's kind,
version, program and status; the folders on `PATH` and whether they are the
login shell's; when the last check was and whether it covered every
source, as the Updates page counts it, naming those it did not; how many
tools Terminal cannot find and how many are installed more than once, or
that this check did not look at the commands; and the disk
they take, once measured. Settings' checkbox, off each time Settings opens,
adds each source's tools by name and version.

The window builds the text from what it already holds, and asks Rust only
for what it cannot read itself, with `get_system_facts`
(`src-tauri/src/ipc.rs`, `crates/banager-core/src/diagnostics.rs`), which
takes nothing from it. That reads two strings the kernel keeps,
`kern.osproductversion` and `machdep.cpu.brand_string`, with
`sysctlbyname`; the process's own `PATH` and `HOME`, and no other
environment variable; and the sources' last known state. No command runs,
no file is opened, nothing is written to disk, and no connection is made.
Every path in the text has the home folder written as `~`. The folders on
`PATH` are the one environment variable's value the text holds, as its
"Command search folders" (「查找命令的文件夹」) lines, and Settings' footnote says
so; it never holds any other environment variable's value (a proxy setting
can hold a password), anything from a shell file, or a token.

Check Tool Setup (「检查工具环境」), in the Help menu and beside Copy
Diagnostic Info in Settings' About, opens a sheet that says the same facts
in sentences: whether the login shell's `PATH` was read, how many of its
folders the last check read and how many it could not, each source's
status, how many tools Terminal cannot find or has twice, what Homebrew
disabled, deprecated or keeps other versions of, and the disk measured. It
is built from the same `get_system_facts` answer and the snapshot and sizes
the window holds; the folder counts, and the unread folders' paths that it
shows only with Show Technical Details on, are what the last refresh round
made of the `PATH` folders when it read them to say which copy of a
command runs (`Session::path_folders`, `commands::finish`). Nothing more is
read, nothing runs, nothing is written, and no connection is made for it.

## Files Banager reads

All read-only, none saved anywhere else, none uploaded:

- Homebrew: whether the three candidate `brew` paths exist;
  `<prefix>/var/homebrew/locks` and the `update` lock file in it, during
  the uninstall preview; its `brew.env` files, during every install,
  uninstall and upgrade preview; during a cask's uninstall preview, the
  names in its `<prefix>/Caskroom/<token>/.metadata` folder and in the
  folders there, the caskfile Homebrew saved when it is JSON, and
  `INSTALL_RECEIPT.json`; during every uninstall preview, its trust list,
  `trust.json` in the user's Homebrew config folder (Homebrew's section).
- A Homebrew cask's app, when the window asks for its icon: `lstat` of the
  `.app` Homebrew named for that cask, and the icon macOS finds for it
  through `NSWorkspace iconForFile:` — Banager opens no file in the app
  (App icons, above).
- npm: whether `{prefix}/lib/node_modules`, `{prefix}/lib` or `{prefix}`
  is writable, via `access(2)`.
- pip: the canonical path of each interpreter found, to count it once;
  for one in `/usr/bin`, where `usr/bin/<its name>` in the developer
  directory `xcode-select -p` names leads, and whether that is an
  executable file (`realpath`, `stat`).
- Cargo: `<CARGO_HOME>/.crates2.json`; whether `cargo-binstall` is on
  `PATH`.
- Ollama: whether `/Applications/Ollama.app` or `~/Applications/Ollama.app`
  is a directory; `~/.ollama/models/manifests/registry.ollama.ai/{namespace}/{name}/{tag}`
  for each pulled model.
- Claude Code: whether `~/.local/bin/claude` exists and where it links to
  (`lstat`, `readlink`, `realpath`, also for the folder the link is in
  and for `~/.local/share/claude`); for the notice under the source, each
  `PATH` directory's `claude` in `PATH`'s order (`lstat` and `readlink`,
  one step at a time, never into a protected place: where it leads, and
  whether that is a regular file with executable bits) until the first
  such file, and, when that one does not resolve to this copy, on down
  `PATH` the same way until one does or `PATH` ends;
  `~/.claude/settings.json`, for the one key `autoUpdatesChannel` (read
  and discarded, only when it is a regular file of at most 16 MiB, opened
  without waiting; a missing or unreadable file or key means `latest`).
  For an uninstall preview, when it is confirmed, and again right before
  each path is moved: `lstat` and the resolved path of each path on the
  uninstall list and of the folder it is in, the resolved home folder and
  the shared folders in it, the launcher's link text, and whether
  `~/.claude` and `~/.claude.json` exist and where they lead (Claude
  Code's section). After an uninstall: the same look at the launcher
  that detection makes (`lstat`, `readlink`, `realpath`, the same paths),
  and nothing else — no version is read.
- Antigravity CLI: whether `~/.local/bin/agy` exists and what it is
  (`lstat`, `realpath`); for the notice under the source, each `PATH`
  directory's `agy`, as for Claude Code. For an uninstall preview, when
  it is confirmed, and again right before each path is moved: the same
  reads as for Claude Code's list, for `~/.local/bin/agy` and every
  `agy.<time>.old` backup, which Banager finds among the names in
  `~/.local/bin` (the Other Programs page's rule 4 goes by the same names); and
  whether `~/.gemini/antigravity-cli`, `~/.cache/antigravity`, `~/.zshrc`
  and `~/.zprofile` exist and where they lead (`lstat`, `realpath`;
  nothing in them is read). After an uninstall: the same look at the
  launcher that detection makes, and nothing else — no version is read.
- Grok Build: whether `~/.grok/bin/grok` exists and where it links to
  (`lstat`, `readlink`, `realpath`, also for the folder the link is in and
  for `~/.grok`); for the notice under the source, each `PATH` directory's
  `grok`, as for Claude Code. For an uninstall preview, when it is
  confirmed, and again right before each path is moved: the same reads as
  for Claude Code's list, for `~/.local/bin/grok`, `~/.local/bin/agent`,
  `~/.grok/downloads`, `~/.grok/bundled`, `~/.grok/completions`,
  `~/.config/fish/completions/grok.fish`, `~/.grok/bin/agent` and
  `~/.grok/bin/grok`, and whether `~/.grok` and `~/.zshrc` exist and
  where they lead; for the preview and when it is confirmed, also whether
  `/usr/local/bin/grok` and `/usr/local/bin/agent` are links into
  `~/.grok` and, for one that is and still leads somewhere, every folder,
  link and file on its way there (`lstat`, `readlink`, `realpath`).
  Nothing in `~/.grok/config.toml` or `~/.grok/auth.json` is read. After
  an uninstall: the same look at the launcher that detection makes, and
  nothing else — no version is read. The preview of uninstalling
  Homebrew's cask `grok-build` walks `~/.grok` for its size, names and
  sizes only (Data an uninstall leaves behind, above).
- rustup: whether `$CARGO_HOME/bin/rustup` exists and is a regular file
  (`lstat`, `realpath`); whether `~/.cargo` and `~/.rustup` are real folders
  and not links, and whether anything directly inside either is a link
  (`lstat`, and a listing of each folder's top level in which nothing is
  opened — to decide whether the uninstall is offered — at every
  inventory, at the uninstall preview, and again right before the
  uninstall command is started); during the uninstall preview only, the
  names in `~/.rustup/toolchains` and in `~/.cargo/bin` (directory
  listings — nothing in them is opened), `~/.cargo/.crates2.json`, whether
  `/opt/homebrew/Cellar/rustup` or `/usr/local/Cellar/rustup` exists, and
  the shell startup files named in its section (eight under your home, and
  zsh's three under `ZDOTDIR` when that names another folder), each
  opened without waiting (links followed) and read only when `fstat` on
  the open file says it is a regular file of at most 16 MiB, its device
  and inode taken from that `fstat` so that two names of one file share
  one copy, and only searched for a line
  about Cargo's env file and for whether the lines above it stand alone,
  as its section says; nothing else under
  `RUSTUP_HOME` is ever read. After an uninstall: whether
  `$CARGO_HOME/bin/rustup` is still there (`lstat`, `realpath`), and
  nothing else — no version is read.
- Codex (its own install, listed only): whether `~/.local/bin/codex`
  exists and where it links to (`lstat`, `readlink`, `realpath`, also for
  the folder the link is in and for `~/.codex/packages/standalone`); where
  `~/.codex/packages/standalone/current` links to and whether that is a
  folder directly in `releases/` (`readlink`, `realpath`, `stat`);
  `~/.codex/packages/standalone/auto-update-version`, at most 256 bytes,
  when it is a regular file (opened without waiting or following a link,
  then `fstat`); for the notice under the source,
  each `PATH` directory's `codex`, as for Claude Code. Nothing else under
  `~/.codex` is read for this row, and no command runs (Codex's section);
  the preview of uninstalling a Codex walks `~/.codex` for its size,
  names and sizes only (Data an uninstall leaves behind, above).
- opencode (its own install, listed only): whether
  `~/.opencode/bin/opencode` exists, whether it is a regular file and
  where it leads (`lstat`, `realpath`); for the notice under the source,
  each `PATH` directory's `opencode`, as for Claude Code. No file's
  contents are read, and no command runs (opencode's section).
- The Other Programs page's scan: the entries of the bin directories its section
  lists, one level deep, and each entry's metadata and link target — never
  a file's contents; and where each source's owned folder leads, one step
  at a time, uv's Python folder `~/.local/share/uv/python` among them
  when uv is found — never anything in the places Disk use names, nor
  through a link into them (Unknown-source scan, above). A row's Show in
  Finder: where the path it shows leads (`realpath`), and nothing else.
- Disk use, after each refresh: each tool's own folder or program file, the
  names in each formula's `<prefix>/Cellar/<name>`, Ollama's
  `~/.ollama/models/blobs` and `<CARGO_HOME>/.crates2.json`, with `lstat`,
  `readdir` and `readlink` — never a file's contents but that one file's,
  and never anything in the places its section names (Disk use, above).
- What an uninstall leaves behind, during an uninstall preview of an AI
  coding tool: the folders and files the table names for its family
  (`~/.claude`, `~/.codex`, `~/.grok`, …; Data an uninstall leaves behind,
  above, lists every one) or `~/.ollama/models`, with `lstat`, `readdir`
  and `readlink` — never a file's contents, and never anything in the
  places disk use never looks into (Data an uninstall leaves behind,
  above).
- What runs on a Homebrew package, during the uninstall preview of a
  formula or cask: where `<prefix>/Cellar` or `<prefix>/Caskroom` and the
  cask's app lead; where the program Banager runs for npm, pip, pipx, uv,
  Cargo and Ollama leads; the first `node` on `PATH`, for npm; and each
  pipx and uv tool's `bin/python` (`lstat` and `readlink`, one step at a
  time, never into a protected place) — never a file's contents (What
  runs on a Homebrew package, above).
- Which copy a command runs, at every refresh: the names in each `PATH`
  folder and in each Homebrew and npm prefix's `bin` (and Homebrew's
  `sbin`), one level deep, and where each entry a command could be leads
  and whether it can run (`lstat` and `readlink`, one step at a time, or
  one lookup that follows no link for the plain names at the end of a
  path; never into a protected place) — never a file's contents (Which
  copy a command runs, above).
- Banager's own `settings.json` in its application data directory
  (`settings::load`; a missing or unreadable file means default settings).
- Banager's own `history.json` beside it, once, as Banager starts
  (`HistoryStore::open` in `crates/banager-core/src/history/mod.rs`; a
  missing, unreadable or malformed file means an empty history, and a file
  a newer Banager wrote is read as empty and never written over).
- Banager's own `.window-state.json` beside it, once, as the window opens:
  the size and position the window had when Banager last quit, the
  position used only if a display is still there (the Tauri window-state
  plugin, registered in `run()` in `src-tauri/src/lib.rs`; a missing or
  unreadable file means the window opens at its default size, centred).

## Files Banager writes

Three, all in Banager's application data directory
(`~/Library/Application Support/com.brulek.banager`). `settings.json`
(`settings::save`, written to a `settings.json.tmp.<n>` beside it and
renamed into place, so a crash mid-write cannot leave it corrupt; the
directory is created if it is missing). It holds the Settings page's
choices, among them the updates hidden from the Updates page: versions
skipped (`skipped_versions`), tools never to remind about
(`ignored_updates`) and tools put off for 30 days (`snoozed_updates`,
each with the time it ends; it hides every version of the tool, not only
the one offered, and is dropped as the file is loaded after that time),
and whether the welcome sheet of the first launch has been shown
(`welcome_seen`), so that it shows once. A `settings.json` written before
that field existed reads it as not shown, so the sheet also shows once
after an upgrade.

`history.json`, Banager's record of the updates and uninstalls it ran,
which the Updates page's 「最近的更新记录」 lists after a restart
(`crates/banager-core/src/history/mod.rs`, attached in
`src-tauri/src/history.rs`). One record per finished update or uninstall:
when it finished, the package's key (its source's instance id — which can
name a folder in the home folder, as `cargo:/Users/you/.cargo` does — its
kind and its name), the name its row had, the source's kind, the version
before and the version read back after, how it ended (succeeded, needs
attention with its reason, failed with the cause in one word when one is
known, could not be confirmed, or cancelled once Banager had handed it to
the tool's adapter, which can be before the tool's own command started, as
when Homebrew was still finishing a `brew update`), and whether Banager
saw the change itself (the version it read before and after differ). Also
the time the page's Clear was last pressed. Never a line of a log, a
command line, an error message or any other path: a failure's cause is
read from the tool's last lines as the operation finishes, and the lines
are dropped. An operation cancelled before Banager began carrying it out
(while it waited for its turn, or while Banager read the installed
version) is not recorded. Each record also carries a random id of the
launch of Banager that ran it and the operation's number in that launch,
so that the page lists an update it watched finish only once. The file
keeps the newest 1,000 records and nothing older than 180 days: as Banager
starts it drops the rest and, if it dropped any, writes the file again
straight away; it drops them again at each record. The 180 days count back
from a time the file keeps as trusted (`trusted_at`), or from now when the
Mac's clock says an earlier one. A later time the clock says is kept as
not yet trusted (`pending_at`) and becomes trusted only once the clock has
said a time at least a week after it (`CLOCK_CONFIRM_MS`); set back before
then, it is forgotten. Records are stamped with the clock as it is, but a
record's own time never decides what is too old. So a clock set far ahead
drops nothing for a week -- however many updates finish under it and
however often Banager is opened again -- and nothing at all if it is put
right within the week; with a right clock, a record is dropped up to a week
(plus the time until the next record) after it is 180 days old, and after
more than 180 days with no update the file can keep older records until
the next record. A file from before these two times were kept starts
trusting from now, or from its newest record when that is earlier.
「最近的更新记录」 lists none older
than 180 days by the Mac's clock (`HistoryStore::view`). It is written whole to
a `history.json.tmp.<n>` beside it and renamed into place, on a thread of
its own, after each operation finishes and after Clear. A missing,
unreadable or malformed file is an empty history and is replaced at the
next record; a file a newer Banager wrote is left exactly as it is. To
remove the history, quit Banager and delete `history.json`; it starts
empty at the next launch. Clear does not delete it.

And `.window-state.json`: the
window's size and position, and whether it was zoomed or in full screen,
written as Banager quits so that the window opens the same way next time
(the Tauri window-state plugin, registered in `run()` in
`src-tauri/src/lib.rs`, which keeps it in Tauri's config directory for the
app — on macOS the same folder). That one is written in place, not renamed
into place: a file a crash cut short is ignored at the next launch, and
the window opens at its default size. Nothing else on the Mac is written
or deleted by Banager itself. It moves files in one case: a
confirmed uninstall of a tool that has no uninstall command (Claude Code,
Antigravity CLI or Grok Build) moves the paths its preview listed to the
Trash (next section). The programs Banager runs write their own files as
they run — Grok Build's own update check (`grok update --check --json`,
Grok Build's section), for one, writes inside `~/.grok` on every refresh:
on the recorded run it replaced `~/.grok/version.json` with the time of
the check, added two lines to its log and touched the user guide it
ships. Those writes are grok's, not Banager's. Every other change to what
is installed is made by the tool named in the preview, running the
command shown there.

## Moving files to the Trash

`RealTrasher` (`crates/banager-core/src/trash/real.rs`) is the only code
in Banager that changes a file on the Mac other than its own settings and
history.
It makes one call per path, `NSFileManager
trashItemAtURL:resultingItemURL:error:` — the call Finder makes for Move
to Trash — through the `objc2-foundation` crate, and it is called only by
a confirmed path-list uninstall (`removal::execute_removal`; the Claude
Code, Antigravity CLI and Grok Build sections), for each path right after
that path's last check. It never deletes anything, never empties the Trash
and never renames a file itself, and a symbolic link is moved as the
link, never its target: the
item's kind comes from the `lstat` that ends its last check, so a link is
never handed to the system as a folder, and nothing else looks at the
path between that check and the call. The call itself takes a path, so
one gap remains: Banager checks each item immediately before moving it;
a program running as you that swaps the item in that instant could still
race it. Each move is written to the operation log with where the item
now is (`LogNote::MovedToTrash`); an item macOS refuses stops the
uninstall there, with macOS's own reason (`LogNote::TrashFailed`); when
the time budget runs out between items, the log names the item the
uninstall stopped before and the budget it ran out of
(`LogNote::OutOfTime`); and a path on the list that is there once the
pause after the last move is over is named too, and left where it is
(`LogNote::BackAfterUninstall`; the Claude Code section says why).
After each move Banager waits 3 seconds
(`removal::PUT_BACK_SETTLE`) before it moves anything else, and before it
reports the uninstall finished. That holds across uninstalls: up to three
operations run at once and each path-list uninstall locks only its own
tool, so two tools' uninstalls can run side by side, and their moves take
turns on one shared clock (`removal::LastMove`) — an item waits while an
item of the other uninstall is waiting or moving, then until 3 seconds
after the last move Banager made, and only then gets its last check and
its move. Cancel ends a wait, and no wait outlasts the uninstall's time
budget; time spent waiting for another uninstall's moves comes out of it
too. The second finding below says why. A debug build of Banager, never
a release one, also tries to list the Trash after each move and prints
whether it may; that is how the pre-merge check learns the build it ran
had no Full Disk Access.

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
  item of the burst did (15 runs out of 15). Those were one process's
  calls, and two uninstalls in Banager are one process too — hence the
  3-second pause between any two of Banager's moves, not only between one
  uninstall's: it makes Put Back likely for every item, not certain, and
  an item without the record can still be dragged back out of the Trash
  by hand. The runs that recorded every item also kept running for 3
  seconds after the last call, and the record is written after the call
  returns — with Full Disk Access, a process that quit at once lost the
  later records — so Banager waits after an uninstall's last move too,
  and quitting Banager while an uninstall is still running may leave the
  item it moved last without Put Back. Why macOS behaves this way is not
  known: the pause is a measurement on one Mac, not a documented
  guarantee.
- A plain `rename` into `~/.Trash` from the same process succeeded too
  (24 runs out of 24), where the design had expected it to be refused:
  the Trash's protection covers listing it, not adding to it, so a `mv`
  could have reached it. Banager does not use one anyway: a renamed item
  gets no Put Back record, and one `mv` of Claude Code's two paths named
  `claude` collides on the name — `mv -n` skips the second and still
  reports success.

Not verified by that app: a click on Put Back itself (the records were
checked, not used), a build of Banager itself, a symbolic link whose
target is gone — which is what every Claude Code uninstall moves last:
the launcher, after the program files it points to — other macOS
versions, and Intel Macs.

`crates/banager-core/tests/standalone_uninstall_test.rs` has an
`#[ignore]`d test that makes five throwaway items — a file, a folder, a
link to each, and a link to nothing — moves them with the real call, and
checks that each lands in `~/.Trash` as itself; CI runs it. It runs from
a terminal or a CI runner, not from a Finder-launched app without Full
Disk Access, so it checks the move, not Put Back.

## Network: Banager only connects to these hosts

Every request goes through `RealHttpClient`
(`crates/banager-core/src/http/real.rs`), and it refuses, before opening a
connection, any `https` request whose host is not on this list
(`ALLOWED_HTTPS_HOSTS`, checked by `host_allowed` at the top of `send`):

| Host | What is fetched | By |
|---|---|---|
| `crates.io` | `GET /api/v1/crates/{name}` — the newest stable version of one crate | Cargo's `check_updates` |
| `pypi.org` | `GET /pypi/{name}/json` — the newest version of one package | pipx's `check_updates`, on pipx < 1.16 only |
| `registry.ollama.ai` | `GET /v2/{namespace}/{name}/manifests/{tag}` — one model's manifest | Ollama's `check_updates` |
| `downloads.claude.ai` | `GET /claude-code-releases/latest` or `/stable` — the newest published Claude Code version on that channel, answered as one bare version number | Claude Code's `check_updates` (`StandaloneAdapter`) |
| `static.rust-lang.org` | `GET /rustup/release-stable.toml` — the newest published rustup version, a two-line TOML file (`version = '…'`) | rustup's `check_updates` (`StandaloneAdapter`) |
| `antigravity-cli-auto-updater-974169037036.us-central1.run.app` | `GET /manifests/darwin_arm64.json` — the newest published Antigravity CLI version for Apple silicon, as the JSON manifest its installer and its updater read (`version`, `url`, `sha512`; only `version` is used) | Antigravity CLI's `check_updates` (`StandaloneAdapter`), only when Banager itself runs on Apple silicon — on an Intel Mac no request is made and the row says only that it could not be checked (why, only with "Show technical details" on) |

Plain `http` is exempt from the list for one caller: the Ollama daemon at
`OLLAMA_HOST` or `http://127.0.0.1:11434` (`GET /api/tags`), which may be
a machine the user named. The exemption is by scheme, not by caller: an
`https://` `OLLAMA_HOST` is refused like any other https host that is not
in the table, before any connection — Ollama's `detect` does not even
build the request — and that Ollama's notice says "Connecting to Ollama
over https isn't supported" (its section says exactly how). Recorded in
`docs/superpowers/backlog.md`.

Every request: TLS through rustls; the header `User-Agent:
banager/<version>`; no other header of Banager's own, except `Accept` on
the Ollama registry request — the HTTP library adds what the protocol
needs, `Host` and `Accept: */*`, and nothing else; no cookies, no
credentials, nothing about this Mac in the request; a timeout per request
(listed in each source's table: 30 s unless stated, and the daemon check
in Ollama's detect is 10 s); a response body limit of 8 MiB
(`MAX_RESPONSE_BYTES`); and no redirect is ever followed — a 3xx is an
error. Nothing is ever sent by any method but `GET`.

Three things are outside that client and worth saying out loud. The
window itself cannot make a network request: its content security policy
is `connect-src 'self'` (`src-tauri/tauri.conf.json`), and a navigation
to another address, which that policy does not stop, is refused
(`src-tauri/src/navigation.rs`: the window loads Banager's own page,
`tauri://localhost`, and in a development build the Vite server on
`localhost`, and no other address). The Tauri opener
plugin — the one that opens a URL or a path in another application — is
registered (`run()` in `src-tauri/src/lib.rs`), and the main window may
call none of its commands: `src-tauri/capabilities/default.json` gives it
no `opener:` permission. Banager's own `reveal_in_finder`, the Other
Programs page's Show in Finder, calls the plugin's `reveal_item_in_dir`
function in Rust for a path the newest scan found, which asks Finder to
show a file and connects to nothing (Unknown-source scan, above). The
window cannot have it open a URL: there is no homepage link; when one
ships, this paragraph changes. And the Tauri updater
plugin is compiled in and configured with the endpoint
`https://github.com/Brulek/Banager/releases/latest/download/latest.json`
(`src-tauri/tauri.conf.json`, `plugins.updater`), but nothing in Banager
calls it yet, and the window is given none of its commands
(`src-tauri/capabilities/default.json` has no `updater:` permission), so
no request to it is made; when app self-update ships, this paragraph
changes.

Showing a logo makes no network request either. The logos Banager shows
for tools and sources are built into the app: `pnpm icons:build`
(`scripts/tool-icons/build.mjs`) writes them into `src/assets/tool-icons/`
at development time, downloading the GitHub avatars among them, and
`src/lib/toolIcons.ts` imports that folder, so the app's build carries
it — `pack.json` inside the window's script, each avatar as a file of its
own that the window loads from the app, as it loads the rest of itself.
The window's content security policy was not changed for them. A logo
Simple Icons lists under a license of its own keeps that license:
`icons:build` stops, before it downloads or writes anything, when the
mapping names one under a license Banager does not ship
(`SHIPPABLE_LICENSE` in `build.mjs`), and otherwise writes that logo
unmodified — its path exactly as Simple Icons has it — with its license
and Simple Icons' source for it into `pack.json`. Settings credits each
such logo under About → Icon credits (`IconCreditsDrawer` in
`src/components/IconCreditsDrawer.tsx`), with its license and the
addresses of the license's text and of that source, shown as text: the
credits call no opener either.

Showing a tool's line in Chinese, or an npm, PyPI or crates.io
package's line in English, makes no network request either. The lines a
window shows under a tool's name, where Banager has one, are built into
the app too: `src/assets/tool-descriptions/zh-CN.json`, translated at
development time from the description each tool's own source gives it,
and `src/assets/tool-descriptions/en.json`, rewritten at development
time from the description each package's own registry gives it, both
committed. `src/lib/toolDescriptions.ts` reads each with a dynamic
`import`, which the build makes a file of its own, apart from the
window's script, that the window loads from the app only once it is in
that file's language (`useTranslatedDescription`).

The tools Banager runs make their own connections — `brew`, `npm`, `pip`,
`pipx`, `uv`, `cargo`, `cargo-binstall`, `ollama pull`, `claude update`,
`rustup self update`, `grok update --check --json` and `grok update` each
reach whatever index, registry or release server they are configured to
use. Those are the tools' connections, under the tools' configuration;
Banager neither chooses nor sees them.

## What Banager never does

- Never runs a shell for any command, and never pipes a download into one
  (`curl … | sh`). The one shell run is the `PATH` read at launch, above.
- Never runs an installer script, and never reruns one to update a tool.
- Never runs `rustup update`: rustup's own update of its toolchains, which
  an interruption leaves half installed. Only `rustup self update`, which
  replaces rustup alone. A refresh that begins while an update or uninstall
  of rustup is under way runs neither `rustup` nor `cargo`. It looks once,
  as it begins, so an update or uninstall that starts after that look can
  overlap the version reads of rustup and cargo that refresh is making
  (rustup's section); its other reads of either source run under that
  source's lock, which the operation holds until it ends. Never lets a
  version read of rustup or cargo, or a Cargo install, upgrade or
  uninstall, set off rustup's automatic install of a missing toolchain
  (`RUSTUP_AUTO_INSTALL=0`).
- Never asks rustup to uninstall from anywhere but its standard folders,
  `~/.cargo` and `~/.rustup`: rustup deletes both whole, permanently, and
  Banager offers that only when the preview can name exactly those two.
- Never runs `agy update` (undocumented, never observed), and never runs
  `grok update` from a refresh: the refresh runs `grok update --check
  --json`, which grok's own help describes as checking without
  installing; `grok update` runs only after a confirmed preview.
- Never passes `--zap`, `--force` or `--ignore-dependencies` to Homebrew
  (the brew plan test), and never runs a bare `brew upgrade`.
- Never runs a `brew` command without `HOMEBREW_NO_AUTOREMOVE=1`, which
  keeps Homebrew from uninstalling packages the command does not name,
  and `HOMEBREW_NO_INSTALL_CLEANUP=1`, which keeps an install or upgrade
  from ending in Homebrew's clean-up, which deletes the older versions of the
  package it names and of any it updates along with it, and stray old
  downloads, every time, and those of all
  Homebrew software when its periodic clean-up is due; when a `brew.env`
  file takes either back, the preview says so (Homebrew's section).
- Never runs a `brew` command as root.
- Never uninstalls a uv tool while `UV_TOOL_DIR` is set in Banager's
  environment: removing the last tool, uv would then also delete the
  folder above that one, with every file in it, when that folder holds no
  other folder (uv's section).
- Never runs a write command from a refresh, and never runs one without a
  preview the user confirmed within the last ten minutes. The `brew
  update` a refresh runs is Homebrew's exception: it can install, move or
  uninstall Homebrew packages by itself when Homebrew has moved a package
  between a formula and a cask, or renamed one (Homebrew's section).
- Never lets the window ask for an install: it can ask for the preview
  of an upgrade or an uninstall only, and `plan_operation_impl`
  (`src-tauri/src/ipc.rs`) refuses an install before any source is
  asked, whatever it names. Nor by another name: the window may ask for
  an upgrade only of an update the last check listed, and an uninstall
  only of a tool it listed installed (`Session::issue_listed_plan`),
  since npm, Cargo and Ollama would install a name they were asked to
  upgrade.
- Never launches an application from a refresh; `open -a Ollama` runs
  only when the button is pressed.
- Never opens a tool to make it update itself: a self-updating tool's row
  tells the user how, and Banager runs nothing.
- Never asks for, stores or types a password; `SUDO_ASKPASS` is passed
  through to Homebrew only when it was already set.
- Never deletes a file and never empties the Trash. Never writes a file
  on the Mac itself other than its own `settings.json`, `history.json` and
  `.window-state.json` (the programs it
  runs write their own files — Grok Build's update check writes inside
  `~/.grok` on every refresh, and the `brew update` a refresh runs
  rewrites Homebrew and its index and can install, move, uninstall and
  clean up Homebrew packages, as their sections say), and moves files
  only to the Trash, only for an uninstall the user confirmed, and only
  the paths its preview listed; never edits a shell startup file — rustup's
  own uninstall edits its startup line and deletes its two folders
  permanently, and the preview says so.
- Never moves anything outside the home folder, anything directly in the
  home folder or in a folder many tools share there (`~/.local`,
  `~/.config`, `~/.cache`, `~/Library`, `~/.cargo`), anything reached
  through a folder that is a link between where the home folder really is
  and it (the home folder itself may be reached through a link), anything
  that does not belong to the user, or anything that is not what the
  tool's uninstall list describes
  (for Claude Code, Anthropic's removal steps plus its installer's
  download cache; for Antigravity CLI and Grok Build, which publish no
  removal steps, Banager's own reading of how each was installed);
  never moves the settings, login and history Claude Code keeps
  in `~/.claude` (of that folder only its download cache,
  `~/.claude/downloads`) or `~/.claude.json`, the login, sessions, memory
  and settings Grok Build keeps in `~/.grok` (of that folder only
  `downloads/`, `bundled/`, `completions/` and the two links in `bin/`),
  or anything in Antigravity CLI's `~/.gemini/antigravity-cli` — nor
  `~/.gemini` itself, which Gemini CLI shares — nor anything they lead to.
- Never connects to an `https` host that is not on the list above, and
  never follows a redirect.
- Never sends usage data and has no account to sign in to: its only
  requests are the `GET`s in the Network table, which carry nothing about
  this Mac, and `history.json`, its record of the updates and uninstalls
  it ran, stays on this Mac.
- Never reports an operation as succeeded on the tool's exit code alone:
  the inventory is re-read afterwards, and a package still present after
  an uninstall, one missing after an install, or an upgraded version that
  did not move is reported as needing attention — the last whenever a
  version before could be read; when Homebrew's index was updating and
  the reading before was refused, presence afterwards is all there is to
  go on (Homebrew's section).
