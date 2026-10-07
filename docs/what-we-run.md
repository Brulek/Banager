# What Banager Runs

Every command Banager runs, every file it reads, writes or moves to the
Trash, every host it connects to and every environment variable it sets,
for the thirteen sources it manages today: Homebrew, npm, pipx, uv, pip
(read-only), Cargo, Ollama, and six tools with their own installer:
Claude Code, Antigravity CLI, Grok Build, rustup and Codex, and
opencode, which is listed only. Each sentence
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
four tools uninstalled by moving files to the
Trash (Claude Code, Antigravity CLI, Grok Build, Codex) name every path those
uninstalls move or keep and their time budget, and the never-list every
path of settings or state they keep, that Grok Build's section shows the
update check it runs on every refresh and says it installs nothing, that
the Trash section names the call and states the pause after each move,
that the app icons section names the call, the size an icon is drawn at,
and that no command runs for it, that this file says whether the opener
plugin is built in and names each permission of it the window has, and
the unknown-source scan's section the call Show in Finder makes, saying
it runs nothing else, that the
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
`src-tauri/src/notify.rs`'s and `src-tauri/src/notify_ops.rs`'s tests
check that the sections quote what each notification says in all three
languages, and `src-tauri/src/ipc.rs`'s that
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

**One shell run, at launch, to read PATH — and again only when that
failed.** An app opened from Finder starts with a minimal `PATH`. As it
starts (`run()` in `src-tauri/src/lib.rs`), Banager runs the user's login
shell once, in the background — `$SHELL` (`/bin/zsh` when unset) with the
arguments `-ilc 'echo -n "_SHELL_ENV_DELIMITER_"; env; echo -n
"_SHELL_ENV_DELIMITER_"; exit'`, `DISABLE_AUTO_UPDATE=true`, and the home
folder as its working directory (`runner::login_path::read`). This is the
same shell command formerly run by `fix-path-env`. Login-shell startup
files may run their own commands. `RealRunner` gives the shell 15 seconds
(`login_path::TIMEOUT`: enough for startup files that set up nvm, conda,
pyenv or oh-my-zsh, even on the first launch after login), followed by its
bounded process-group termination and pipe-drain handling described below
(including up to 5 seconds of termination grace). The window opens
meanwhile; every refresh waits for that read before it looks for sources,
and the window says it is checking. Only a complete, successful, framed
result is used. A timeout, failed spawn/exit or malformed output leaves
the inherited `PATH` in place, the session records that login PATH
discovery failed, and the Overview says so ("Couldn't read Terminal's
settings", with Check Again): sources found only through Terminal's
`PATH` — npm, pipx, uv, Cargo and the rest — may then be missing. The
next refresh — Check Again, or any later one — reads the shell once more;
after a read that worked, none runs again. Each refresh takes the `PATH`
and whether it is the login shell's together, as one value, as it starts
(`login_path::round_env`), and keeps it: a read that works while an older
round is still running changes neither for that round, which says which
copy of a command runs only when its own `PATH` was the login shell's.
From the same read Banager takes the proxy and mirror settings listed
next (`login_path::IMPORTED`), and no other variable. The `PATH` read is
kept in Banager's memory and handed to every command it runs as that
command's `PATH`, and each of those settings the shell set is handed to
every command as it was (`RealRunner::run`); Banager's own process
environment is never changed, since changing it while other threads may
read it is unsafe.

**Proxy and mirror settings from the login shell.** Opened from Finder,
Banager would not otherwise see a proxy or a mirror exported only in
`~/.zprofile` or `~/.zshrc`, and a `brew update` that works in Terminal
would time out in Banager. These, and only these, are taken from the read
above when the shell set them (`runner::login_path::read`):

- proxies, in both spellings, since programs differ in which they read:
  `http_proxy`, `HTTP_PROXY`, `https_proxy`, `HTTPS_PROXY`, `all_proxy`,
  `ALL_PROXY`, `no_proxy`, `NO_PROXY`;
- Homebrew's mirrors: `HOMEBREW_API_DOMAIN`, `HOMEBREW_BOTTLE_DOMAIN`,
  `HOMEBREW_BREW_GIT_REMOTE`, `HOMEBREW_CORE_GIT_REMOTE`,
  `HOMEBREW_PIP_INDEX_URL`;
- pip's, which pipx runs: `PIP_INDEX_URL`;
- npm's, which reads its settings in either case: `npm_config_registry`,
  `NPM_CONFIG_REGISTRY`;
- uv's: `UV_INDEX_URL`, `UV_DEFAULT_INDEX`;
- rustup's: `RUSTUP_DIST_SERVER`, `RUSTUP_UPDATE_ROOT`.

Each of these replaces where a tool downloads from. pip's and uv's extra
index (`PIP_EXTRA_INDEX_URL`, `UV_EXTRA_INDEX_URL`, `UV_INDEX`) is not
taken: it is no mirror but a second index searched beside PyPI, and an
update could then install a higher-numbered package of the same name from
it than the one Banager found on pypi.org.

A setting with no value, with a control character in it, or whose name
starts more than one line of what the shell's `env` printed (one of
those lines is then the rest of another variable's value, and which is
real cannot be told) is not taken. Banager neither changes these values
nor adds to them: a command gets them as Terminal would give them,
`no_proxy` included, and a variable a source sets itself for a command
(each source's section) still wins. Nothing that decides where things are
installed is taken -- `CARGO_HOME`, `RUSTUP_HOME`, `UV_TOOL_DIR`,
`PIPX_HOME`, an npm or Homebrew prefix -- because that would change which
folders an uninstall and its preview act on; nor any token, nor
`OLLAMA_HOST`. Banager's own requests go through the proxy these settings
name -- or, when they name none, the one this Mac's network settings name
-- but never for this Mac itself, and still only to the hosts listed
under Network: a mirror changes where the tools download from, not where
Banager's own checks ask (Network, below). The values are never written
to a log and never in the diagnostic info, since a proxy setting can hold
a password; the app's own output at launch names the settings read, not
their values.

**What a tool prints about a login.** A proxy or mirror setting can hold a
login -- `http://user:password@proxy:8080`, or a mirror's
`https://token@mirror…` -- and tools print a setting back, whole or in
part, when something about it is wrong: curl, and so Homebrew, says
`Unsupported proxy syntax in 'http://user:password@…'`; pip ends its
traceback with `Failed to parse: http://user:password@…`; git, refused,
says it `could not read Password for 'https://user@github.com'`, naming
the user alone (git 2.54); npm says ``Invalid protocol `user:` `` of a
proxy written with no scheme (npm 10.9.9). So before a line a command
prints reaches the operation's log, and before its stderr becomes a
failure's summary or a source's error, Banager masks the login in it
(`runner::redact`, which `RealRunner::run` applies to everything it hands
on), putting `****` where a secret was and leaving the rest of the line as
the tool wrote it.

It knows the logins of the settings the command was handed -- those read
from the login shell, and the same names in Banager's own environment,
which a command inherits when the shell did not set them -- and of
`OLLAMA_HOST`: Banager's own, which every command inherits, and the one an
Ollama command is given (Ollama). Each value is
read by fixed rules, never by what its parts look like:

- a scheme counts only where the value starts with one (a letter, then
  letters, digits, `+`, `.` or `-`, then `://`): in a proxy written with
  no scheme, `user:rev://secret@host:port`, a later `://` is part of the
  password;
- a proxy's login (`http_proxy`, `https_proxy`, `all_proxy`, in either
  case) is all before the last `@` of its value, as a proxy's address has
  no path, so a `/`, `?`, `#` or `@` written into its password, not
  percent-encoded, is part of it: curl cannot read such a setting and
  prints it back as written (`Unsupported proxy syntax in
  'http://user:pass/word@…'`, curl 8.7.1);
- a mirror's or a remote's login is all before the last `@` of its
  authority -- up to the first `/`, `?` or `#` -- so an `@` in its path is
  the path's: `https://mirror.example:8443/x/user@example.com/simple` has
  no login, and nothing of it is masked; nor has an intranet's
  `https://nexus:8081/repository/npm/@scope/pkg`, or an absolute name's
  `https://mirror.example.:8443/…`. When what follows that `@`, or the
  authority with no `@` in it, is no host and port -- a domain name of two
  labels or more, with or without the final `.` of an absolute name, an
  IPv4 or bracketed IPv6 address, or a name of one label with a letter in
  it (`nexus`, `localhost`), each with or without a port of digits; after
  an `@`, a name of one label only with a port, or `localhost` -- the
  rules cannot read the value: a `/`, `?` or `#` written into a password
  cut the authority short (`https://user:p@ss/word@mirror.example/` reads
  `ss` as its host). Then its login is all before the last `@` of the
  value, as a proxy's is, which masks too much rather than too little. A
  password whose first part reads as a port, a `/` written right after
  it (`https://user:1234/rest@mirror…`), or a token alone whose first part
  reads as a host (`https://tok/en@mirror…`), is read as the rules read
  it, an address with a path and no login, as the tools read it too, and
  is not masked by this rule;
- a value with no `@` where these rules look for one holds no login.

In a login, the user name and the password are both secrets, whatever
they look like: a user name can itself be a token -- GitHub's
`https://TOKEN:x-oauth-basic@github.com/…`, a mirror's `https://token@…`,
a proxy's in npm's "Invalid protocol" -- and nothing in it says whether it
is one. Each is masked wherever it appears, as written, percent-decoded
and percent-encoded; so are the whole login and the whole value (shown as
`scheme://****:****@host…`, so the host stays readable), and the
`user:password` pair as the HTTP Basic credential `curl -v` shows for a
proxy (`> Proxy-Authorization: Basic …`, curl 8.7.1). Every one of these
is found ignoring the case of its letters (ASCII), since a tool may print
a secret in another case than it was written: npm reads a proxy's user
name as the scheme of an address and prints it lowercased (``Invalid
protocol `proxytokenabcdefghijklmn:` `` for `ProxyTokenAbCdEfGhIjKlMn`,
npm 10.9.9). Two kinds of part are masked only where they stand in their
login (`:ab@`, the whole login before its `@`, the whole value):

- a user name or password of fewer than three characters: masked
  everywhere, it would turn every `ab` in a build log into `****`;
- a user name or password that is one of these words, ignoring case --
  names a host has everyone write with a token, or after one:
  `x-oauth-basic`, `x-access-token`, `x-token-auth`, `oauth2`, `oauth`,
  `gitlab-ci-token`, `__token__`, `token`; account names that are words
  tools print (`git@github.com:…` names an ssh user): `git`, `user`,
  `username`, `admin`, `root`, `guest`, `anonymous`, `proxy`, `login`,
  `test`, `default`; and sudo's words in "sudo: a password is required",
  so that sudo's lines, with the steps for Terminal beside them, stay
  readable: `password`, `required`, `terminal`, `sudo` (`COMMON_WORDS` in
  `runner/redact.rs`).

A user name alone that is that short or one of those words
(`git@github.com:…`) adds no literal rule. Inside a URL authority it is
still masked by the generic rule below. `OLLAMA_HOST`'s is the exception:
its login is always one, so even such a name is masked where it stands in
it (`ab@`), in the whole value and as HTTP Basic.
Anything else is masked wherever it appears, more than needed rather than
less: a password of digits masks a date's year that matches it, and a
user name that is the Mac account's masks it in every path a tool prints
(`/Users/****/…`), and a password that is part of a word masks that
part in every word that holds it (`pass`: "a ****word is required").
Besides, the complete login of any `scheme://user:password@` or
`scheme://user@` in the output is masked, whatever setting or file it came
from, including Git configuration. Both user name and password are hidden:
either may be a token. This generic rule ends at the authority, so an `@`
in a public path, query or fragment stays as written. It does not discover
bare tokens supplied by files outside the imported settings.

Why an operation failed is not read off what the mask left. The runner
reads it off the last five lines the tool wrote to stderr as the tool
wrote them, before the mask (`CommandOutput::failure_cause`,
`history::failure_cause`), and only the cause -- one of a few words, never
the lines -- goes on, beside the masked summary (`Outcome::Failed`'s
`cause`): so an operation where sudo wanted the Mac's password still says
so, with its steps for Terminal, in the operation bar, the Updates row, the
history and the notification, though a password `pass` masked sudo's
"password" (`needsPassword` in `src/lib/failureCause.ts`, which the window
takes from the outcome). What a check or a source failed with is still
read off its masked words: a password that is part of one of the few
phrases Banager looks for there (`timed out`, `Could not resolve host`)
can lose the word for its cause, and the message is shown as it is.

A line is masked once it has ended, so a login split across two reads is
masked whole. A transcript for a person keeps at most its first and its
last MiB (`HEAD_CAP`, `TAIL_CAP` in `runner/real.rs`), so a runaway build
log does not fill memory; where Banager drops the middle of one, the word
on each side of the cut is masked too, up to 1 KiB of it, since it may be
half a login. What a parser reads -- a command's JSON on stdout, and the
login shell's `env` output the settings come from -- is not masked:
masking it would hand every command `****` as its proxy's password. Where
a reason quotes it -- a standalone tool's update check whose answer
Banager cannot read, shown on the source's row and in the diagnostic
info -- the quote is masked first, then cut to length
(`parse_update_check` in `adapters/standalone/latest.rs`).
The settings are handed to the commands unchanged; only what they print is
masked. Masking works on the text: a login a tool printed in some other
form -- broken across two lines, or encoded some way not listed here --
would not be caught.

**What a command inherits.** A child gets Banager's own environment — the
`PATH` and the proxy and mirror settings above, the only variables taken
from the login shell (set on each command, not in Banager's own
environment), and whatever else Banager itself was started with — plus
the variables listed in each source's section below (`RealRunner::run`
adds them with `envs` and never clears the environment). Opened from
Finder or the Dock, Banager starts with macOS's small default
environment, so any other variable exported only in a shell startup file
— `CARGO_HOME`, `RUSTUP_HOME`, a token — does not reach the commands
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
(`crates/banager-core/src/runner/path_env.rs`) reads `PATH` (the login
shell's, once read), `HOME`,
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
Those fixed paths are looked up the same way, one step at a time
(`route::probe_strict`, through `protected::look`), and so is every other
path Banager looks at by itself: the links around a tool's launcher and
the program they lead to (`route::one_hop`, `route::leads_to_program`),
Codex's release link and marker (`release_link::read`), every path a
path-list uninstall checks and the kept paths and the way to what they
lead to (`removal`), rustup's two folders, toolchains, Cargo's `bin` and
the shell startup files its preview reads (`rustup`), each file a tool
wrote that Banager reads -- `<CARGO_HOME>/.crates2.json`, Ollama's model
manifests, `~/.claude/settings.json`, Homebrew's `brew.env` files and
trust list, a cask's receipt and saved caskfile, an app's `Info.plist`
(`read_file`) -- the folders of a cask's `Caskroom` record, npm's global
prefix (what `npm prefix -g` printed, whose `lib/node_modules`, `lib` or
prefix Banager asks `access(2)` of from the folder held open), pip's
developer folder, `Ollama.app` in `/Applications` or `~/Applications`,
Homebrew's three fixed paths and its update lock, the folder an app's
icon is drawn from, and the program a command is about to run
(`RealRunner::run`). None of them is ever looked at in or through one of
the places Banager never looks into (`~/Documents`, iCloud Drive,
`/Volumes`, ...; Disk use, below): a step into one is not taken, and if
a person has made one of these paths, or a folder above it, a link into
one, Banager answers as it does for a path it may not read -- and where
a program Banager runs may still read it, the preview says the most that
program may do rather than the least. A launcher there is not listed, a
Codex among them: when `~/.codex`, or its `releases/`, is kept in such a
place, its launcher leads there, and the Codex row is not listed at all
(Codex's section); an uninstall preview follows a kept path only as far
as such a place's edge -- nothing it moves is ever inside one (Claude
Code's section says what that leaves) -- and takes a listed path reached
through one as not what the list describes; a `brew.env` file there is
taken as one that may undo Banager's settings (Homebrew's section); a
file a tool wrote there is not read; npm's prefix there is treated as one this account
cannot write to (read-only); a Homebrew or a program there is not found
and never run. `crates/banager-core/tests/safety_source_test.rs` holds
every production file to this: a path looked up any other way fails it,
but for the few its `PATH_LOOKUPS_ALLOWED` names with why -- Banager's
own `settings.json` and `history.json`, `/` itself, the icon drawn for an
app folder already looked at this way, the Trash call for a path just
checked, Show in Finder of a path the scan resolved and just found again
this way, a command's working
folder (no command is given one), and a debug build's look at the
Trash.

The places are those of the home folder `HOME` names -- as given and
where its own links lead -- and of the account's own home folder, which
Banager takes from the password database (`getpwuid_r` of the real user
id, once; `protected::account_home`): a `HOME` set to another folder from
a terminal keeps both out, and a `HOME` that is empty or relative names
no home folder at all, so only the account's are kept out then
(`Protected::new`). Where `HOME` is the account's folder, as it is when
Banager is opened from the Finder, the list is the one folder's. What
Banager does not look at, the programs it runs may: the login shell it
runs at launch reads your startup files, and `brew`, `npm`, `pip` and
`cargo` read their own folders, as they do when run from Terminal.

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

**Passwords.** Banager never asks for or types a password. It passes
`SUDO_ASKPASS` through, unchanged,
to Homebrew cask installs and upgrades when the variable is already set
in Banager's environment (Homebrew's section); it never sets it on its
own behalf. An `OLLAMA_HOST` URL may already contain a login, which is
sent to that daemon as HTTP Basic authentication (Network). Its userinfo
is omitted from public instance ids and masked in command previews,
operation summaries and inventory errors. It is also removed from keys
before history or settings are stored; older records
are scrubbed on load and a rewrite is attempted (Files Banager writes).
A proxy setting read from the login shell may hold a login;
Banager hands it on unchanged to the commands it runs and gives it to that
proxy alone (Network), never writes it to a log or the diagnostic info,
and masks it in what those commands print before anything shows or keeps
it (What a tool prints about a login, above). Its commands run with no terminal (stdin is `/dev/null`), so
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
after which it has to be previewed again. This checks the time from issue
to submission; accepted work may wait longer in the queue before running.
Rust holds at most 1,024 plans
(`MAX_ISSUED_PLANS` there), letting the oldest go past that, so Update
all of more updates than that has lost its first plans by the time it is
confirmed. Each of those is then planned again at its turn, through the
same `plan_operation` and every refusal below. One no longer offered an
update by then -- an earlier update of the batch updated it as a
dependency, say -- is still planned again while it is installed under the
same source, kind and name and its update was previewed less than ten
minutes before: apart from the plans, Rust keeps for each update it
previewed the version it was offered and when it was previewed
(`ListedUpgrade` in `crates/banager-core/src/session/plans.rs`), and each
planning request drops what is older than ten minutes or neither
installed nor offered. The plan made again from it keeps that preview's
time, so it lives no longer than the plan shown, and is aimed at
that offered version: if the tool is already there when its turn comes,
the update is reported as done, as any update an earlier one got to
first is (Homebrew's section). A
tool installed but not previewed with an update in those ten minutes is
refused as before. The new plan starts only if it is field for field the
one that was shown, and only while the
batch is no more than ten minutes old, counted from when the
confirmation asked for its first plan. That age is checked again once the
new plan is back, just before it is submitted, so a slow re-planning
cannot stretch the ten minutes. Otherwise it is refused and
nothing runs (`startShown` in `src/lib/heldPlans.ts`). Only an upgrade's
command is planned again this way. The window can ask for the
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
so the version before can be compared with the version after. That
reading is taken holding the locks, and when it finds the tool no longer
installed the update stops there and nothing runs (`GoneBeforeUpgrade`):
an npm, Cargo or Ollama update of a name that is not installed would
install it. A reading that fails, as Homebrew's does while `brew update`
is still running (`IndexUpdating`), is not taken as absence: the update
runs, and the reading after it decides as before. Submitting an update
also refuses it (`not_listed`) when a refresh since its preview read its
source without failing or deferring and found the tool neither installed
nor offered an update under the same source, kind and name; an update no
longer offered because an earlier one already did it still starts while
the tool is installed. Operations
that need the same lock start in the order they were confirmed: one waits
while an earlier one that needs any of its locks is still waiting
(`run_operation`'s queue in `crates/banager-core/src/ops/mod.rs`). An install
after which the package is not present, an uninstall after which it still
is, and an upgrade that exits 0 with the version unchanged are reported
as needing attention, with one exception for that unchanged upgrade: if
its version is already at or beyond the confirmed target, it succeeds
and says it was already updated (`already_at_target`), for every source.
Without a target it can compare against, or below that target, it still
needs attention. The one case with less
to go on: when the reading before an upgrade was refused — on Homebrew,
while a `brew update` a refresh left running is still going (Homebrew's
section) — there is nothing to compare, and an upgrade that exits 0 is
reported as a success whenever the package is still present afterwards,
whether or not its version moved. A command that was
cancelled or timed out, or that a signal Banager did not send ended
(killed from Activity Monitor, say), is reported as unconfirmed unless
the reading after settles it: an install after which the package is
present, or an uninstall after which it is gone, is reported as
succeeded. An interrupted uninstall whose package still appears present
remains unconfirmed: metadata or a launcher can survive partial removal.
A Cancel pressed before the command started — the operation already
running, its command not yet begun — is reported as cancelled whatever
the operation: nothing ran. A path-list uninstall cancelled before its first move is cancelled;
after a move it remains unconfirmed unless absence is verified. An
interrupted install still absent can be cancelled; an upgrade stopped
partway is never settled either way
(`run_plan` in `crates/banager-core/src/adapters/mod.rs`, then
`run_operation`).

**An update and another action on the same tool.** Update and Update All
leave out a tool with any unfinished operation under the same full key
(source instance, artifact kind and name) -- an uninstall queued behind
another operation, running, being cancelled or checking its result, as
much as an update -- and its row says that action in the operation bar's
words (「卸载 · 排队中」, "Uninstall · Queued") where Update was; it is not
counted among the updates being installed. How a finished update ended
still shows only while the row offers the version it was for, and a
finished uninstall shows nothing there. An update confirmation that is
already open checks the operations the window last heard of again just
before it submits each update, a re-prepared one included, and starts no
update of a tool that has one: that update is listed as not started,
「这个工具还有操作未完成。请等操作结束后再试。」 ("Another operation for
this tool hasn't finished. Wait for it to finish, then try again.") --
otherwise its upgrade would queue behind the uninstall and fail against a
tool already gone (`useUpdateOperationFor` in
`src/components/UpdateProgress.tsx`, `useUpdateConfirm` in
`src/components/UpdateConfirm.tsx`). This runs no command and adds no host,
permission or file.

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
question again and starts no second wait, and the window reads the
operations for that number once: the question it shows, or the quit it
makes when nothing is left to wait for, comes from that one read. Once
*Keep Waiting*, Escape, *Quit* or the question going by itself has
answered a question, a reply to it that comes late -- or to a question a
newer one replaced, or after the window stopped listening -- shows no
question, changes no list and quits nothing; a later quit, with a new
number, asks again. A refresh alone never
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
(「N个工具可以更新」, in Traditional Chinese 「N個工具可以更新」) in the
window's language, N being every update
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
page sends the number of its newest operation (`report_finished_run`
in `src-tauri/src/notify_ops.rs`). The legacy kind/count fields remain in
the request for wire compatibility but are ignored. Rust derives them
from its own completed operation records, over the interval after the
last accepted boundary through this boundary (initially starting at 1).
A missing, duplicate, unfinished or already accepted boundary is rejected
without advancing notification state; an operation submitted whose
record is not in yet counts as unfinished. Each time an operation is
submitted, Rust drops its oldest finished operation records until it
holds 200 (`DEFAULT_MAX_RECORDS` in `crates/banager-core/src/ops/mod.rs`):
a target, not a bound, since the record of an operation not yet finished
is never dropped, and records that finish after it stay until the next
submission. Of each record it drops, it keeps, in memory only, what the
operation did and how it ended, until a run that includes it is
accepted, so a long run or a late report is still counted whole. It
keeps at most 10,000 of those (`MAX_EVICTED`); a run with an operation
dropped past that is accepted telling of nothing, so it never holds back
the runs after it. Rust posts one notification only when
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
"Updated N tools" (「已更新N个工具」; in Traditional Chinese
「已更新N個工具」) when every one worked, and otherwise each way they
ended, "N updated, N couldn't be updated" (「N个已更新，N个未能更新」;
「N個已更新，N個未能更新」), with "N need attention" (「N个需要查看」;
「N個需要查看」) for one the tool said worked and Banager could not
confirm. An update that stopped where `sudo` wanted the Mac's password,
which Banager cannot ask for, is told of as the operation bar and the
Updates page tell of it, not as one that couldn't be updated: "N updated,
N need your password" (「N个已更新，N个需要输入密码」;
「N個已更新，N個需要輸入密碼」). Rust reads that cause off the last
lines the update wrote to stderr, as the history does
(`history::failure_cause`), and counts it beside the run, from the
same records the run is counted from
(`ReportedRuns::accepted` in `crates/banager-core/src/notify_operations.rs`);
what Banager keeps of an operation whose record it no longer holds keeps
that cause too (`ops::Ended::NeedsPassword`). Uninstalls say
"Uninstalled" and "couldn't be uninstalled" (「已卸载」, 「未能卸载」;
「已解除安裝」, 「未能解除安裝」).
It is handed to macOS the way the update notification is (`notify::post`):
no command runs, nothing connects, and Banager writes no file for it. A
click on it brings Banager to the front, and with the window closed or in
the Dock brings the window back as it was left
(`NotificationPending::set_window` in `src-tauri/src/window.rs`).

After a successful operations-list response, the window also retires the
finished times, display names, update targets and cleared-row IDs of
operations no longer listed (`listOperations` in `src/lib/api.ts`,
`pruneOperationMetadata` in `src/store/ui.ts`). Metadata needed by an
open log or uninstall result is kept. A list cannot retire metadata
added after it started; a list that overlaps a submission still waiting
for its response, a failed list and a list a newer one has overtaken
retire nothing.

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

Before install, upgrade or outdated runs — the commands `bin/brew` runs
`brew update --auto-update` before (`setup-auto-update`,
`utils/auto-update.sh`) — Banager replays the `brew.env` files described
below. One that sets `HOMEBREW_NO_AUTO_UPDATE` to nothing refuses the
command, so it cannot start an automatic update outside Banager's tracked
update task, where a timeout or a Cancel could stop it halfway. Install
and upgrade check this both at preview and immediately before execution.
The shell treats even `0` and `false` as non-empty here. A `brew.env` in,
or reached through, a protected place is not read, and does not refuse
anything: the check runs, and an install's or upgrade's preview says
first that Homebrew may update itself and its list of software before it
starts, with why (`Warning::HomebrewMayAutoUpdate`). Such a file matters
only if it sets the switch to nothing, and even then Homebrew, by
default, updates only when its last fetch is more than a day old
(`HOMEBREW_AUTO_UPDATE_SECS`; less with `HOMEBREW_NO_INSTALL_FROM_API` or
a tap-qualified name), which the update a refresh runs every six hours
(below) normally prevents.

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

**Old versions** (the author's decision U9, 2026-10-06). With
`HOMEBREW_NO_INSTALL_CLEANUP=1`, an upgrade used to leave the version it
replaced in `<prefix>/Cellar/<name>`, and `brew uninstall` without
`--force` deletes one version only (`cli/named_args.rb:567-601`,
`uninstall.rb:46-69`): an uninstalled formula with an older version left
came back on the Installed page, still installed. So:

- After `brew upgrade --formula {name}` exits 0, Banager runs
  `brew cleanup {name}` under the same environment
  (`PlanAction::CommandThen`). With a name, `brew cleanup` deletes that
  formula's installed versions older than the one now installed that are
  not linked, pinned or still needed, its downloads in Homebrew's cache
  that are outdated or older than `HOMEBREW_CLEANUP_MAX_AGE_DAYS` days
  (120 unless set), and every download in the cache's `downloads` folder
  that nothing refers to any more, whichever package it was for and
  however recent (`Cleanup#clean!` with names, `cleanup.rb:497-517`;
  `cleanup_formula`, `:564-571`; `cleanup_unreferenced_downloads`,
  `:709-733`; `Formula#eligible_kegs_for_cleanup`). `brew cleanup` has no
  option that keeps that last part to the one formula, so the preview
  says it (below) rather than Banager running another command to avoid
  it; that part deletes nothing installed. With a name it runs no periodic
  clean-up and no autoremove. A cask of the same name has its outdated
  downloads in the cache deleted too (`cleanup_cask`, `:581-588`): the
  cache only, nothing installed. This is what Homebrew does by itself
  after an upgrade when `HOMEBREW_NO_INSTALL_CLEANUP` is not set
  (`Cleanup.install_clean!`, `:361-389`), for that one formula. It runs
  only when (`BrewAdapter::cleanup_after_upgrade`): the upgrade is a
  formula's, not a cask's, and not an install; the `brew.env` files leave
  Banager's `HOMEBREW_NO_INSTALL_CLEANUP=1` standing and none of them is
  unread (otherwise Homebrew cleans up by itself, or may, and the
  preview's lines say so); the person has not set
  `HOMEBREW_NO_INSTALL_CLEANUP` themselves, in a `brew.env` or in
  Banager's environment -- what Homebrew would make of the switch without
  Banager's `1`; `HOMEBREW_NO_CLEANUP_FORMULAE` does not name the formula,
  by the name Homebrew checks (one it names by an alias Banager cannot see
  is refused by `brew cleanup` itself, `cleanup.rb:511-514`, which still
  exits 0: the log then says which versions are left, below); the
  formula is not pinned and its pin record could be looked at (a pin
  keeps a version); and the names of its versions were read (below).
  Each of these is asked again right before the cleanup runs, once the
  upgrade has exited 0 (`BrewAdapter::cleanup_allowed`): a `brew.env`,
  a pin or the Cellar can change while the confirmation is open or the
  update runs, and `brew cleanup` with a name ignores
  `HOMEBREW_NO_INSTALL_CLEANUP` (`cleanup.rb:497-519`) and, for a pinned
  formula, skips only the pinned version itself: the formula's other old
  versions it can still delete (`Formula#eligible_kegs_for_cleanup`,
  `formula.rb:3760-3793`), which is why Banager runs no cleanup for a
  pinned formula. When the answer is no longer yes -- the person
  turned the cleanup off or named the formula since the preview, pinned
  it, a `brew.env` took Banager's `1` back or can no longer be read, or
  the Cellar cannot -- or the Cellar now holds an old version the preview
  did not name (one installed in Terminal while the confirmation was open:
  every version there but the newest, the one the update put in, must be
  one the preview named; review r7 F3) -- the cleanup does not run, and
  the log says so (`LogNote::OldVersionsCleanupSkipped`, 「没有运行brew
  cleanup：确认窗口打开后，Homebrew的设置或已安装版本有了变化，或无法核对。……」);
  the update's outcome is unchanged. The update's preview says
  first which versions go -- every version installed when it looked, the
  one the update replaces among them -- and, in the same line, that
  Homebrew also deletes the downloads in its cache it no longer uses,
  other tools' too, without calling those outdated
  (`Warning::HomebrewCleansUpOldVersions`,
  「更新后会删除旧版本1.25.0。Homebrew还会删除缓存里已不再使用的下载文件，
  包括其他工具的。」, the command behind its ⓘ) -- and shows
  both commands. How the cleanup ends never changes the update's outcome:
  its lines go to the log after one saying it starts, and when it does
  not exit 0, is stopped (Cancel, or `CLEANUP_TIMEOUT_SECS`, 600 s) or
  cannot be started, one more says it did not finish, that the update
  itself is done and that what it did not delete is still listed as the
  tool's other versions (`LogNote::CleaningUpOldVersions`,
  `OldVersionsNotCleanedUp`). When it exits 0, the Cellar is read again,
  as for the preview, and the versions the preview named that are still
  there, but for the newest there (the one the update put in), are named
  in the log (`LogNote::OldVersionsKept`): `brew cleanup` keeps, and
  still exits 0 for, a formula an alias in `HOMEBREW_NO_CLEANUP_FORMULAE`
  names and a version it still needs -- linked, kept by a `keepme`, the
  newest HEAD (`Formula#eligible_kegs_for_cleanup`). A Cancel that lands
  after the upgrade and before the cleanup starts runs no cleanup; the
  log says it did not finish.
- An uninstall of a formula with more than one version installed and no
  pin passes `--force`, Homebrew's own way to delete every version
  (`cmd/uninstall.rb:45`, `uninstall.rb:32-44`), and its preview names
  each (`Warning::HomebrewRemovesEveryVersion`, 「已安装的所有版本都会删除：
  1.24.0、1.25.0。」). `--force` deletes every version Homebrew finds when
  it runs (`uninstall.rb:31-43`), not the ones the preview named, so the
  Cellar and the pin record are looked at again right before the command
  runs: an uninstall of a formula with a version installed that the
  preview did not name -- an update run in Terminal since then, with its
  cleanup off, say -- or pinned since its preview, or whose Cellar or pin
  record cannot be looked at then, runs nothing and ends as
  「未能开始：确认窗口打开后，Homebrew里的{name}有了变化」, asking for the
  confirmation to be opened again (`Fault::FormulaChanged`,
  `BrewAdapter::require_kegs_as_previewed`); fewer versions than the
  preview named deletes nothing it did not name, and runs. `--force`
  leaves Homebrew's check of what still depends on the formula
  (`uninstall.rb:25-28`, over every version) and the autoremove switch as
  they are. It skips Homebrew's refusal of a pinned formula, so it is
  never passed when `<prefix>/var/homebrew/pinned/<name>` is there or
  cannot be looked at, and the pin is looked at again as above;
  its refusal of a name with nothing installed (`cmd/uninstall.rb:138`),
  which the reading after the uninstall answers anyway; and the lock
  Homebrew takes on each version while it deletes it (`uninstall.rb:56`),
  which keeps a `brew` command on the same formula, run in Terminal at
  that moment, from overlapping it -- Banager's own operations on one
  Homebrew never overlap. With one version installed, or a pin, the
  uninstall is the plain `brew uninstall --formula {name}`, which deletes
  the one version `opt/` points to (`resolve_default_keg`,
  `cli/named_args.rb:567-578`) and which its preview says removes "this
  version". So the Cellar is read again before it runs too: a second
  version installed since the preview, with no pin -- which may be the
  one deleted, while the one the preview showed stays -- runs nothing and
  ends the same way (`Fault::FormulaChanged`). A pin since then is left
  to Homebrew's own refusal, and a Cellar that cannot be read then runs
  as before.

Both read, during the upgrade and the uninstall preview of a formula,
again right before an uninstall with `--force` runs, and right before
the cleanup that follows an update runs and again after it exits 0, the
names in
`<prefix>/Cellar/<name>` -- its versions, the folders there -- and whether
`<prefix>/var/homebrew/pinned/<name>` is there (`brew::kegs`, `lstat`
only); `<name>` is the last part of a tap's `user/tap/name`.

**Keg-only formulae linked into Terminal** (the author's request of
2026-10-07, "能否自动修复", after `node@22`'s update took `node` -- and
with it `npm`, a script that starts with `#!/usr/bin/env node` -- out of
Terminal; corrected after review). A keg-only formula is one Homebrew
leaves out of `<prefix>/bin` on purpose: `node@22`, `openssl@3`. It is
linked into the prefix all the same by `brew link --force {name}`; by
Homebrew itself, for a versioned one installed on request with no other
version of it there (`FormulaInstaller#auto_link_versioned_keg_only?`,
`formula_installer.rb:1923-1934`, in Homebrew 7.0.8); or because it was
linked before it turned keg-only (`openssl@3` did with 3.6.5) -- all three
leave Homebrew's record of the link, `<prefix>/var/homebrew/linked/<name>`
-- or by a person's own links. Which of these made a link Banager cannot
tell, and does not say. What Homebrew's upgrade does with it turns on the
record alone (`Keg#linked?`, `keg.rb:274-278`):

- With the record, it first unlinks the version it replaces
  (`Upgrade.outdated_kegs`, `upgrade.rb:268-272`; `install.rb:632-641`):
  each link whose one-level target -- its text joined to its folder,
  nothing followed (`Utils::Path.resolved_path`, `utils/path.rb:84-85`)
  -- is that version's own file goes, and nothing else does (`Keg#unlink`,
  `keg.rb:361-391`, the test at `keg.rb:376-377`). Then it links the new
  version (`Upgrade.create_formula_installer`, `upgrade.rb:635-643`), and
  any place that is there and is not its own link or a cask's stops
  `Keg#link` (`Keg::ConflictError`, `keg.rb:823-861`), which unlinks what
  it had linked, and the upgrade fails with "The `brew link` step did not
  complete successfully" (`FormulaInstaller#link`,
  `formula_installer.rb:1281-1347`). On 2026-10-07 that place was
  `<prefix>/bin/npm`: an update of npm, run by npm, had put its own copy
  of itself in `<prefix>/lib/node_modules/npm` and its link in
  `<prefix>/bin/npm`, in the same batch, before node@22's turn.
- Without it, the upgrade unlinks and links nothing; only
  `<prefix>/opt/<name>` moves to the new version. A person's own link
  through it (`ln -s ../opt/node@22/bin/node <prefix>/bin/node`) follows
  it and keeps working; one straight into the version replaced keeps
  leading to that version until it is cleaned up (`brew cleanup` keeps an
  old version only where the record leads, `formula.rb:3767-3793`), and
  then to nothing.

So, for the upgrade of a formula that the last
`brew info --installed --json=v2` called keg-only (`keg_only`) -- but not
one keg-only because of macOS (`keg_only_reason` `:provided_by_macos` or
`:shadowed_by_macos`), which `brew link` refuses to link at Homebrew's
default prefix (`cmd/link.rb`) -- and whose record is there
(`BrewAdapter::relink_after_upgrade`; with no record the upgrade is
planned as any other's):

- Where every one of its commands' places is free or holds Homebrew's
  own link to it, the preview says first that it is linked into
  Terminal, that the update unlinks it and Homebrew links it back, and
  that it is checked after (`Warning::HomebrewRelinksAfterUpdate`,
  「node@22已接在终端里。更新会先断开它，再由Homebrew接回；更新后会检查，没接回就把它接上。」;
  behind its ⓘ the commands and the command), and shows the upgrade,
  `brew link --formula --force {name}` and, when one follows, the
  cleanup, in the order they run. Once the upgrade has exited 0 the links
  are read again: where Homebrew linked it back -- which an upgrade of a
  recorded formula that exits 0 has done -- no command runs and the log
  says so (`LogNote::StillLinkedAfterUpdate`); otherwise, unless Cancel
  was pressed, `brew link --formula --force {name}` runs
  (`RELINK_TIMEOUT_SECS`, 300 s; `LogNote::RelinkingAfterUpdate` before
  its lines). `--formula` keeps the name to the formula, never a cask of
  that name, whose links `--force` would overwrite; for a formula
  `--force` only lets `brew link` link a keg-only one. Without
  `--overwrite` it stops at another program's file rather than overwrite
  it -- a file in the way makes it link nothing and exit 1 (`cmd/link.rb`,
  `Keg#link`) -- but for two things Homebrew does on its own: it replaces
  a link a cask put there (`keg.rb:851-856`, `Keg#record_cask_symlink`,
  `keg.rb:878-893`; only `bin` and `sbin` are read beforehand, where such
  a link counts as in the way), and it first unlinks the formulae it
  names as linking over this one -- its other versions and its
  unversioned twin, `node` for `node@22` -- where they are linked
  (`Unlink.unlink_link_overwrite_formulae`, `unlink.rb:8-17`, from
  `cmd/link.rb:115`; Homebrew's own link after the upgrade does the same,
  `formula_installer.rb:1297`). Then the links are read once more, and the
  commands the preview named that no longer lead into it are named in the
  log with how to link it back (`LogNote::NoLongerLinked`,
  「node@22没有重新接到终端里，输入node、npm不再运行它。……」). How the link
  ends never changes the update's outcome. A formula whose record is gone
  since the preview is not linked again.
- Where something else is at one of those places -- a file, a link that
  leads anywhere else (npm's own copy of itself in `<prefix>/bin/npm`
  among them), or a person's own link into the formula, which the
  unlink leaves and the link stops at -- the update would take the
  formula's commands out of Terminal with no way to link them back short
  of deleting that file. So a check marks the update as one that cannot
  be done here (`UpdateBlocked::LinkTaken`, 「无法重新接上」, no button),
  with the places it found (`Warning::LinkPlacesHeld`, said under the
  row's sentence as 「挡住它的文件：/opt/homebrew/bin/npm等2个」), its
  preview is refused, and an update already confirmed whose place was
  taken since ends as
  「未能开始：/opt/homebrew/bin/npm等2个文件已被另一个程序占用」
  (`Fault::LinkTaken`, `BrewAdapter::require_link_places_free`), the
  formula still linked as it was. Banager deletes no such file. And an
  npm operation never runs while a brew one on the same prefix does:
  each npm plan also takes the lock of a Homebrew at npm's global prefix
  (npm's section), so an update of npm in the same batch cannot land
  between Homebrew's unlink and its link, where no check could see it.
- An upgrade that fails or is stopped after Homebrew unlinked the formula
  is followed by the same reading, and the same log line names what is no
  longer in Terminal.

The same link runs on its own for the formula a source's notice offers,
once its preview is confirmed (`OpKind::Link`, `NoAnswer::link_fixes`;
why it is offered: Why a source did not answer, below): `brew link
--formula --force {name}` (`link_argv`, `LINK_TIMEOUT_SECS`, 300 s; no
password, no download, no update of Homebrew). It is planned only for a
formula the last inventory listed as keg-only and linkable -- not one
keg-only because of macOS, which `brew link` refuses at Homebrew's
default prefix while exiting 0, nor one that is not keg-only -- and
refused otherwise (`AdapterError::Unsupported`). Its preview reads the
formula's links as an update's does (`brew::links`) and says the
commands it links into that Homebrew's prefix (`Warning::LinkPutsCommands`,
`KegLinks::command_names`: what it links of the keg's `bin` and `sbin`;
where Terminal does use them, linking `node@20` changes which `node`,
`npm`, `npx` and `corepack` it runs for everything, not only for the
source that needed it). That list says what the link creates, not which
command Terminal will run: Terminal uses a linked command only when its
folder is on Terminal's `PATH` and no command of the same name comes
earlier, and this preview checks neither. So the sheet names the prefix
the links go under and states that condition, with the list or without
it, and never promises which copy or version Terminal will run; Banager
never edits `PATH` or a shell startup file to make it so. The preview
also names every place it would stop at -- the same places that make an update
`UpdateBlocked::LinkTaken`: a file, a link that leads anywhere else, a
person's own link into the formula (`Warning::LinkConflicts`,
`KegLinks::held_paths`). Where there is one, the preview offers no Link
button, `Session::submit` refuses the plan whatever the window sends
(`SubmitError::LinkBlocked`), and the sheet shows instead, as text to
copy, `<brew> link --formula --force --overwrite {name}`, which deletes
what is in the way, for the person to run in Terminal if they choose,
and Check Again for after; Banager never runs it. An unrecorded keg that
already has direct command links (`Place::Linked`) is also blocked
(`Warning::LinkRollbackRisk`, `KegLinks::rollback_paths`): if linking later
fails, Homebrew rolls back by unlinking every matching link to that keg,
including links that existed before the attempt (`Keg#link` rescue calls
`Keg#unlink`). The sheet names those links and explains that risk in all
three languages. When this is the only blocker, its copy-only command is
`<brew> link --formula --force {name}`, without `--overwrite`; when conflicts
also exist, the existing overwrite handoff and its deletion warning remain.
`Session::submit` refuses either warning. Immediately before a standalone
link executes, after waiting for Homebrew and under the operation's resource
locks, the same links are read again. Newly present rollback-risk links stop
the command (`Fault::LinkRollbackRisk`), leaving them untouched. Conflicts
outside `bin` and `sbin` are still left to Homebrew; Banager does not preserve
or restore links, and cannot prevent another process changing them after its
last reading. Only command links are read: where a link stops, Homebrew
also takes back the formula's own links already in the prefix's other
folders (a manual page in `share/man`, say), which no preview names.
After it its links are read again, and it counts as done only where
Homebrew's record is there and every command's place holds Homebrew's
link to it (`KegLinks::fully_linked`, the reading that tells whether the
link after an update is needed; `Adapter::reconcile_link`, no command
runs): `brew link` also exits 0 having linked nothing ("Refusing to link
macOS provided/shadowed software"), which is said as needing attention
(`Attention::NotLinkedAfterLink`). A link is not kept in the history: it
updates and uninstalls nothing.

"Linked", for the update and for the link on its own alike, is
Homebrew's record of a `brew link`, `<prefix>/var/homebrew/linked/<name>`
(`Keg#linked?`): read off the disk at a preview and around a run
(`KegLinks::recorded`), and, in a refresh's snapshot, as `brew info
--installed --json=v2`'s `linked_keg`, which is Homebrew's own reading of
that record (`Formula#linked_keg`, `linked_version`,
`formula.rb:1004-1007`, `1103-1107`, `3146`; `CommandInputs::link_recorded`),
where it decides which formulae a source's notice offers.

Not guarded: an update of another formula that depends on this one,
which upgrades it first where it is outdated, unlinking and linking it
again (`FormulaInstaller#install_dependency`,
`formula_installer.rb:891-931`), with no check before or after; a file
in the way in `lib`, `include` or `share`, which only Homebrew's own
link finds -- its failure fails the upgrade, and the log line then names
the commands it took away; and a person's own link straight into the
version replaced, with no record, which leads to nothing once that
version is cleaned up (by Banager's cleanup after the update, too) --
`brew link` would link every command of the formula, not only those, so
that is left to the person. What is read -- for an update at its
preview, right before it, after it and after its link; for a link on its
own at its preview and after it (`brew::links`, through
`protected::look`: `lstat`, `readlink` and `realpath` only): where
`<prefix>/opt/<name>` and `<prefix>/Cellar/<name>` lead and the text of
the first, the names in the `bin` and `sbin` of the keg `opt` leads to,
what is at each of those names in `<prefix>/bin` and `<prefix>/sbin`, its
link's text and where it leads, and `<prefix>/var/homebrew/linked/<name>`,
its text and whether it leads to a folder.

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
and in the files too. It also reads `HOMEBREW_NO_AUTO_UPDATE` from the
files to refuse commands that would start an untracked automatic update
(Environment applied to every invocation, above). A file's lines count as
bash reads them: the last line to set a variable wins, and a last line
with no newline after it is not read. Homebrew counts
`HOMEBREW_NO_AUTOREMOVE` as unset when it is empty, only whitespace, or
`0`, `false`, `no`, `off` or `nil` in any case (`env_config.rb:871`,
`:926`), and `HOMEBREW_NO_INSTALL_CLEANUP` only when it is empty or only
whitespace. A file Banager does not read because it is in, or reached
through, one of the places Banager never looks into (`~/.homebrew` a
link into iCloud Drive, an `XDG_CONFIG_HOME` in `~/Documents`; How
Banager runs anything, above) is not taken as absent: `bin/brew`, which
Banager runs, may still read it, so what it sets is unknown, and an
unknown variable is taken the way that says more -- `HOMEBREW_NO_AUTOREMOVE`
and `HOMEBREW_NO_INSTALL_CLEANUP` as unset, `HOMEBREW_NO_CLEANUP_FORMULAE`
as naming nothing, `HOMEBREW_NO_REQUIRE_TAP_TRUST` as unset -- until a
later file sets it outright (`brew_env::EnvFile::Unknown`); when the
folder of the user's file hangs on such a variable, what that file sets is
unknown too. The lines below then say what Homebrew *may* do, and why
behind their ⓘ: that a `brew.env` Homebrew reads is in a protected
location, or points there, and was not read (`Warning::HomebrewMayAutoremove`,
`HomebrewMayCleanUp`, `HomebrewCleanupMayAutoremove`, in place of the
three named below; the last whenever either variable is unknown, since
its line rests on both). A line whose variables are all known still says
"will", beside a "may" line. When the files leave `HOMEBREW_NO_AUTOREMOVE` unset, the
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

An install or upgrade reads them again right before its command runs,
once any wait for an update of Homebrew's is over
(`BrewAdapter::require_cleanup_as_previewed`, review of v1-brew's fixes,
r6): a file edited, or turned unreadable, while the confirmation is open
can take `HOMEBREW_NO_INSTALL_CLEANUP` back, and the command would then
end in Homebrew's clean-up where its preview said nothing of one. When
Homebrew would now delete more afterwards than the preview said -- it
cleans up, or may, and the preview had no line saying it would or might;
that clean-up autoremoves, or may, and the preview had no line for that;
or a formula the preview said `HOMEBREW_NO_CLEANUP_FORMULAE` leaves out
is no longer left out -- nothing runs, and the operation ends as
「未能开始：确认窗口打开后，Homebrew的设置有了变化，或无法读取」, asking
for the confirmation to be opened again (`Fault::HomebrewSettingsChanged`).
Less than the preview said, or "will" where it said "may", or the
reverse, runs.

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
`BrewAdapter::uninstall_scope`). For a formula with one installed
version — or a pinned one, or one whose versions or pin record could not
be read — it says that this installed version and the links to it go.
For an unpinned formula with more than one installed version
(`removes_every_version`), the plan adds `--force` and
`HomebrewRemovesEveryVersion`: the confirmation instead says that what
Homebrew installed of that formula and its links go, and lists every
version to be removed (`warningLines` in `src/lib/warnings.ts`). Config
and data kept elsewhere are not deleted. The scope omits "only" when
`brew.env` brings autoremove back, beside the line above. A cask's comes from what Homebrew
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

The fresh inventory used to qualify outdated names is indexed once per
check by kind and full/short name, preserving the first installed match
for ambiguous short names. Reconciliation before and after each operation
still runs the full installed-info reader above; it does not use a cached
inventory or a new per-package command.

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
more runs for this, and Banager never passes `--force` to an install or
an upgrade. The mark is as
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
| Uninstall a formula with more than one version installed and no pin | `<brew> uninstall --formula --force {name}` | 1800 s | No |
| Uninstall a cask | `<brew> uninstall --cask {name}` | 1800 s | Sometimes — Homebrew runs `sudo`, for example when the cask's recorded uninstall deletes paths (`delete:`), removes a background service (`launchctl:`) or a kernel extension (`kext:`), removes an installer package that is installed (`pkgutil:`), or runs a program the cask marks to run as root |
| Upgrade one formula | `<brew> upgrade --formula {name}` | 1800 s | No |
| Then, once that upgrade has exited 0, link a keg-only formula whose link Homebrew recorded back into the prefix, where Homebrew did not (Keg-only formulae linked into Terminal, above) | `<brew> link --formula --force {name}` | 300 s | No |
| Then, once that upgrade has exited 0, delete the formula's old versions (Old versions, below) | `<brew> cleanup {name}` | 600 s | No |
| Upgrade one cask | `<brew> upgrade --cask {name}` | 1800 s | Sometimes — as for install |
| Link, on its own, a keg-only formula another source's launcher could not find a program of, once its preview is confirmed (Keg-only formulae linked into Terminal, above) | `<brew> link --formula --force {name}` | 300 s | No |

Every one of these argvs is exactly the verb, the kind flag and the name
(`test_plan_never_passes_zap_force_or_ignore_dependencies` in the same
file), but for the two the author's decision U9 added (Old versions,
below) -- the `brew cleanup {name}` that follows a formula's upgrade, and
the `--force` of the uninstall of a formula with more than one version --
and the one link Banager runs (Keg-only formulae linked into Terminal,
above), `brew link --formula --force {name}` (`link_argv`), whose
`--force` is what Homebrew asks before it links a keg-only formula: after
the upgrade of one whose link Homebrew recorded, and on its own for the
formula a source's notice offers
(`test_a_link_plans_brew_link_formula_force_and_says_what_is_in_the_way`).
Banager never passes `--zap`, `--ignore-dependencies`
or `--overwrite` to Homebrew, never passes `--force` to anything but that
uninstall and that link, and never runs a bare `brew upgrade`, a
bare `brew cleanup` or a bare `brew link`: upgrades are one confirmed
artifact per invocation, a cleanup or a link after an upgrade names the
formula just upgraded, and a link on its own names the formula a source's
notice offered. Before a write command starts,
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
Claude Code's section names another. A third: one whose version, read
before its command and after, is already at least the version the
confirmed update aimed for (the check's new version, which the
confirmation showed). Homebrew upgrades a formula's outdated dependencies
before the formula itself, so in an Update all an earlier update often
upgrades a later one's package first; that later `brew upgrade` finds it
current, says so and exits 0. It is reported as done -- by an earlier
update when another update of the same Homebrew and kind (a formula's
for a formula: Homebrew upgrades a formula's dependencies, which are
formulae) that may have changed
something (its version moved, it failed, or it was stopped partway; not
one that was itself already up to date or skipped) ended after it was
confirmed, otherwise as already up to date when its turn came; on every
other source, where one update never updates another package, always
the latter -- not as
needing attention, which it is when the version is still below that
target (`already_at_target` in `crates/banager-core/src/ops/mod.rs`).
"At least" is the same string, or, where both versions are made only of
digits, dots, underscores and commas, a later one by their numbers
(`reached_target`). A version with a letter or a hyphen in it is at its
target only when it is the target: a prerelease comes before its
release, and npm's `1.2.3-1`, all numbers, would sort after `1.2.3` by
them. So an npm `1.2.3-1` offered `1.2.3` that reads `1.2.3-1` again
after an `npm install -g` that exited 0 -- with `dry-run=true` in the
person's `.npmrc`, npm installs nothing -- needs attention, and the
history keeps it so, not as updated (r11 F1). Homebrew's own hyphenated
versions (ImageMagick's `7.1.1-47`) still count when equal, which is how
`brew outdated` and `brew info` both spell them.

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
16 MiB (`read_file::LIMIT`) — otherwise it is skipped as unreadable,
as `bin/brew` skips it, but for one in a protected place, which is not
looked at and counts as unknown (`brew.env`, above);
only the lines that set `HOMEBREW_NO_AUTO_UPDATE`, `HOMEBREW_NO_AUTOREMOVE`,
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
of it. A keg-only formula's links -- for its update, and for the link a
source's notice offers -- are read as Keg-only formulae linked into
Terminal, above, says (`brew::links`): names and links only, never a
file's contents.

## Why a source did not answer, and the link that fixes it: nothing runs until a link is confirmed

A source that did not answer is `NotResponding`, and the window used to say
only that it was "not responding" -- which is true only of one that ran
and did not answer in time. On 2026-10-07 the author's npm said so. Their
`node@22` is keg-only and had been linked by hand (`brew link --force`),
so `/opt/homebrew/bin/npm` was Homebrew's link into its keg. Banager's
Update All first updated npm through itself (`npm install -g npm@latest`,
10.9.9 to 12.2.0, 09:55:57), which replaced that link with npm's own.
Homebrew does link again a keg that was linked before an upgrade
(`link_keg = keg.linked?`, upgrade.rb:643 in Homebrew 7.0.8), but when
`brew upgrade node@22` then poured 22.23.3_1 and linked it, it met npm's
`bin/npm`, which it had not linked, raised `ConflictError`, took back what
it had linked, and failed the run ("The `brew link` step did not complete
successfully", exit 1 at 09:57:31). The old version had been unlinked
first, so `/opt/homebrew/bin/node` was gone; npm's launcher,
`#!/usr/bin/env node`, could not start (`env: node: No such file or
directory`, exit 127), and nothing on screen said why. Its other links went
too: corepack's, so `/opt/homebrew/bin/pnpm`, a link into
`lib/node_modules/corepack`, led nowhere. Three things now answer for it:
the reason said here; the failed update's cause, 「新版本没有接到终端里」
(`FailureCause::NotLinked`, read off that line); and npm's own update,
which is no longer offered where npm is a formula's
(`UpdateBlocked::UpdatesWithFormula`, npm, below), so it cannot happen
again that way.

**Why.** Each package manager's `detect` hands the result of the command
that did not answer -- npm's `npm prefix -g` or `npm --version`, the
`--version` of Homebrew, Cargo, pipx and uv, pip's `<python> -m pip
--version` -- to `runner::no_answer::of`, which reads it, and nothing
else (`InstanceStatus::no_answer`, `NoAnswer`):

| What the command did | Reason (`NoAnswerKind`) |
|---|---|
| The runner stopped it when its time ran out | `TimedOut` |
| Its program is not there or macOS would not start it (`RunnerError::NotFound`, `Spawn`), or it exited 126 or 127 | `CouldNotStart` |
| It exited 127 and its last lines include `env`'s `env: <name>: No such file or directory` | `CouldNotStart`, with `<name>` as the program it needs |
| Any other non-zero exit, or a signal Banager did not send | `ExitedWithError` |
| It exited 0, was stopped by Banager's own Cancel, or wrote more than Banager reads | no reason |

A failed startup also keeps a diagnostic: the last five lines of the
runner's already-redacted stderr, capped at 4,096 UTF-8 bytes. Its recovery
category is read before masking, just as `Outcome::Failed.cause` is. The
shared source notice and Overview offer details and Copy Diagnostic;
Copy Diagnostic Info includes the same diagnostic even without the tool
list. No raw stderr or spawn-error text is added. Review or share these
details, address the error, then use Check Again.

The program's name is read off stderr as the command wrote it, before a
proxy's or mirror's login is masked out of it (`StderrCause::Read`), as an
operation's failure cause is: a proxy user name `node` would mask the very
word. A Python with no pip keeps its own reason (`NoPip`) and no other;
Homebrew under root asks nothing and has none; a tool with its own
installer, and Ollama, which is asked over HTTP, have none yet.

**The fix it offers.** For a source that could not start for want of a
program, once a refresh round has every source's rows
(`link_fixes::fill`, from the snapshot alone -- working out the reason
and the formulae offered reads nothing and runs nothing; the link itself
runs only once its preview is confirmed): the formulae of a Homebrew
source Banager can act on that are keg-only, not because of macOS, and
not linked (`keg_only`, `keg_only_reason` and `linked_keg` of `brew info
--installed --json=v2`, already read for the inventory) and named for the
program (`node`, or `node@<version>`), newest first
(`NoAnswer::link_fixes`). The source's notice says why it did not answer
and offers them; each is the one link Banager runs, `<brew> link
--formula --force {name}` -- the same command, read the same way, as the
link after a keg-only formula's update (Homebrew's section, "Keg-only
formulae linked into Terminal", which says what its preview reads and
names, when it is refused, and when it counts as done) -- and runs only
once confirmed, as every operation does. The window may ask for the
preview of a link only of a formula a source's reason offers
(`Session::issue_listed_plan`). Where the formulae offered come from
more than one Homebrew -- the same `node@20` in `/opt/homebrew` and
`/usr/local`, say -- each choice names which Homebrew it is in, by where
that Homebrew is (as the sidebar does), and the choice is the whole key
(Homebrew, kind and name), so each is planned with its own
`brew`; Link submits only the preview of the formula chosen now, never
one planned before another was chosen.

## npm

Adapter: `NpmAdapter` in `crates/banager-core/src/adapters/npm.rs`.
Verified against npm 12.0.2 (`adapters/meta/npm.toml`).

**Detect.** `npm` is the first `npm` on `PATH`. Banager runs `<npm>
prefix -g` (30 s) to learn the global prefix, which is the instance's
identity, and `<npm> --version` (30 s), then asks `access(2)` whether the
current user can write `{prefix}/lib/node_modules` — or, when that does
not exist yet, `{prefix}/lib` or `{prefix}` (`real_prefix_read_only`).
A prefix this user cannot write (a Node installed from nodejs.org's
package leaves a root-owned one) makes the instance read-only
(`PrefixNotWritable`). So does a prefix that is in, or leads into, a
protected place, such as `~/Documents`: it is not looked into, so whether
it could be written is not known, and the rows say that rather than that
the account cannot change it (`PrefixProtected`). An npm that
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
| List global packages (`inventory`) | `<npm> ls -g --depth=0 --json --prefix {prefix}` | 60 s |
| List outdated global packages (`check_updates`) | `<npm> outdated -g --json --prefix {prefix}` | 60 s |
| Search | `<npm> search --json --searchlimit 20 {query}` | 30 s |

`npm ls` exits 1 for problems it found in the packages it read (a peer
dependency mismatch): it then writes the packages, with the problems and
an `error` beside them. So exit 1 is read when stdout has a `dependencies`
object, even an empty one. Any other non-zero exit, or exit 1 with no such
object, is a failed reading, not an empty list; its reason is what npm
wrote to stderr, never stdout, which Banager reads as npm wrote it,
without masking a proxy login out of it. An answer with an `error` and no
`dependencies` is a failed reading on exit 0 too, named by npm's error
code alone (`ENOTDIR`); one with neither -- `{"name": "lib"}`, npm's own
for a prefix with nothing installed -- is no global packages. `npm outdated`
exits 1 whenever it finds a version difference, including one that is no
update (a package installed ahead of `latest`), so exit 1 with rows npm
printed is a result even when none of them is kept as an update. Any other
non-zero exit, or exit 1 with no rows (empty or error output), is reported
as "could not check" for every package rather than as "everything is up to
date" — listing every package that way takes one more run of
`<npm> ls -g --depth=0 --json --prefix {prefix}`, so
a refresh whose `outdated` failed runs the inventory command twice. The
search query passes `validate_search_query`.

**Write commands:**

| Purpose | Argv | Timeout | Needs a password |
|---|---|---|---|
| Install | `<npm> install -g {name} --prefix {prefix}` | 600 s | No |
| Uninstall | `<npm> uninstall -g {name} --prefix {prefix}` | 600 s | No |
| Upgrade | `<npm> install -g {name}@latest --prefix {prefix}` | 600 s | No |

All global reads and saved write commands carry `--prefix {prefix}`, the
answer `<npm> prefix -g` gave when the source was found (`NpmAdapter::run_npm`
and `NpmAdapter::plan`). npm ranks a setting given on the command line above
the environment and every `npmrc` (`@npmcli/config` 9.0.0, `lib/index.js:42-48`,
`:260`), so changing npm's configuration while a confirmation is open cannot
move the operation away from the prefix its permission check and locks were
taken for. Search and `<npm> prefix -g` itself run without it.

Naming the prefix has two side effects. npm then reads its global settings
file from `{prefix}/etc/npmrc` (`lib/index.js:286-293`): the same file it read
before, unless the prefix was itself set in a global `npmrc` somewhere else,
whose other settings these commands then do not read. And Volta's `npm`
passes a global command that names a prefix straight to npm instead of moving
it into Volta's own package folder (`has_global_without_prefix`, Volta 2.0.2
`crates/volta-core/src/run/parser.rs:466-488`), so an update or removal lands
in the folder its row was listed from.

A plan is refused at click time, with the same reason, if the prefix has
stopped being writable, or is now in a protected place, since the refresh
that listed it.

Every npm plan takes two locks: npm's own, and that of a Homebrew at
npm's global prefix (`brew:{prefix}`; y1-keg review). An npm that came
with a Node from Homebrew writes into Homebrew's prefix --
`<npm> install -g npm@latest` puts its own `bin/npm` there -- where a
Homebrew upgrade of that Node unlinks and links again, and stops at any
file in the way (Homebrew's section, "Keg-only formulae linked into
Terminal"). With both locks no npm operation runs while a brew one on the
same prefix does, in Update All too. Where no Homebrew lives at that
prefix, no other plan takes the second lock. Both adapters use the same lock
helper. It matches case variants with `protected::same_path` and symbolic-link
aliases by directory device and inode, using protected, read-only path lookups,
then reuses the fixed Homebrew discovery prefix in the lock string. A prefix
that cannot be looked at (not there, not a folder, or in a protected place)
keeps the lock its own spelling names and is never matched to another that
cannot be looked at. Display paths and npm argv retain the user's spelling.

npm's own package, `npm`, is never uninstalled: `<npm> uninstall -g npm`
would remove the npm every other package is updated and uninstalled with.
Its row in the list says so where Uninstall would be
(`UninstallBlocked::SourceProgram`, set by `parse_ls_global`), and both the
gate and `NpmAdapter::plan` refuse it. Its update is offered as any
package's, but for one case: where the `npm` in the prefix's `bin` is a
Homebrew formula's -- a link that leads, every link followed, into
`<prefix>/Cellar/`, which is what `brew link --formula --force node@22` puts there
-- its update is not offered (`UpdateBlocked::UpdatesWithFormula`), and
`NpmAdapter::plan` refuses it too. `<npm> install -g npm@latest` would
replace that link with npm's own, and the formula's next `brew upgrade`
could not link its new version over it: Homebrew stops at a file it did
not link, and leaves no `node` where Terminal looks -- what happened on the
author's Mac on 2026-10-07 (Why a source did not answer, above). Such an
npm updates with its formula. To tell, `check_updates` (only when it lists
npm's own update) and the plan of that update read where
`<prefix>/Cellar` and `<prefix>/bin/npm` lead (`real_npm_comes_with_formula`
in `npm.rs`): each link one step at a time, never into or through a
protected place, never a file's contents; anything it cannot tell is not
a formula's. The unversioned `node` formula's npm is a copy in
`<prefix>/lib/node_modules/npm`, outside the Cellar, and keeps its update.

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
pipx runs `pip list --outdated` in each tool's environment and keeps that
command's error output to its own debug log, so a lookup pip gave up on
there (as under pip, below) reaches Banager as no update; Banager cannot
tell it from one, and reads no pipx log to find out.

On a pipx older than 1.16, which has no `list --outdated`, Banager
instead asks PyPI about each installed tool: `GET
https://pypi.org/pypi/{name}/json` (30 s each), the name percent-encoded.
This is the recorded main-package name, not a suffixed environment alias;
upgrade and uninstall still address that environment alias. Only a strictly
newer PEP 440 version produces an update: equivalent spellings, older
stable versions than an installed prerelease, and lower epochs do not.
An invalid or numerically unrepresentable version is "could not check".
A tool PyPI does not answer for is listed as "could not check", never as
an error for the whole source. pipx has no search command Banager uses.

**Write commands:**

| Purpose | Argv | Timeout | Needs a password |
|---|---|---|---|
| Install | `<pipx> install {name}` | 600 s | No |
| Uninstall | `<pipx> uninstall {name}` | 600 s | No |
| Upgrade | `<pipx> upgrade {name}` | 600 s | No |

Before planning any upgrade, Banager repeats the existing `<pipx> list --json`
read and refuses a tool whose `main_package.pinned` is true. On older pipx,
the same metadata marks PyPI fallback candidates as pinned; pins do not
prevent uninstalling. No extra network request is needed for that check.

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
additional environment is needed for detection. Parsed `tool list`
commands explicitly set `NO_COLOR=1`, including when the process inherits
`FORCE_COLOR`. Unrecognized nonempty output is a parse error rather than
a successful empty inventory. Banager makes no network request of its own for uv: `uv tool list --outdated` reaches PyPI
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
path. When outdated rows exist, inventory is also read to locate each
tool's environment. Banager reads `<tool environment>/uv-receipt.toml`
through the bounded, protected regular-file reader (at most 16 MiB),
and inspects `[tool].requirements` for the main package, and the
constraints and overrides saved at install (`[tool].constraints`,
`[tool].overrides`, from `--constraint` and `--override`) for any entry
naming it, names compared as PEP 503 normalizes them: `uv tool upgrade`
restores both, while `uv tool list --outdated` looks for the latest
release without them. Only an ordinary index requirement without a
version constraint, with no saved constraint or override naming the main
package, is currently actionable. Pinned, bounded, missing, malformed or
unsupported requirements, a saved constraint or override naming the main
package, and a saved constraint or override Banager cannot read are
"could not check": Banager cannot prove that the offered latest target
is compatible. Constraints and overrides naming other packages, and the
requirements `--with` added, are not resolved: should uv keep the version
it has because of one of them, that upgrade ends as needing attention,
with the version unchanged. Planning an upgrade repeats the
inventory and receipt check and refuses a constraint or unknown receipt.
It neither edits a receipt nor removes a version pin. uv has no
tool-search command Banager uses.

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
two names for one program count once -- Homebrew's `python3` and
`python3.13` are asked once -- and runs `<python> -m pip --version` (30 s)
for each launcher left. A launcher in a virtual environment is counted by
its environment as well as its program: Banager checks for `pyvenv.cfg`
beside the launcher and one folder above, without reading its contents,
and includes that environment's resolved directory in the identity, so
different venvs remain separate even when their executable links share a
base binary, and a venv stays apart from the base program it links to.
The existing answer, `pip X from <site-packages>/pip (python 3.13)`, then
supplies the environment location: Banager resolves that directory with
protected read-only path lookups and counts it once, including distinct
pyenv shim files that reach the same pip. The first launcher's original
path remains the source ID and argv. No extra command is run. The venv
directory is part of that identity too, which keeps venvs separate when
they inherit the base environment's pip.
An unavailable or unparseable answer, a location that cannot be resolved,
or an unreadable venv marker does not establish identity; those launchers
remain separate. Every pip instance is read-only by design. Only the
outdated check is given environment variables, `PIP_QUIET=0`,
`PIP_VERBOSE=0` and `PIP_RETRIES=5` (`PipAdapter::OUTDATED_ENV`), which
outrank a `pip.conf`. The first two hold pip at its normal verbosity —
not quieter, which would hide the warnings Banager reads below, and not
louder, which prints a line for every file pip skips (thousands for one
project) and every index's answer. The third is pip's own default number
of retries: with retries turned off (`retries = 0`), pip gives up on an
index it cannot reach without printing anything at that verbosity, and
every package would read as up to date (seen with pip 26.2.1 on
2026-10-05: exit 0, `[]`, nothing on stderr). Offline, five retries take
pip about 7.5 seconds a package, so a large environment runs into the
60-second limit and every package is listed as "could not check". Banager makes no network request of its own for
pip: `pip list --outdated` reaches PyPI itself.

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
are installed, its pip is listed. Both the initial shim-parent check and
the developer-tool exclusion compare paths with `protected::same_path`, so
case variants such as `/USR/BIN` and links spelled that way are guarded too.

**Read-only commands:**

| Purpose | Argv | Timeout |
|---|---|---|
| Which developer directory the `/usr/bin` shims use (`detect`, before running an interpreter there; at most once a refresh) | `/usr/bin/xcode-select -p` | 10 s |
| Version | `<python> -m pip --version` | 30 s |
| List packages (`inventory`) | `<python> -m pip list --format=json` | 60 s |
| List packages nothing else depends on (`inventory`, to tell dependencies apart) | `<python> -m pip list --format=json --not-required` | 60 s |
| List outdated packages (`check_updates`) | `<python> -m pip list --outdated --format=json` with `PIP_QUIET=0`, `PIP_VERBOSE=0` and `PIP_RETRIES=5` | 60 s |

If `pip list --outdated` exits non-zero, `<python> -m pip list
--format=json` is run once more so every installed package can be listed
as "could not check", with the reason — one more process than the table
shows, on that path only. It is run too when the command exits 0 but gave
up on reaching the index for some package: pip then leaves that package
out as if it were up to date. Banager reads two things pip prints at its
normal verbosity to tell. On stderr, the warning urllib3 prints after the
fifth failure in a row, before one final try — `Retrying
(Retry(total=0, …)) after connection broken by '…': /simple/<project>/`
(`lookups_given_up` in `adapters/pip.rs`). On stdout, ahead of the JSON
list, pip's `Could not fetch URL <address>: <reason> - skipping` line,
which it prints at that verbosity only for an index whose certificate
could not be verified (`final_fetch_failures`); the reason saved for such
a line is a fixed sentence that never includes the index URL or its
credentials. The JSON list is read past those lines. Each such package
that is not listed is "could not check", with the error from that
warning or line; where the address names no installed package (a
`--find-links` page, say), every package not listed is. What was listed
is kept — also when that second list fails, and then the packages pip
gave up on cannot be named and read as up to date, as before. Nothing is
printed when that final try answers, so a package
whose sixth try worked and that is up to date is still shown as not
checked — counted, where the words name the network, among the tools to
check again, until the next check that reaches the index.

An index's answer that it does not have a project — HTTP 404 (PyPI's),
410, or 403 (what PyTorch's `download.pytorch.org` answers, checked
2026-10-05) — is never a failed lookup, even if pip prints it: with an
extra index (`extra-index-url`), pip asks every index about every
package, and the one that does not host a package answers that way
while PyPI answers with it.

What is still not seen: an index that answers a lookup with an error
status other than those three — a server error, say (pip retries 500,
502, 503, 520 and 527 without printing anything, then gives up). pip
reports those only at `-vv`, as `Could not fetch URL` lines at its
debug level; at its normal verbosity it prints nothing, leaves the
package out of the JSON list, and exits 0, so that package reads as up
to date. Banager cannot tell it from a package that is up to date
without pip's debug output, which it does not request: `-vv` makes the
output grow by thousands of lines a project, and `--log` would write a
file. A package whose index could not be reached at all — refused,
timed out, a certificate or proxy failure — is always shown as not
checked.

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
The managed install root remains that Cargo home; discovering additional
roots selected by `CARGO_INSTALL_ROOT` or `install.root` is not supported.
Every write explicitly passes `--root` with the instance's inventory
root, so inherited Cargo root settings cannot redirect an operation to
another installation. Inventory, reconciliation and the instance lock
all refer to the same root.

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
rather than list it.

It also reads `<CARGO_HOME>/.crates.toml`, Cargo's older install
manifest, the same way (no command; a regular file of at most 16 MiB,
never in or through a protected place; a missing file means
`.crates2.json` is read as it is, anything else is an error for the
source). Cargo rewrites both files on every `cargo install` and `cargo
uninstall`, but cargo-binstall's binary installs rewrite only
`.crates.toml` (and a `binstall/crates-v1.json` of its own, which Banager
does not read), so after cargo-binstall upgrades a crate,
`.crates2.json` still names the old version. Where the two disagree,
`.crates.toml` therefore records the newer install, and Banager applies
the rule Cargo itself applies whenever it loads them (`sync_v1`;
`merge_crates_v1` in `adapters/cargo.rs`): each `.crates.toml` entry is
one installed crate, at that entry's version, with that entry's
programs; a `.crates2.json` record whose entry `.crates.toml` no longer
has is an install that was replaced or removed, and is left out — every
one of them when `.crates.toml`'s `[v1]` table lists nothing, as Cargo
leaves it once the last crate is uninstalled. Build choices (below) come
from `.crates2.json` only — `.crates.toml` records none — and only from
the record of the same install: the same name, version and source. A
version only `.crates.toml` names, such as the one cargo-binstall put in
place of an older install, has none, as Cargo gives it none: the older
install's features or profile are never carried over to it. A blank
`.crates.toml` (Cargo empties it just before writing the new listing,
and cargo-binstall creates it blank before its first write) beside a
`.crates2.json` that lists anything is an error for that refresh, which
the next refresh reads again; one without a `[v1]` table is an error, as
it is to Cargo. A crate that only cargo-binstall installed is listed
too, as `cargo install --list` lists it. `inventory`, the check after an
operation, `check_updates` and upgrade planning all read this merged
record.

`check_updates` reads the
merged record and, for each crate installed from crates.io, asks crates.io
once: `GET https://crates.io/api/v1/crates/{name}` (30 s), the name
percent-encoded, one crate at a time and within the registry phase's
120-second budget (see "Network: Banager only connects to these hosts").
Crates installed from a git repository or a local path
or another registry are never looked up; they are listed as "could not
check" with that reason. The complete source identity is retained and
checked again when planning an upgrade; an unsupported source is refused.
Only a stable version with strictly greater SemVer precedence is offered;
build metadata alone is not an update, and an older stable release never
replaces a newer prerelease. Versions that cannot be compared are "could
not check". Cargo has no search command Banager uses.

Upgrade planning also reads the saved `features`, `all_features`,
`no_default_features`, `profile`, `target` and `rustc`
(`BuildChoices` in `adapters/cargo.rs`). Cargo writes a profile and a
target into every record, also for a plain `cargo install`, so only a
choice that differs from what cargo picks by itself counts: any feature,
`all_features`, `no_default_features`, and a profile other than
`release` (`--debug` is saved as `dev`). Those choices are replayed as
Cargo flags and force a source build with the existing "compiles
locally" warning, even when cargo-binstall is available. A crate
installed with cargo's defaults has no build choice, and its upgrade
uses cargo-binstall when it is found.

The saved target is never replayed as `--target`. Cargo saves the target
it resolved, not whether `--target` or a `build.target` setting chose
it, and replaying it can ask for a standard library this Mac's Rust does
not have ("can't find crate for `std`") or build a program this Mac
cannot run. A target equal to the `host:` line of the saved `rustc -vV`
is cargo's own default on the machine that built it — a `~/.cargo` that
Migration Assistant brought from an Intel Mac says
`x86_64-apple-darwin` for both — and such a crate is upgraded like any
other, for this Mac. A target that differs from that host (an explicit
cross-build, a `build.target` setting) forces a source build without
`--target`: `cargo install` then applies the user's own `build.target`
if there is one and otherwise builds for this Mac; cargo-binstall is not
used for it. A record that names no compiler host is treated as cargo's
default. An ambiguous or malformed install record is refused.

**Write commands:**

| Purpose | Argv | Timeout | Needs a password |
|---|---|---|---|
| Install, a usable cargo-binstall found (below) | `<cargo-binstall> -y --root {root} --index sparse+https://index.crates.io/ {name}` | 1800 s | No |
| Install, otherwise | `<cargo> install --root {root} --index https://github.com/rust-lang/crates.io-index {name}` (previewed with a "compiles locally" warning) | 1800 s | No |
| Upgrade, a usable cargo-binstall found, no saved build choices and no other target | `<cargo-binstall> -y --force --root {root} --index sparse+https://index.crates.io/ {name}` | 1800 s | No |
| Upgrade, otherwise | `<cargo> install --force --root {root} --index https://github.com/rust-lang/crates.io-index [saved build flags] {name}` (same warning) | 1800 s | No |
| Uninstall | `<cargo> uninstall --root {root} {name}` | 300 s | No |

The explicit index binds installation to the crates.io identity checked
above even when `registry.default` selects another registry. Each program
is given crates.io's index in the form that keeps it on the index host it
reaches with no flag:

- `cargo` is given `https://github.com/rust-lang/crates.io-index`,
  crates.io's own name for its index. Cargo takes that address for its
  built-in crates-io source, so it reads the index where it reads it
  without the flag: `index.crates.io` (cargo's default sparse protocol),
  or the mirror a `[source.crates-io]` replacement names. It contacts
  github.com for the index only when the user's own Cargo settings choose
  the git protocol for crates.io (`registries.crates-io.protocol`).
- `cargo-binstall` is given `sparse+https://index.crates.io/`, the index
  it uses by default since version 1.3.0, so it reads `index.crates.io`
  as it does without the flag. It is never given the github.com address:
  binstall reads any index address without `sparse+` as a git index and
  would download the whole crates.io index from github.com on every run.

Which cargo-binstall is usable: cargo-binstall 1.1 and 1.2 asked
crates.io's API (`crates.io`) by default, and versions before 1.1 do not
accept `--index` at all, so a binstall older than 1.3.0 could not be
bound to crates.io's index without moving it to a host it did not use.
Banager learns a binstall's version without running it only from the
Cargo root's own records: when the binstall on `PATH` is
`<CARGO_HOME>/bin/cargo-binstall` and the merged record above holds
exactly one crates.io entry for the crate `cargo-binstall` — which its
install script (`--self-install`), `cargo install cargo-binstall` and
its own self-update all leave. A binstall those records date before 1.3.0 is not used: the
install or upgrade compiles with `cargo install` instead, previewed with
the "compiles locally" warning. A binstall installed elsewhere —
Homebrew's, or a copied file — has no version Banager can read and is
used with the sparse index; were it older than 1.3.0, it would read
`index.crates.io` instead of crates.io's API, both crates.io's own
hosts; 1.3.0 was released in 2023, and Homebrew's formula is the current
release.

Where each program then downloads the crate or the prebuilt binary from
is its own choice, unchanged by these flags (the last paragraph of
"Network: Banager only connects to these hosts", below).

`--force` here is cargo's own flag, meaning "reinstall even though a
version of this crate is already installed" — it is how cargo upgrades a
binary. The only other `--force` Banager passes to any tool is Homebrew's:
to the uninstall of a formula with more than one version installed and no
pin, which deletes every version (Homebrew's section, "Old versions"), and
to `brew link --formula --force`, after the update of a keg-only formula
whose link Homebrew recorded and on its own for the one a source's notice
offers, which lets it link a keg-only formula and stops rather than
overwrite another program's file (Homebrew's section, "Keg-only formulae
linked into Terminal").

## Ollama

Adapter: `OllamaAdapter` in `crates/banager-core/src/adapters/ollama/mod.rs`.
Verified against Ollama 0.34.1 (`adapters/meta/ollama.toml`).

**Detect.** `ollama` is the first `ollama` on `PATH`; `<ollama> --version`
(30 s) — never `ollama list`, which on macOS launches Ollama.app as a side
effect, and a background refresh must never launch an application. The
daemon is asked over HTTP instead: `GET {host}/api/tags` (10 s), where
`{host}` is `OLLAMA_HOST` from the environment, normalised to an absolute
http(s) URL (a bare `host:port` gets `http://` in front; a bare host without
a port gets port 11434, including `user:password@host`: the colon in
userinfo is not a port. Explicit http/https schemes retain their
80/443 defaults; a value that
does not make an http(s) URL is ignored and the default used), or
Ollama's default `http://127.0.0.1:11434` (`DEFAULT_HOST`). Banager also
checks whether `/Applications/Ollama.app` or `~/Applications/Ollama.app`
is a directory: a daemon on this Mac that does not answer while the app
is there is reported as not running, with an Open Ollama button; anything
else that does not answer is reported as not responding, with no button.
Every pull and rm plan explicitly sets `OLLAMA_HOST` to that normalized
instance endpoint, matching inventory and reconciliation. URL userinfo
is retained for requests and commands, including HTTP Basic authentication
to the daemon; plain HTTP does not encrypt it (Network). The authenticated
endpoint stays privately in the adapter, while detected instance ids omit
userinfo before snapshots, model keys, plans or operation events reach the
window. Plan serialization and operation `env_preview` put `****` in
place of the user name and of the password, whichever the login has
(`runner::redact`); an `OLLAMA_HOST` with no login is shown as it is. The
command preview and command text copied from the window keep those masks. Such copied text requires the
person to supply their own login. The backend retains the real plan and
passes the original normalized value to the command; the window submits
only the held plan id. This does not add CLI support for URL userinfo:
Ollama itself may still reject that value.

Inventory errors mask the requested URL and any login echoed in transport
errors, non-200 response bodies or parse errors, using the same redactor.
Command output masking also includes the inherited or explicitly supplied
`OLLAMA_HOST`, including decoded, encoded and HTTP Basic forms under the
redactor's existing short-secret rules (How Banager runs anything).
History and settings still remove logins from legacy keys before storing
instance ids (Files Banager writes). The version command has no added
environment variables.

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

For a remote daemon, every model is "could not check": this Mac's local
manifests do not establish the selected daemon's model state. No local
manifest is read or registry request made for that comparison. For a
daemon identified as on this Mac, `check_updates` first reads
`~/.ollama/models/manifests-v2/ollama.com/{namespace}/{name}/{tag}`.
Ollama 0.40 uses `ollama.com` as the official registry's on-disk name;
this is a local path, not another network host Banager contacts. This entry
normally links to `~/.ollama/models/blobs/sha256-<digest>`; Ollama can also
store a regular copy there. Only if this model's entry or link target is
missing does Banager fall back to
`~/.ollama/models/manifests/registry.ollama.ai/{namespace}/{name}/{tag}`.
The existence of the v2 directory alone does not prevent that per-model
fallback. Other read errors, a protected target, invalid JSON, oversized
files or a live-digest mismatch do not cause a legacy fallback.
Both paths and any link target use the existing protected-place reader
(opened without waiting, and read only when `fstat` says it is a regular
file of at most 16 MiB; otherwise the model is "could not check"). No model
file, link or directory is created or changed by this check.
Ollama 0.40 leaves a downgrade anchor at the old path with extra manifest-blob
layers. Such a stand-in is rejected, even if it is the only file left and
its digest matches an older daemon's answer; it is never compared as a real
model manifest. A manifest list -- what 0.40 stores for a tag the registry
serves with one model per runner (`mediaType`
`application/vnd.ollama.manifest.list.v2+json`, no layers of its own;
its `/api/tags` row carries the selected child's digest) -- is not
compared either: "could not check", with no registry request
(`Warning::NotLookedUpHere`, below). For a real manifest, Banager first
checks that the SHA-256 of its original bytes matches the live
`/api/tags` manifest digest. A mismatch, including a leftover default-store
copy when the local daemon uses another model folder or port, is "could
not check" with no registry request or download estimate. Only a matching
manifest is compared by its layer digests with the registry's. The file is looked up one
step at a time and never in or through a place Banager never looks into
(How Banager runs anything, above): models kept on another disk through a
link -- `~/.ollama/models`, or `~/.ollama`, linked to `/Volumes/<disk>/…`,
as Macs with a small disk often have them -- or in `~/Documents` or iCloud
Drive are not read there, and each such model is listed as "could not
check" (the row's chip; with technical details shown, its reason says
the manifest is in a place Banager never looks into). Their disk use is
not measured either (Disk use, below). The three name parts
come out of the daemon's `/api/tags` answer, so before any path is built
each must be a plain path segment (`contained_manifest_path`: nothing
absolute, no `..`), and in the URL each is percent-encoded. The registry
manifest is always fetched from `registry.ollama.ai`. A model whose name
begins with another registry -- `hf.co/…`, the one mirror Ollama
documents, or any other host -- is not looked up at all: Ollama keeps its
manifest under its own host in `manifests-v2/` or `manifests/`, not where
Banager reads, so no file is read and no request made for it. Nor is a request made for a model
whose local manifest is not there (the models kept elsewhere through
`OLLAMA_MODELS`, which Banager's environment does not carry) or is in a
place Banager never looks into. Each such model is listed as "could not
check", with the line that it isn't checked for updates on this Mac
(`Warning::NotLookedUpHere`), and keeps no Overview from saying all is up
to date. Ollama has no search command Banager uses.
From the same two manifests, and nothing else, a model with an update
also gets the most its pull can download: the sum of the `size`s the
registry manifest gives its layers and config whose digests the local
manifest does not name (`changed_blob_bytes`, the candidate's
`download_bytes`), each entry counted — a digest listed twice counts
twice, as Ollama's pull for a model with tensor layers can download it
twice. No other request is made and no other file is read for this
number: it does not look at the layer files in `~/.ollama/models/blobs`
(the one file read there is the manifest itself, through its v2 entry,
above; only the size measurement, under "Disk use" below, looks through
that folder, for the models' total). So a file another model shares,
which `ollama pull` skips, still counts, and the number is an upper bound, which the window words as one
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
whether LaunchServices accepted the request. It and the homepage link in
the Installed page's details, which can start the default browser
(Network), are the only launches in the app that are not package-manager
commands, and neither happens during a refresh.

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
an error for the source, and so is a `~/.claude/settings.json` in or
through a protected place, which is not read (no pointer is asked: which
channel it names is not known), and so is an installed version that cannot be
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
whenever a version before it could be read and it is below the version
the confirmed update aimed for, as for every source; at or past that
version -- it updated itself to it before Banager's turn came -- the
update is reported as done, already up to date. When
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
refuses the uninstall. Each of these looks is taken one step at a time
and never into or through a place Banager never looks into
(`protected::look`; How Banager runs anything, above). A listed path
that is, or is reached through, one is not what the list describes, so
nothing the uninstall moves is ever inside one. A kept path that leads
into one -- a `~/.claude` or `~/.claude.json` that Mackup or a dotfiles
folder keeps in iCloud Drive, Dropbox (`~/Library/CloudStorage`) or
`~/Documents` -- is followed only as far as the place's edge
(`removal::kept_places`): the links and folders on its way there are
checked as any kept path's are, and what it leads to inside the place is
never looked at; the uninstall goes ahead, and the preview lists it among
what stays. What this leaves unchecked, precisely: a link inside the
place that leads back out of it. If the file Mackup keeps were itself a
link to a file inside a folder the uninstall moves -- `~/.local/share/claude`,
say -- the move would take that file to the Trash with its folder, and
the kept path would lead nowhere afterwards until the folder is put back
from the Trash. A link that stays inside the place, or leads anywhere
the uninstall does not move, is unaffected. If a
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
file, and does not read these to find the line). A kept path that leads
into one of the places Banager never looks into -- a `~/.zshrc` that
Mackup keeps in iCloud Drive -- is followed only as far as the place's
edge, and what that leaves unchecked is what Claude Code's section says. The whole uninstall has
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
unchanged needs attention unless it is already at or beyond the confirmed
target; then it succeeds as already updated, as for every source (How
Banager runs anything). **How
`grok update` behaves when nothing can answer a prompt** (Banager gives it
no terminal and a closed stdin) was recorded on 2026-10-07 on a GitHub
Actions `macos-latest` runner (image 20260907), in a throwaway HOME, by
`.github/workflows/standalone-upgrade-probe.yml` (pull request #1, run
37557838541): from 1.0.34 to 1.0.46 it asked nothing, exited 0 after about
a second and printed to stderr only its progress and "Please restart
Grok."; afterwards `grok --version` read 1.0.46, both links in
`~/.grok/bin` pointed at the new `~/.grok/downloads/grok-1.0.46-macos-aarch64`,
the previous program stayed in `~/.grok/downloads` (about 143 MB), and
`~/.grok/config.toml` was unchanged. So the update runs unattended, and the
recipe above stays as it is.

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
marked block; a kept path that leads into one of the places Banager never
looks into is followed only as far as the place's edge, as for Claude
Code's. A `/usr/local/bin/grok` or `/usr/local/bin/agent` is outside
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
Detection atomically acquires its resource locks against the operation
queue before scheduling the adapter, and holds them until detection
finishes or is aborted. Cargo and rustup reserve their known lock names
even before the first snapshot exists. An operation submitted after
detection starts waits for those same locks; a pending operation also
prevents a later detection from taking its resources.

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
move, the operation needs attention unless it is already at or beyond the
confirmed target; then it succeeds as already updated, as for every source
(How Banager runs anything). A run stopped by the timeout is reported as unconfirmed, whatever the
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
started with: at launch Banager restores `PATH` and the proxy and mirror
settings (How Banager runs anything) from your login shell, nothing that
says where Rust is, and every command it runs inherits the rest. Banager reads `CARGO_HOME`,
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
Cargo's env file, under every one of those names that leads to it. A
name that is, or leads into, one of the places Banager never looks into
(a `~/.zshrc` that Mackup keeps in iCloud Drive) is not read; rustup, which
Banager runs, still reads and may edit it, so instead of nothing the
preview names it as a file it could not read, whose line about Cargo is
not known (`Warning::ShellConfigUnread`). The
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
`release_link.rs` the version read, `route.rs` the recognition,
`removal.rs` the uninstall). **No command runs for it, ever** — not `codex
--version`, not an update check, not an update, not an uninstall: its
uninstall moves files to the Trash itself, as Claude Code's does (below).
The row is Codex installed
by OpenAI's own script (`curl -fsSL https://chatgpt.com/codex/install.sh |
sh`, run by the user — Banager never runs it); what the recipe expects
was read from that script as text on 2026-10-01 (the fixture README,
`adapters/fixtures/standalone-codex/install-script-2026-10-01/README.md`,
names the lines). npm's `@openai/codex` and the Homebrew cask `codex` are
other paths and stay npm's and Homebrew's rows; the AI Tools filter puts
all three in one family.

**Detect.** Banager looks at the fixed path the installer writes,
`~/.local/bin/codex` — never a `codex` found through `PATH` — and checks
with `lstat`, `readlink` and `realpath`, each one step at a time and never
into or through a place Banager never looks into (`protected::look`), that
it is a symbolic link whose text and final target are both inside
`~/.codex/packages/standalone` (the script links it to
`…/packages/standalone/current/bin/codex`). A launcher, or a folder above
it, that leads into one of those places -- `~/.local` or `~/.codex` kept
in iCloud Drive or `~/Documents` -- is not followed there, and the row is
not listed, as for a launcher Banager cannot look at. A link into
`node_modules` or Homebrew's `Caskroom` is not this row. Only the default
folders are looked at: `CODEX_HOME` and `CODEX_INSTALL_DIR` are not read,
because a Mac app started from the Finder inherits no variable from your
shell except the `PATH` and the proxy and mirror settings Banager asks your
login shell for (How Banager runs anything), so a
`CODEX_HOME` set in `~/.zshrc` is invisible to it. A Codex installed under
another `CODEX_HOME` is not listed here (the Other Programs page lists its
launcher instead). Nothing else in `~/.codex`, with your settings, login
and sessions, is looked at to list this row or read its version, and no
file of yours there is ever opened. Only the preview of uninstalling a
Codex -- npm's `@openai/codex`, Homebrew's `codex` -- walks `~/.codex` to
say how much it takes, by names and sizes alone (Data an uninstall leaves
behind, below); the preview of uninstalling this one names `~/.codex` as
what stays without walking it.

**Version, with no command.** `readlink` and `realpath` of
`~/.codex/packages/standalone/current`, which the installer points at
`~/.codex/packages/standalone/releases/<version>-<target>`: the version is
that folder's name less `-aarch64-apple-darwin` or `-x86_64-apple-darwin`,
when that folder is directly in `releases/` (the same folder, by device
and inode, as `releases` leads to). Each look is taken one step at a time
and never into or through a place Banager never looks into
(`release_link::read`, `protected::look`). A missing, dangling or
unexpected link gives no version (the row is listed with its version
unknown, and is not marked as not responding, since nothing was asked).
A `~/.codex`, or its `releases/`, kept in iCloud Drive or `~/Documents`
is not looked into at all: the launcher leads there through `current`,
so the row is not listed (Detect, above), and no version is read. Then
one small file,
`~/.codex/packages/standalone/auto-update-version` (at most 256 bytes,
only when it is a regular file; opened from the folder it is in, held
open, without waiting and without following a link, then checked with
`fstat`; never in a protected place): the installer writes the release's name
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
installer's `curl | sh`).

| Purpose | Argv | Timeout | Needs a password |
|---|---|---|---|
| Uninstall | none: Banager moves up to three paths to the Trash itself (below) | 120 s; Banager stops between items once it is spent | No |

**Uninstall** (the author's decision U8, 2026-10-06; only after the user
reviews and confirms a preview; no command runs): Banager moves to the
Trash, in this order, `~/.local/bin/codex-code-mode-host` when the
installer made it (it does so only on a Mac and only for a release that
has the helper; it goes first, while the folder it leads into is still
there), `~/.codex/packages/standalone` (the program: every release the
installer unpacked, the `current` link and the `auto-update-version`
file), and last `~/.local/bin/codex`, the command itself. The folder
`~/.local/bin` is shared with other tools (Claude Code's launcher is there
too) and is never moved. Each path passes the checks Claude Code's section
describes. The helper link passes one more: its own text must point into
`~/.codex/packages/standalone`, and, if it still leads somewhere, lead
there too — a file or a link of yours by that name stays where it is, and
the preview says Banager could not confirm it is Codex's. The launcher is
last: once the package folder is in the Trash, a run that stops leaves a
launcher-only row that a second Uninstall finishes, as for Claude Code.
The whole uninstall has 120 s, as Claude Code's does. It keeps the rest of
`~/.codex` — `config.toml`, `auth.json` (the login), `sessions/`, the
history — and `~/.zprofile`, where the installer adds its marked block
(`# >>> Codex installer >>>`) for zsh when `~/.local/bin` is not on your
`PATH`; Banager edits no shell file, and the block stays. For bash the
installer writes to `~/.bash_profile` instead, which the preview does not
name. There is no `codex uninstall` and no vendor uninstall document; the
list is Banager's own reading of the install script (read as text on
2026-10-01 and again on 2026-10-06, the same file both times), and the
fixture README names the lines. Put Back from the Trash after this
uninstall has not yet been tried by hand in Finder; the author does so
once before this merges, as for Claude Code's (step C).

## opencode

Adapter: `StandaloneAdapter` over the `OPENCODE` recipe in
`crates/banager-core/src/adapters/standalone/` (`recipes.rs` is the data,
`route.rs` the recognition). Listed only: **no command runs for
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
says so on the page, with the number it stopped at. Directory names are read
one at a time under the deadline; at most one extra name, never statted,
distinguishes an exactly full entry budget from an unfinished scan. Only
retained results are sorted, so an oversized directory is never collected
in full. The deadline is checked between filesystem calls; it cannot
interrupt a single call that has stopped answering. It never runs, opens,
moves or deletes anything it finds. It takes no lock and is not part of a
refresh (`Session::scan_unknown` in
`crates/banager-core/src/session/scan.rs`): it runs when the page opens
(from the sidebar, or Other Programs, ⌘4, in the menu bar's View menu),
again when the sources' state changes while the page is open, and when
you press *Scan Again* — always against the sources' last known state —
and its result is not stored. One scan runs at a time: a request that
comes while a scan against the same state of the sources is under way is
handed that scan's result rather than starting another, and one that
comes after the sources' state changed waits for it and then scans
(`SharedRun` in `src-tauri/src/shared_run.rs`, keyed by the snapshot's
`generation`).

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
read. What a path names can change after the scan -- the program, or a
folder on its way, replaced by a link into `~/Documents` -- so the
command then looks the path up again as the scan did, one step at a time
and never into or through a protected place (`protected::resolve`:
`lstat` and `readlink` of each step, from the folder before it held
open), and goes on only when it still leads, with no link anywhere on its
way, to the very file the scan found there (its device and inode, which
the scan kept in memory, `UnknownEntry.seen`, never sent to the window;
`still_found`). Otherwise it refuses (`changed_since_scan`), and the page
says it changed after the last scan and to scan again (「它在上次扫描后有变动。请重新扫描。」,
"It changed after the last scan. Scan again."): a new scan finds what is
there now. It then makes one call,
`NSWorkspace activateFileViewerSelectingURLs:`, through AppKit directly
(`show_in_finder`), with a file URL of that path as it is
(`NSURL fileURLWithPath:isDirectory:`, told it is a file), with which
Finder opens a window on the program's folder with the program selected:
nothing resolves the path again first. So for a link Finder shows the
file the link points to; a broken link's is gone, and on its row the item
is off, as it is on the row of a link into a protected place, which the
scan did not follow. The Tauri opener plugin, whose `reveal_item_in_dir`
the command used to call, followed every link of the path again
(`std::fs::canonicalize`) before asking Finder, into any place; it is no
longer built into Banager (Network, below). Copy Path puts the path the row
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
lock is taken. Reading the folders stops after 100,000 entries or 5
seconds, and working out the answer after 5 seconds more
(`CommandBudget::default`). Reading a folder reads only the names in it,
and 12,000 of them take about 5 milliseconds, so the entry limit is far above what a Homebrew
with thousands of formulae links into its `bin` (ten to fifteen
thousand); past either limit that refresh says nothing about which
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
`useCopyCommand`), `~` and all, and so can the line that puts it on the
search path, `export PATH="$HOME/<folder>:$PATH"`, which the row under it
says to add to a shell startup file such as `~/.zshrc` (*Copy Line*,
`PathLineRow` in `src/components/PathLine.tsx`; the author's decision
U15 a). Banager only shows and copies that line: nothing edits a shell
file. Since the login shell's `PATH` is read once per run (above: no
read runs again after one that worked), the row also says to quit and
reopen the app to see the change there; no new read is run for it.

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
| a Cargo crate | each program `<CARGO_HOME>/.crates2.json` says it installed, in `<CARGO_HOME>/bin` (that file is read again for this, only when it is a regular file of at most 16 MiB, as the Cargo source reads it: a larger one is refused by its size, not read, and the crate is measured as its own listing names it) |
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
at most 300,000 entries, with a deadline of 30 seconds (`SizeBudget::default`)
checked between filesystem calls, starting before planning and including
old-version directory enumeration. A call already waiting on the disk
cannot be interrupted by this budget.
Planning checks for a newer round before each directory entry and stat;
exhaustion leaves unplanned sizes unknown and totals marked "or more".
Every entry a listing names counts, also one that then cannot be looked
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

Even when a kept-data link leads to the home folder or its Library, a
protected child is skipped before its stat and the measurement is partial.
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
of (`crates/banager-core/src/protected.rs`). `~` there is both the home
folder `HOME` names and the account's own home folder from the password
database, whatever `HOME` says (How Banager runs anything, above;
`Protected::new`).

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
nothing from your shell but the `PATH` and the proxy and mirror settings
Banager asks your login shell for (How Banager runs anything) -- so a
folder moved that way is not named.

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
  own (the folder pipx's `app_paths` -- or, for a package whose only apps
  are its dependencies', `app_paths_of_dependencies` -- and uv's
  `--show-paths` name), every link followed;
- only when one of those could not be followed (below), and the package
  is not named for what it would have to end at nor a cask known to hold
  it, whether the package has a program of that name: for a formula
  `<prefix>/opt/<name>/bin/<program>`, for a cask `<prefix>/bin/<program>`,
  every link followed -- one look a name; and then, by the names in them
  (`protected::look::list`, one look a folder and one before each name
  read in it), for a Python a formula's
  `<prefix>/opt/<name>/bin`, and a cask's app's `Contents/MacOS` and
  `Contents/Resources` (for a program) and the `bin` of each folder in its
  `<prefix>/Caskroom/<token>` but the hidden `.metadata`.

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

A preview that found nothing running on the package found nothing among
the tools the snapshot listed then, so it keeps what the look read of the
snapshot (`needed_by::Inputs` in
`crates/banager-core/src/session/needed_by.rs`): the package's key and a
cask's app, its Homebrew's id and prefix, and for each source with a tool
that counts, its program and those tools, with a pipx or uv tool's
environment. When a refresh has committed since, `Session::submit`
compares the latest snapshot with that, in memory under the snapshot's
lock (`needed_by::adds_no_dependent`): a tool the look never saw -- a
source's first tool that counts (an npm package installed in Terminal
while the confirmation was open, say), a pip package that came to count,
a tool in another environment, or a source run by another program -- spends
the preview (`unknown`: 「此确认已失效。请关闭后重新开始。」, "This
confirmation is no longer valid. Close it and start again."), and
nothing runs; the new preview looks again and names that source. Fewer
tools cannot add one, so an uninstall or update in another source, a
source that stopped answering with its rows carried, another Homebrew
package, a note on Homebrew's catalogue or when a source last answered
leaves the preview the one to confirm. This reads no disk and runs no
command at submission; a change that no refresh has committed, or one
after submission, is not seen by it.

How: read-only, as the command check is (Which copy a command runs,
above): each path is followed one step at a time (`protected::resolve`:
`lstat` and `readlink`, each asked of the folder before it, held open),
and never into the places macOS asks about first nor onto another disk --
the same places the command check never reads. Only folders are opened,
to follow each link and to read the names in the few listed above: no
file's contents are read, nothing is written, and no command runs. At most 2,000 paths and 1 second for one
preview (`needed_by::BUDGET`), including one look before each directory-name
read. Names are consumed one at a time, with no directory-sized collection;
the search stops at the first match, and exhaustion stays unknown. The
preview waits one second more at
most for a step that does not answer at all (a folder on a disk that
stopped answering), then goes on without it; a look that did not finish
says so (「无法确定还有哪些软件要用它。卸载前请自行确认。」, "Couldn't check what
else needs this. Check yourself before you uninstall.",
`Warning::DependentsUnknown`), never that nothing runs on it. Nor did a
look that met a path it may not or cannot follow (`needed_by::Doubt`) --
a source's program, a pipx or uv tool's environment (a venv kept in
`~/Documents`, say, whose `bin/python` may be a Homebrew Python's) or a
`PATH` folder passed over on the way to `node` that is, or leads into,
one of those places; one on the way to which a folder could not be
searched; and a pipx or uv tool with no environment Banager knows of --
when the package could be what that path leads to (`Look::could_be`).
A pip launcher in a folder named `shims` -- as found on `PATH` or where its
links lead, any case -- is a version manager's (pyenv's `~/.pyenv/shims` or
`$PYENV_ROOT/shims`, asdf's, mise's), and is also an unresolved Python
dependency (`Doubt::Python`) unless it leads into the package's own folders:
a readable pyenv script can dispatch to Homebrew's Python through `global
system`. It is told by the folder's name, not by `PYENV_ROOT`, which an app
opened from the Dock does not have; nothing new is read. No command is added
to resolve that runtime choice, and no dependency is invented. The same
candidate-runtime filter below decides whether to show the existing
incomplete-check warning. A launcher that leads into the package -- mise's
shims are links to mise itself -- still names it as needed.

What the path would have to lead to is judged by what it is for, not
only by its name, for a link can change both a program's name and where
in a keg it is: `~/bin/python3` may lead to python@3.13's
`bin/python3.13`, npm's `npm` to `lib/node_modules/npm/bin/npm-cli.js`.
So pip's program is a Python whatever it is called, and so is a tool's
environment; npm's is an `npm` in a Node.js, with its `node`; the `PATH`
folder may hold a `node`; any other source's program is a program of its
own name. The package could be that when it is named for it -- `uv` for
uv's program, `node` or `node@22` for a `node`, any `python@3.N` for a
Python -- or is a cask known to keep it with no link in `<prefix>/bin`
(`PYTHON_CASKS`: `anaconda`, `mambaforge`, `miniconda`, `miniforge`, whose
installer puts a Python in `Caskroom/<token>/base`; `PROGRAM_CASKS`:
`ollama` and `ollama-app` for Ollama's `ollama`, in the app's
`Contents/Resources`), or when it has, of its own, a program of that
name: in its `opt/<name>/bin` for a formula, linked or keg-only, or in
`<prefix>/bin`, where its `binary` links go, for a cask (for a Python,
`python3`, or `python3.N` for `python@3.N`, whose keg has no `python3`
unless it is Homebrew's default Python). For a Python it is also any
interpreter in a formula's `opt/<name>/bin` -- a name that is `python`,
`pypy` or `graalpy` and a version, so PyPy's `pypy3.11` and the
free-threaded `python3.14t` -- or in the `bin` of a folder in a cask's
`Caskroom/<token>` (Miniconda's `base/bin`); for a program, one of its
name in a cask's app's `Contents/MacOS` or `Contents/Resources`, or in
such a `bin`. An app's own Python (LibreOffice's) is not counted: a
tool's environment is not made with it. A folder that cannot be listed
(protected, not searchable) counts as holding it. What is there may run
on such a package, so its preview says it could not check, and still
names whatever it did find running on it; uncertainty alone never
invents a dependency or blocks uninstall. A package none of these finds
-- jq, a font, LibreOffice -- is taken not to be what the path leads to,
and its preview says nothing of it; a runtime kept under another name,
with no link to it in these places, would not be found this way. A
cask's own app that cannot be followed leaves that cask's look
unfinished, whatever met it. A path that is not there at all is known
not to run on any package.

## Diagnostic info: read-only, no command runs

Settings' Diagnostics group (「诊断」) has Copy Diagnostic Info (「拷贝诊断信息」); the Help menu's
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
can hold a password) -- not even those of the proxy and mirror settings
read from the login shell -- anything from a shell file, or a token.

Check Tool Setup (「检查工具环境」), in the Help menu, on the Overview and beside Copy
Diagnostic Info in Settings' Diagnostics group, opens a sheet that says the same facts
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

Commands whose ownership paths could not be resolved safely carry
`ArtifactFacts.commands_unavailable`, even when all their command facts
were dropped. A link in Homebrew's `bin`/`sbin` or npm's `bin` that leads
into a protected place, or that could not be followed, marks only the
formula or package whose folder (`Cellar/<name>`, `node_modules/<name>`)
its own first step goes into: its text is read where it is (`readlink`,
the step following it starts with), and the folders on the way to where
that step goes are followed as far as they lead outside any protected
place — a `lib/node_modules` or `Cellar` that is itself a link, to
compare it with the folder the formula or package is in — and nothing in
a protected place is looked at.
One whose first step goes anywhere else — a user's own script linked into
Documents from `/usr/local/bin`, Homebrew's prefix on an Intel Mac — is
no formula's or package's and marks none; one whose text cannot be read,
or a `bin` folder that could not be read, marks every one of that
prefix's. Check Tool Setup and copied diagnostics count those tools
as uncheckable, alongside commands with no verdict. When only some tools
were checked, the positive Terminal sentence refers only to those tools,
and so does the sentence that no tool is installed more than once when
some tools' commands could not be listed.

## Files Banager reads

All read-only, none saved anywhere else, none uploaded, and each path
looked up one step at a time, never in or through a place Banager never
looks into -- a path that leads into one is answered as one Banager may
not read (`protected::look`; How Banager runs anything, above):

- Homebrew: whether the three candidate `brew` paths exist;
  `<prefix>/var/homebrew/locks` and the `update` lock file in it, during
  the uninstall preview; its `brew.env` files, during every install,
  uninstall and upgrade preview, again right before an install or upgrade
  runs, and right before the cleanup that follows an update; during a
  cask's uninstall preview, the
  names in its `<prefix>/Caskroom/<token>/.metadata` folder and in the
  folders there, the caskfile Homebrew saved when it is JSON, and
  `INSTALL_RECEIPT.json`; during every uninstall preview, its trust list,
  `trust.json` in the user's Homebrew config folder; during a formula's
  upgrade and uninstall preview, the names in `<prefix>/Cellar/<name>` and
  whether `<prefix>/var/homebrew/pinned/<name>` is there (Homebrew's
  section, "Old versions"); for a keg-only formula's upgrade, during its
  preview, right before it, after it and after the `brew link` that
  follows, and for the link a source's notice offers, during its preview
  and after it, where `<prefix>/opt/<name>` leads and its text, the names
  in its keg's `bin` and `sbin`, what is at those names in `<prefix>/bin`
  and `<prefix>/sbin`, its text and where it leads, and
  `<prefix>/var/homebrew/linked/<name>`, its text and whether it leads to
  a folder, and during a check the same for each keg-only formula with an
  update (Homebrew's section, "Keg-only formulae linked into Terminal").
- A Homebrew cask's app, when the window asks for its icon: `lstat` of the
  `.app` Homebrew named for that cask, and the icon macOS finds for it
  through `NSWorkspace iconForFile:` — Banager opens no file in the app
  (App icons, above).
- npm: whether `{prefix}/lib/node_modules`, `{prefix}/lib` or `{prefix}`
  is writable, via `access(2)` (`faccessat` of the folder held open; one
  in a protected place is taken as not writable); where `{prefix}/Cellar`
  and `{prefix}/bin/npm` lead, when a check lists npm's own update or its
  update is planned, to tell a Homebrew formula's npm (npm's section).
  Planning also compares the prefix directory's device/inode with Homebrew's
  fixed discovery prefixes to share one lock across symbolic-link aliases.
- pip: the canonical path of each interpreter found, to count it once, and
  for the shim guard; whether `pyvenv.cfg` is a regular file beside the
  launcher or one folder above, and the site-packages directory named by
  the existing version output, to count environments once without merging
  separate venvs;
  for one in `/usr/bin`, where `usr/bin/<its name>` in the developer
  directory `xcode-select -p` names leads, and whether that is an
  executable file (`realpath`, `stat`).
- uv: `<tool environment>/uv-receipt.toml`, at the environment path its
  inventory returns; only the saved main-package requirement and the saved
  constraints and overrides naming the main package are inspected, for
  update checks and upgrade planning (uv's section).
- Cargo: `<CARGO_HOME>/.crates2.json` and `<CARGO_HOME>/.crates.toml`
  (Cargo's two install manifests, merged as the Cargo section says);
  whether `cargo-binstall` is on `PATH`.
- Ollama: whether `/Applications/Ollama.app` or `~/Applications/Ollama.app`
  is a directory; for each pulled model of a daemon identified as on this Mac,
  `~/.ollama/models/manifests-v2/ollama.com/{namespace}/{name}/{tag}` first,
  including its link target in `~/.ollama/models/blobs/sha256-<digest>`;
  `~/.ollama/models/manifests/registry.ollama.ai/{namespace}/{name}/{tag}` only
  when that model's v2 entry or target is missing. Both reads use the same
  protected-place, regular-file and 16 MiB limits; downgrade anchors and
  manifest lists are not compared, and nothing in the model store is
  written (Ollama's section).
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
  without waiting; a missing or unreadable file or key means `latest`,
  as it does to Claude Code; one in or through a protected place is not
  read, and since Claude Code still reads it, the channel is not known and
  the update check lists Claude Code as one it could not check).
  For an uninstall preview, when it is confirmed, and again right before
  each path is moved: `lstat` and the resolved path of each path on the
  uninstall list and of the folder it is in, the resolved home folder and
  the shared folders in it, the launcher's link text, and whether
  `~/.claude` and `~/.claude.json` exist and where they lead (Claude
  Code's section). After an uninstall: the same look at the launcher
  that detection makes (`lstat`, `readlink`, `realpath`, the same paths).
  When the launcher is gone, `removal::left_behind` also enumerates the
  recipe's remaining paths and checks each with `lstat`, with ownership
  and kept-path checks for optional items. The execution path makes this
  leftover check after the last move too (Claude Code's section). No
  version command runs.
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
  launcher that detection makes, then the same `removal::left_behind`
  check described for Claude Code, including another listing of
  `~/.local/bin` for `agy.<time>.old` backups. No version command runs.
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
  an uninstall: the same look at the launcher that detection makes, then
  the same `removal::left_behind` check described for Claude Code, including
  ownership and kept-path checks for optional paths. No version command
  runs. The preview of uninstalling
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
- Codex (its own install): whether `~/.local/bin/codex`
  exists and where it links to (`lstat`, `readlink`, `realpath`, also for
  the folder the link is in and for `~/.codex/packages/standalone`); where
  `~/.codex/packages/standalone/current` links to and whether that is a
  folder directly in `releases/` (`readlink`, `realpath`, `stat`);
  `~/.codex/packages/standalone/auto-update-version`, at most 256 bytes,
  when it is a regular file (opened without waiting or following a link,
  then `fstat`); for the notice under the source,
  each `PATH` directory's `codex`, as for Claude Code. Nothing else under
  `~/.codex` is read for this row, and no command runs (Codex's section);
  the preview of uninstalling npm's or Homebrew's Codex walks `~/.codex`
  for its size, names and sizes only (Data an uninstall leaves behind,
  above). For the uninstall preview of this one, when it is confirmed, and
  again right before each path is moved: the same reads as for Claude
  Code's list, for `~/.local/bin/codex-code-mode-host`,
  `~/.codex/packages/standalone` and `~/.local/bin/codex`; and whether
  `~/.codex` and `~/.zprofile` exist and where they lead (`lstat`,
  `realpath`; nothing in them is read). After an uninstall: the same look
  at the launcher that detection makes, and whether the paths on the list
  are there again, and nothing else — no version is read.
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
  time, never into a protected place); when one of those could not be
  followed, the names in a few of the package's own folders (`readdir`:
  a formula's `opt/<name>/bin`, a cask's app's `Contents/MacOS` and
  `Contents/Resources`, the `bin` of each folder in its `Caskroom/<token>`)
  — never a file's contents (What runs on a Homebrew package, above).
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

How many names one check reads from a folder, where it needs every name
there -- a keg's `bin` and `sbin` and a formula's folder in the Cellar, a
cask's `Caskroom/<token>/.metadata` and the version folders in it, the top
of `~/.cargo` and `~/.rustup`, `~/.rustup/toolchains` and `~/.cargo/bin`
for rustup, and the folder of a standalone tool's backup copies
(`~/.local/bin`): one name at a time, at most **4096 names and 2 seconds**
per check (`look::ListingBudget`), hidden names and names then left out
counted too. At most **one extra name** is read, to tell exactly 4096 from
more; it is never looked at or used. The time is checked before and after
each name; a read the file system is already stuck in cannot be cut short.
A check that runs out never uses the names it did read:

- Homebrew -- one budget for a keg's `bin` and `sbin` together, one for a
  formula's folder in the Cellar, one for a cask's whole `.metadata`
  search: the answer is "cannot tell", as for a folder that cannot be
  read. An update is planned as for a formula whose link is not recorded,
  and a link's preview names no commands and no conflicts (`brew link`,
  never given `--overwrite`, still stops at a file in the way); an
  uninstall gets no `--force`, an update no cleanup of old versions; a
  cask's uninstall preview says the general sentence, never one built from
  a caskfile chosen among part of the versions.
- rustup -- one budget per folder: the uninstall is refused
  (`NoSafeMethod`) at that folder, and no partial list of toolchains or
  programs is shown. A `~/.rustup/toolchains` or `~/.cargo/bin` that cannot
  be opened at all keeps the preview's wording from before: "every
  toolchain", and the programs `~/.cargo/.crates2.json` records, beside the
  line that the whole of `~/.cargo` goes.
- A standalone tool's uninstall -- one budget for all of its backup
  patterns, at each look: only a missing folder means no backups. One that
  cannot be listed in full (unreadable, protected, or too many names)
  refuses the preview at the pattern (`~/.local/bin/agy.*.old`, "isn't what
  was expected"), stops a confirmed run before its next move
  (`PathChanged`), and leaves the result after the last move unconfirmed;
  it never counts as "no backups left".

These are reads listed above; the limits add no command, network request
or written file. The dependency check, disk use, Which copy a command runs
and the Other Programs scan keep their own budgets, in their sections.

## Files Banager writes

Three, all in Banager's application data directory
(`~/Library/Application Support/com.brulek.banager`). `settings.json`
(`settings::save`, written to an exclusively created random
`settings.json.tmp.<random>` regular file beside it and renamed into
place, so overlapping processes do not share staging files and an existing
temporary symlink is not followed; a failed write removes its own staging
file, and a crash mid-write cannot leave the destination half-written; the
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

Ollama URL logins are removed when public instance ids are created, and
legacy ids are scrubbed before a history record
is kept and before ignored, skipped or snoozed keys are saved in settings
(`runner::redact::without_ollama_login`). The host, port and path remain;
the live adapter keeps its original URL for authentication. UI key
comparison applies the same removal, so history and reminders still
match live models. Older readable history and settings files are scrubbed
before being returned to the window and rewritten through their existing
atomic writer. A failed rewrite can leave the old bytes on disk; a newer
history format is still left untouched. No additional file is introduced.

Successful updates with an unfinished cleanup or missing links keep a
visible follow-up warning and View Log in the operation bar, batch result,
update row and Recent Updates. `follow_up_warnings` carries only the two
structured notes: `OldVersionsNotCleanedUp` (formula name and exit code)
and `NoLongerLinked` (formula name and affected command names). It does not
change `Succeeded`. These notes remain available when the bounded log is
evicted. They are optional in the existing history file; files without
them still read normally. After restart, View Log shows the saved warning
notes and existing recovery instructions, explicitly saying the full log
was not kept. This adds no command, host, permission or written file.

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
saw the change itself (the version it read before and after differ). An
update that succeeded because its package was already at the version its
confirmed plan aimed for when its turn came -- an earlier update of the
same Update all had upgraded it as a dependency, say -- also keeps that it
was so, and whether an earlier update of the same Homebrew and kind that
may have changed something had ended in between. Also the time the page's Clear was last pressed. Never a line of
a log, a command line or any other path, and of an error message one line
at most: a failure's cause is read from the tool's last lines as the
operation finishes, and the lines are dropped; only where they name no
cause, or name one whose words point at the tool's own (a file in the
way, something missing, a Mac the version does not support), is the
first line that says what went wrong kept, so that the page can say why,
and which file or what, after the window that watched it has closed -- at most 160
characters, its label (`Error:`) taken off (with the line after it where
it ends with a colon), with any home folder but `/Users/Shared` written
as `~` and any login, query, fragment or token-like part of the path in
an address masked (`failure_detail`). An operation cancelled before Banager began carrying it out
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
a random, exclusively created `history.json.tmp.<random>` beside it by the
same atomic writer as settings and renamed into place, on a thread of
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
Code, Antigravity CLI, Grok Build and Codex sections), for each path right after
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

**Through a proxy, as Terminal's or this Mac's settings say.** When the login shell's
settings name a proxy (How Banager runs anything), Banager's own requests
go through it, looked up at each request (`http::proxy::proxy_for`, which
`RealHttpClient` asks), by rules like curl's: `https_proxy` for an https
address and `http_proxy` for an http one, each read in lowercase first and
then in uppercase, then `all_proxy` / `ALL_PROXY`; a setting with nothing
in it counts as not set; and `no_proxy` / `NO_PROXY` -- names (each with
every name under it, written with or without a leading `.`), addresses,
address ranges such as `192.168.0.0/16`, or `*` for everything -- sends
what it names straight. They differ from curl's in two things: for an
http address `HTTP_PROXY` in uppercase is read too, as Go programs such as
`ollama` read it, where curl reads only the lowercase one; and a
`no_proxy` entry starting `*.` names what the rest of it names, where curl
takes no wildcard but a lone `*`. A setting the
login shell did not set is taken from Banager's own environment, which the
commands it runs inherit too. When neither names a proxy for a request,
it goes through the one this Mac's own network settings name (System
Settings → Network → a service → Details → Proxies): the web proxy (HTTP)
for an http address, the secure web proxy (HTTPS) for an https one --
what a proxy app such as Clash Verge, ClashX or Surge sets in its "system
proxy" mode, with nothing exported in a shell. Those are the settings
Banager's requests went by before it read the login shell's, read the same
way (`http::proxy::system_proxy`) and at each request, so turning such an
app on or off counts from the next check; as before, the SOCKS proxy and
the list of hosts to bypass in those settings are not read, so a machine
on the local network is reached through that proxy unless `no_proxy`
names it. The commands Banager runs are handed no setting from there: they
get the settings above, as in Terminal, and one that reads this Mac's
network settings itself (pip does) reads them as it does there. An https
request then goes as a `CONNECT`
tunnel through the proxy to the same host, and the certificate is checked
against that host as before; a `socks5://` or `socks5h://` proxy -- as
Clash and Surge print `all_proxy` -- is spoken to as SOCKS. This Mac itself
is reached never through a proxy, whatever the settings say: `localhost`
and any name under it, `127.0.0.1` and the rest of `127.0.0.0/8`, `::1`,
and `0.0.0.0` or `::` (an `OLLAMA_HOST` of `0.0.0.0` is common) -- so the
request to an Ollama daemon on this Mac always goes straight to it. The
proxy is then the one host Banager connects to that is not in the table
above, and only because the user's own settings -- Terminal's or this
Mac's -- name it; the host a
request is for is still checked against the table first, and a refused
one never reaches the proxy. A proxy setting that holds a login
(`http://name:password@host:port`) gives that login to that proxy alone.
The mirror settings change nothing here: Banager's own checks still ask
the hosts in the table.

The per-package registry phases for legacy pipx (PyPI), Cargo (crates.io)
and local Ollama models run at most four lookups at once per source
(`registry_checks` and `get_ok` in `crates/banager-core/src/adapters/mod.rs`).
A shared limit also allows at most four requests at once to pypi.org and
to registry.ollama.ai across source instances, and one to crates.io:
crates.io asks API users for at most one request per second
(<https://crates.io/data-access>, "crates.io API"), so its lookups stay
one after another, as before -- not overlapped, though not paced to one
per second either. The existing 30-second request timeout stays in
place. Each source's registry phase has a 120-second total budget,
including waits for a host permit; inventory and detection are outside
that budget. Completed answers are kept in inventory order. Requests
still waiting or not yet started at the deadline become transient
uncheckable rows, never claims that those tools are up to date. Dropping
a check cancels its pending requests; no background lookup tasks remain.

Every HTTPS request uses TLS through rustls; plain HTTP Ollama requests
are not encrypted. Every request has the header `User-Agent:
banager/<version>`; no other header of Banager's own, except `Accept` on
the Ollama registry request. The HTTP library supplies protocol headers,
including `Host` and `Accept: */*`. When `OLLAMA_HOST` contains URL userinfo
such as `http://name:password@server:11434`, reqwest also sends HTTP Basic
`Authorization` to that daemon. Over plain HTTP, those credentials are
base64-encoded, not encrypted. The HTTPS host allowlist still applies;
this does not enable an HTTPS daemon host outside it. A proxy's login,
above, goes to the proxy only. No cookies or other credentials are added,
and no information about this Mac is added to the request. Each request
has a timeout (listed in each source's table: 30 s unless stated, and the
daemon check in Ollama's detect is 10 s) and a response body limit of
8 MiB (`MAX_RESPONSE_BYTES`), and no redirect is ever followed — a 3xx is
an error. Nothing is ever sent by any method but `GET`.

A lookup that ends in one of those refusals — a redirect, a host off the
list (`HttpError::Refused`) — or in a secure connection rustls will not
set up because of the server's certificate (`HttpError::Tls`: one it
does not trust, as a proxy or security software that reads https traffic
presents, one that has expired by this Mac's clock, one for another name,
or none at all) would end the same way on the next check. Its row says
that it could not be checked — for a certificate, also "Couldn't
establish a secure connection to" the host — and it is not counted among
the tools that "couldn't be checked", whose notice asks to check again.
A handshake that fails any other way — a server that answers in plain
HTTP, as a Wi-Fi sign-in page does, an alert, a reset — counts as the
network. Of the requests that get no answer, only one whose connection
failed or timed out (`HttpError::Network`, `HttpError::Timeout`) is
counted; so is an answer of 408, 429 or a server error (`LookupFailure`
in `crates/banager-core/src/adapters/mod.rs`).

Three things are outside that client and worth saying out loud. The
window itself cannot make a network request: its content security policy
is `connect-src 'self'` (`src-tauri/tauri.conf.json`), and a navigation
to another address, which that policy does not stop, is refused
(`src-tauri/src/navigation.rs`: the window loads Banager's own page,
`tauri://localhost`, and in a development build the Vite server on
`localhost`, and no other address). The Tauri opener
plugin — the one that opens a URL or a path in another application — is
not built into Banager: `src-tauri/Cargo.toml` does not depend on it,
`run()` in `src-tauri/src/lib.rs` registers no such plugin, and
`package.json` does not install its script, so
`src-tauri/capabilities/default.json` could give the window no `opener:`
permission to call. Nothing in Banager called it once Banager's own
`reveal_in_finder`, the Other Programs page's Show in Finder, asked
Finder through AppKit to show a file the newest scan found, which
connects to nothing (Unknown-source scan, above). Nor can the window
have Banager open just any URL. A tool's homepage in the Installed page's
details is a link (the author's request of 2026-10-07, in place of
decision S9's copy-only): a click sends the address shown to Banager's own
`open_homepage` (`src-tauri/src/homepage.rs`), which has the default
browser open it, through AppKit (`NSWorkspace openURL:`, which starts the
browser when it is not running: with the Open Ollama button, the one
application Banager launches outside the runner), only when it is,
exactly, the homepage of a tool in the current snapshot, trimmed, as that
tool's source reported it (Homebrew's `homepage`, a standalone
installer's recipe) -- and an `https` address with a host: decision S9
allows a source's `https` homepage, so a plain `http` homepage is shown to
copy, not as a link. Any
other address is refused before anything is parsed (`not_listed`), and a
homepage a tool lists that is any other kind of address -- plain `http`,
`file:`, `ftp:`, an app's own scheme -- is refused too (`not_web`). No command
runs and Banager connects to nothing: the browser loads the page, under
the browser's own settings. The window is given no new permission for
it: Banager's own commands are behind no permission of their own
(`src-tauri/build.rs` declares no app manifest), and
the homepage link adds nothing to `src-tauri/capabilities/default.json`;
the opener plugin stays out. The window itself still never leaves Banager's page
(`src-tauri/src/navigation.rs`). And the Tauri updater
plugin is compiled in and configured with the endpoint
`https://github.com/Brulek/Banager/releases/latest/download/latest.json`
(`src-tauri/tauri.conf.json`, `plugins.updater`), but nothing in Banager
calls it yet, and the window is given none of its commands
(`src-tauri/capabilities/default.json` has no `updater:` permission), so
no request to it is made; when app self-update ships, this paragraph
changes.

**Window permissions.**

The window is given only these of Tauri's permissions
(`src-tauri/capabilities/default.json`): `core:event:allow-listen` and
`core:event:allow-unlisten`, to hear the events Rust sends the page;
`core:window:allow-start-dragging` and
`core:window:allow-internal-toggle-maximize`, which Tauri's own script
for the title bar's drag regions calls to move the window and to zoom it
on a double-click; `core:window:allow-set-badge-count`, for the Dock
badge; and `notification:allow-is-permission-granted`, which the
notification plugin's own script calls as the page loads.
`core:image:deny-from-path` still refuses the one command that reads an
image file by the path it is given. Each of the six was among what the
window had before -- Tauri's `core:default`, which this list replaced on
2026-10-07, or a permission given beside it -- so the change only took
permissions away, the native menu's among them. Rust builds the menu
(`menu::show` in `src-tauri/src/menu.rs`) and the page names its language
through Banager's own `set_menu_language`, so the page needs none of the
menu's commands; a `plugin:menu|new` asking for a `Predefined` item with
no `options`, which makes Tauri 2.11.5's handler panic, is refused before
the handler reads it. No command, host or written file is added.
`window_rights` in `src-tauri/src/lib.rs` checks, against the ACL `run()`
builds, that of every plugin command in the build the window reaches
these six and no other, and none of them from another window or another
address.

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
Banager neither chooses nor sees them. The proxy and mirror settings it
hands them are the user's own, read from the login shell as they are (How
Banager runs anything). One flag narrows them: Cargo's
install and upgrade commands name crates.io's index, so that a
`registry.default` naming another registry cannot swap the crate for a
namesake. Each is given the index it reads when nothing else is
configured, `index.crates.io`, and cargo still follows a
`[source.crates-io]` mirror (Cargo section).

## What Banager never does

- Never runs a shell for any command, and never pipes a download into one
  (`curl … | sh`). The one shell run is the `PATH` read at launch, above.
- Never runs an installer script, and never reruns one to update a tool.
- Never runs `rustup update`: rustup's own update of its toolchains, which
  an interruption leaves half installed. Only `rustup self update`, which
  replaces rustup alone. A refresh that begins while an update or uninstall
  of rustup is under way runs neither `rustup` nor `cargo`. Detection holds
  the same resource locks, including on the first refresh; an update or
  uninstall submitted afterwards waits until detection ends or is aborted
  (rustup's section). Other reads of either source also run under that
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
- Never passes `--zap`, `--ignore-dependencies` or `--overwrite` to
  Homebrew, nor `--force` but to the uninstall of a formula with more than
  one version installed and no pin, so that every version goes (the brew
  plan tests; the author's decision U9), and to the one `brew link`
  Banager runs, `brew link --formula --force {name}` of a keg-only
  formula -- after the update of one whose link Homebrew recorded, and on
  its own for one another source's launcher needs (Why a source did not
  answer) -- which without `--overwrite` stops rather than overwrite
  another program's file -- but for a cask's link, which Homebrew
  replaces, and the formulae it names as linking over it, which it
  unlinks first (Homebrew's section, "Keg-only formulae linked into
  Terminal"); and never runs a bare
  `brew upgrade`, a bare `brew cleanup` or a bare `brew link`.
- Never runs a `brew` command without `HOMEBREW_NO_AUTOREMOVE=1`, which
  keeps Homebrew from uninstalling packages the command does not name,
  and `HOMEBREW_NO_INSTALL_CLEANUP=1`, which keeps an install or upgrade
  from ending in Homebrew's clean-up, which deletes the older versions of the
  package it names and of any it updates along with it, and stray old
  downloads, every time, and those of all
  Homebrew software when its periodic clean-up is due; when a `brew.env`
  file takes either back, the preview says so, and one that takes
  `HOMEBREW_NO_INSTALL_CLEANUP` back only after the preview stops the
  install or upgrade before it starts (Homebrew's section). What
  Banager runs in its place deletes the installed old versions of the one
  formula it just upgraded, that formula's outdated downloads in
  Homebrew's cache and every unreferenced download there, whichever package
  it was for -- which the update's preview says -- and no other installed
  software, only where Homebrew would have done that by itself and the
  person turned nothing of it off (Homebrew's section, "Old versions").
- Never runs a `brew` command as root.
- Never uninstalls a uv tool while `UV_TOOL_DIR` is set in Banager's
  environment: removing the last tool, uv would then also delete the
  folder above that one, with every file in it, when that folder holds no
  other folder (uv's section).
- Never runs a write command from a refresh, and accepts a confirmed
  preview only within ten minutes of its issue (`Session::submit`). An
  accepted operation can wait longer than ten minutes in the queue before
  it runs; expiry is checked at submission, not again at execution. The `brew
  update` a refresh runs is Homebrew's exception: it can install, move or
  uninstall Homebrew packages by itself when Homebrew has moved a package
  between a formula and a cask, or renamed one (Homebrew's section).
- Never lets the window ask for an install: it can ask for the preview
  of an upgrade, an uninstall or a link only, and `plan_operation_impl`
  (`src-tauri/src/ipc.rs`) refuses an install before any source is
  asked, whatever it names. Nor by another name: the window may ask for
  an upgrade only of an update the last check listed, an uninstall
  only of a tool it listed installed, and a link only of a formula a
  source's reason offers (`Session::issue_listed_plan`),
  since npm, Cargo and Ollama would install a name they were asked to
  upgrade.
- Never launches an application from a refresh; `open -a Ollama` runs
  only when the button is pressed, and the default browser is started
  only by a click on a tool's homepage.
- Never opens just any web address the window names: a click on a tool's
  homepage opens, in the default browser, only an `https` homepage a
  tool in the current snapshot lists (Network).
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
  download cache; for Antigravity CLI, Grok Build and Codex, which publish
  no removal steps, Banager's own reading of how each was installed);
  never moves the settings, login and history Claude Code keeps
  in `~/.claude` (of that folder only its download cache,
  `~/.claude/downloads`) or `~/.claude.json`, the login, sessions, memory
  and settings Grok Build keeps in `~/.grok` (of that folder only
  `downloads/`, `bundled/`, `completions/` and the two links in `bin/`),
  the settings, login and sessions Codex keeps in `~/.codex` (of that
  folder only `packages/standalone`, the program its installer unpacked),
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
  version before could be read, unless both readings are at least the
  version the confirmed update aimed for (an earlier update of the same
  Update all had already upgraded it, say), which is reported as done and
  says so (`run_operation`'s `already_at_target`); when Homebrew's index
  was updating and the reading before was refused, presence afterwards is
  all there is to go on (Homebrew's section).

## Known bugs not fixed yet

Each of these is reproduced by a test that is kept out of the ordinary run
(`#[ignore = "bug: …"]`, or `it.skip("bug: …")` for the window's) and fails
until the bug is fixed; `cargo test -- --ignored` runs the Rust ones. Until
then, what this document says above holds only as narrowed here.

- When the login shell's output has two `PATH=` lines (a multi-line
  variable can hold one), Banager uses the first instead of keeping the
  `PATH` it inherited.
- A Homebrew uninstall still runs when, since its confirmation was shown,
  `brew.env` has turned autoremove on or now sits in a protected place
  Banager does not look into; a cask uninstall still runs when the cask's
  install receipt has gained removal steps the confirmation did not show.
- An npm uninstall runs against whatever global prefix npm is set to when
  it starts, which can be another installation than the one confirmed.
- A Cargo update still runs `cargo install --force` from crates.io after
  the installed copy was replaced from Git or a local path, or rebuilt with
  other features, since its confirmation; a uv upgrade still runs after
  the tool's receipt gained a version constraint or became unreadable.
- Hiding an update on the Updates page, then changing a setting in
  Settings before that first save has finished, can lose the hidden
  update even when both saves succeed.

Put Back itself is still untested (“Moving files to the Trash” above).
Testing it needs a throwaway macOS account, Banager started from Finder
without Full Disk Access, its paced removal of a throwaway file, folder and
launcher link, and Put Back clicked in Finder, then a check of their
paths, contents and link target, with the macOS version recorded. None of
that has been set up.

## 简体中文：运行与隐私要点

- 软件包管理操作直接传入参数，不经过 shell。启动时会另行运行登录 shell，读取环境设置；启动文件也会执行。读取失败后，后续刷新会重试。原生模拟窗口也有这一步，浏览器模拟没有。
- 普通测试跳过 11 项：2 项真实 Homebrew 测试、1 项废纸篓测试、1 项 AppKit 测试、1 项磁盘探测和 6 项性能测试。安装卸载与废纸篓测试需要显式启用，详见 README。
- Homebrew 公式更新通常会预览并在成功后运行指定名称的清理，删除该公式的旧版本、过期缓存下载及缓存中所有未引用的下载。固定版本、无法读取版本或固定记录、用户关闭清理、`brew.env` 启用自动清理或无法确定其影响时，不安排这一步。`brew.env` 启用的自动清理范围更广，预览会另行说明。
- 未固定且装有多个版本的公式，卸载会移除所有已安装版本及链接，确认框会列出版本。保存在其他位置的设置和数据保留；启用自动移除依赖时，会另行说明。
- `brew.env` 读取清单包括 `HOMEBREW_NO_AUTO_UPDATE`。它被设为空时，会阻止可能触发未跟踪自动更新的命令。
- `OLLAMA_HOST` 地址中的登录信息会用于该服务的 HTTP 基本认证。普通 HTTP 不加密这些信息。发给窗口的实例标识不含登录信息；命令预览、操作摘要和库存错误会遮蔽登录信息，实际请求和命令仍使用原值。拷贝的预览命令也保留遮蔽，需要自行补入登录信息。历史和忽略、跳过、稍后提醒设置保存前会去除地址中的用户名与密码；旧文件读取时也会脱敏并尝试重写。写入失败可能使旧内容仍留在磁盘，较新格式的历史文件不会被覆盖。
- Claude Code、Antigravity CLI 和 Grok Build 卸载后，除了检查启动器，还会检查卸载清单中的其他路径；Antigravity 也会重新列出备份。可选路径还会检查归属及应保留的路径，不运行版本命令。
- 预览必须在生成后 10 分钟内确认并提交。已接受的操作可以排队超过 10 分钟再执行。
- 更新后版本未变通常需要检查；若已达到或超过确认的目标版本，则报告已更新。这也适用于 Grok Build 和 rustup。没有可比较的目标版本时，不适用此例外。

## 繁體中文：執行與隱私要點

- 套件管理操作直接傳入參數，不透過 shell。啟動時會另外執行登入 shell，讀取環境設定；啟動檔也會執行。讀取失敗後，後續重新整理會重試。原生模擬視窗也有這一步，瀏覽器模擬沒有。
- 一般測試略過 11 項：2 項實際 Homebrew 測試、1 項垃圾桶測試、1 項 AppKit 測試、1 項磁碟探測及 6 項效能測試。安裝移除與垃圾桶測試需要明確啟用，詳見 README。
- Homebrew 公式更新通常會預覽並在成功後執行指定名稱的清理，刪除該公式的舊版本、過期快取下載及快取中所有未參照的下載。固定版本、無法讀取版本或固定記錄、使用者關閉清理、`brew.env` 啟用自動清理或無法確定其影響時，不安排這一步。`brew.env` 啟用的自動清理範圍更廣，預覽會另外說明。
- 未固定且裝有多個版本的公式，移除時會移除所有已安裝版本及連結，確認視窗會列出版本。儲存在其他位置的設定和資料保留；啟用自動移除相依套件時，會另外說明。
- `brew.env` 讀取清單包括 `HOMEBREW_NO_AUTO_UPDATE`。它被設為空值時，會阻止可能觸發未追蹤自動更新的命令。
- `OLLAMA_HOST` 網址中的登入資訊會用於該服務的 HTTP 基本驗證。一般 HTTP 不會加密這些資訊。傳給視窗的實例識別碼不含登入資訊；命令預覽、操作摘要和庫存錯誤會遮蔽登入資訊，實際要求和命令仍使用原值。拷貝的預覽命令也保留遮蔽，需要自行補入登入資訊。歷程和忽略、略過、稍後提醒設定儲存前會去除網址中的使用者名稱與密碼；舊檔案讀取時也會遮蔽登入資訊並嘗試重新寫入。寫入失敗可能使舊內容仍留在磁碟，較新格式的歷程檔案不會被覆寫。
- Claude Code、Antigravity CLI 和 Grok Build 移除後，除了檢查啟動器，還會檢查移除清單中的其他路徑；Antigravity 也會重新列出備份。選用路徑還會檢查歸屬及應保留的路徑，不執行版本命令。
- 預覽必須在產生後 10 分鐘內確認並送出。已接受的操作可以排隊超過 10 分鐘再執行。
- 更新後版本未變通常需要檢查；若已達到或超過確認的目標版本，則回報已更新。這也適用於 Grok Build 和 rustup。沒有可比較的目標版本時，不適用此例外。
