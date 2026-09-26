# What Canager Runs

Every command Canager runs, every file it reads, writes or moves to the
Trash, every host it connects to and every environment variable it sets,
for the eleven sources it manages today: Homebrew, npm, pipx, uv, pip
(read-only), Cargo, Ollama, and four tools with their own installer:
Claude Code, Antigravity CLI, Grok Build and rustup. Each sentence
describes what the code does now and names the function it describes, so
it can be checked against `crates/canager-core/src/adapters/` rather than
believed. `crates/canager-core/tests/what_we_run_test.rs` checks the parts
a test can: a section per registered source, every host on the https
allowlist, every environment variable Homebrew's and npm's commands are
given, that Cargo's section says every Cargo command is given the
variables in `CargoAdapter::ENV` and shows each one, the three Homebrew
flags this file promises are never passed, that the unknown-source scan's
section states the two limits the code enforces, that the sections of the
three tools uninstalled by moving files to the Trash (Claude Code,
Antigravity CLI, Grok Build) name every path those uninstalls move or
keep and their time budget, and the never-list every path of settings or
state they keep, that Grok Build's section shows the update check it runs
on every refresh and says it installs nothing, and that the Trash section
names the call and states the pause after each move.

Throughout, `<brew>`, `<npm>` and so on stand for the absolute path of the
executable the adapter found; `{name}` is the one user-chosen argument a
command can carry.

## How Canager runs anything

**Never through a shell.** Every package-manager command is a fixed argv
array run directly against an absolute program path by `RealRunner::run`
(`crates/canager-core/src/runner/real.rs`): `Command::new(program)` with
the arguments appended one by one. No string is ever handed to `sh`, and
nothing Canager downloads is ever piped into one.

**One shell run, at launch, that runs no command.** An app opened from
Finder starts with a minimal `PATH`, so at startup (`run()` in
`src-tauri/src/lib.rs`) the `fix-path-env` crate runs the user's login
shell once — `$SHELL` (`/bin/zsh` on a Mac when `SHELL` is unset) with the
arguments `-ilc 'echo -n "_SHELL_ENV_DELIMITER_"; env; echo -n
"_SHELL_ENV_DELIMITER_"; exit'`, with `DISABLE_AUTO_UPDATE=true` in its
environment and the home folder as its working directory — reads the
`PATH` that shell exports, and sets it on Canager's own process
(`fix_vars` in `fix-path-env-rs` at the pinned commit `c4c45d5`). That is
the only time a shell is involved, and all it does is print the
environment.

**What a command inherits.** A child gets Canager's own environment — the
`PATH` above and whatever else the login shell exported — plus the
variables listed in each source's section below (`RealRunner::run` adds
them with `envs` and never clears the environment). Its stdin is
`/dev/null`, so a tool that asks a question gets end-of-file rather than a
wait; its stdout and stderr are piped and, for a write command, streamed
line by line into the operation log. Each child runs in its own process
group. Every command has a timeout (listed below; `RealRunner` caps any
timeout at 24 hours); on timeout or cancel the whole group gets `SIGTERM`,
a grace period, and then `SIGKILL` for whatever is left.

**Where the program comes from.** At launch (`run()` in
`src-tauri/src/lib.rs`), at the start of every refresh, when the Open
Ollama button is pressed, and at the start of every Unknown-page scan,
`HostEnv::discover`
(`crates/canager-core/src/runner/path_env.rs`) reads `PATH`, `HOME`,
`CARGO_HOME`, `RUSTUP_HOME`, `ZDOTDIR` and `OLLAMA_HOST` from Canager's
environment and the effective user id from the process. Every package manager
but Homebrew finds its executable with `resolve_exe`: the first directory
on that `PATH` containing a regular file of that name. Homebrew is looked
for at three fixed paths instead (its section), and so is a tool with its
own installer: Claude Code at `~/.local/bin/claude`, Antigravity CLI at
`~/.local/bin/agy`, Grok Build at `~/.grok/bin/grok`, rustup at
`$CARGO_HOME/bin/rustup` (their sections). The path that was found is the
one previewed and the one run.

**What a user-chosen value may look like.** A package name reaches an
argv only after `validate_package_name`
(`crates/canager-core/src/adapters/mod.rs`): `^[A-Za-z0-9@._+/-]+$`, not
starting with `-`, `/` or `.`, no `..` segment, no `.rb` suffix. Two
sources have their own rule for their own shape of input: npm's search
box (`validate_search_query`: once surrounding whitespace is trimmed,
non-empty, not starting with `-`, and at most 200 bytes of UTF-8 — a CJK
character is three of those; npm receives the query untrimmed) and
Ollama's model references, which contain a colon
(`validate_model_reference`). Every other token in every argv below is a
fixed string.

**Root.** Homebrew refuses to run as root, so Canager never runs a `brew`
command when its effective user ID is 0 (`refuse_if_root`); a Homebrew
found under root is listed as refusing, not as missing. No other source
checks.

**Passwords.** Canager never asks for a password and never handles one.
The only thing it does with one is pass `SUDO_ASKPASS` through, unchanged,
to Homebrew cask installs and upgrades when the variable is already set
in Canager's environment (Homebrew's section); it never sets it on its
own behalf.

## When commands run

**A refresh** happens when the window opens (`refreshIntoCache(…,
"initial")` in `src/lib/events.ts`), when the user presses a Retry or
Refresh control (the status bar after a failed refresh, a source notice),
after every operation finishes, when the "include self-updating apps"
setting changes, after Ollama is opened from its notice, and whenever a
`brew update` a refresh left running in the background ends
(`refresh_on_background_change` in `src-tauri/src/ipc.rs`). Within a
refresh (`refresh_round` in `crates/canager-core/src/session/refresh.rs`)
every source's detect runs concurrently; then, for each instance found,
under that instance's lock, its inventory is read and then its update
check runs. Everything a refresh runs is in the read-only tables below:
no refresh runs a write command, moves a file, launches an application or
asks for a password.

**An operation** is previewed first: `plan` builds the exact argv — or,
for an uninstall that runs no command, the exact list of paths it will
move to the Trash (the Claude Code, Antigravity CLI and Grok Build
sections) — and the front end shows it (`plan_operation` in
`src-tauri/src/ipc.rs`; the front end never builds an argv and sends back
only the id of a plan Rust issued). The plan can be confirmed for ten
minutes (`PLAN_LIFETIME` in `crates/canager-core/src/session/plans.rs`),
after which it has to be previewed again. Before a plan is built,
`Session::issue_plan` refuses an operation on a source that is read-only
or not answering, an upgrade or uninstall the tool itself reports it will
refuse (a pinned package), and an update of a tool that installs its
updates itself and has no update command Canager may run (Antigravity
CLI's section) — the buttons the pages hide are backed by that refusal,
not only by the page. On confirmation
`run_operation` (`crates/canager-core/src/ops/mod.rs`) takes the plan's
locks, runs the command (or moves the listed paths to the
Trash), and then re-reads the inventory to check what actually happened;
an upgrade is also preceded by a reading,
so the version before can be compared with the version after. An install
after which the package is not present, an uninstall after which it still
is, and an upgrade that exits 0 with the version unchanged are all
reported as needing attention, never as success. The one case with less
to go on: when the reading before an upgrade was refused — on Homebrew,
while a `brew update` a refresh left running is still going (Homebrew's
section) — there is nothing to compare, and an upgrade that exits 0 is
reported as a success whenever the package is still present afterwards,
whether or not its version moved. A command that was
cancelled or timed out, or that a signal Canager did not send ended
(killed from Activity Monitor, say), is reported as unconfirmed unless
the reading after settles it: an install after which the package is
present, or an uninstall after which it is gone, is reported as
succeeded, and one the user cancelled that did not take effect as
cancelled; an upgrade stopped partway is never settled either way
(`run_plan` in `crates/canager-core/src/adapters/mod.rs`, then
`run_operation`).

## Homebrew

Adapter: `BrewAdapter` in `crates/canager-core/src/adapters/brew/mod.rs`.
Verified against Homebrew 7.0.3 (`adapters/meta/brew.toml`).

**Detect.** Canager checks whether `/opt/homebrew/bin/brew`,
`/usr/local/bin/brew` and `/home/linuxbrew/.linuxbrew/bin/brew` exist
(`BrewAdapter::CANDIDATE_PATHS`) — never a `brew` resolved through `PATH`
— and runs `<brew> --version` (30 s) for each that does. Each is its own
instance, with the prefix two directories up from the executable.

**Environment applied to every invocation** (`BrewAdapter::ENV`),
including `--version`, `update` and every plan:

    HOMEBREW_NO_AUTO_UPDATE=1
    HOMEBREW_NO_ENV_HINTS=1
    HOMEBREW_NO_INSTALL_CLEANUP=1
    NO_COLOR=1

Install and upgrade plans additionally carry `SUDO_ASKPASS` when it is
already set in Canager's process environment (`askpass_fn`, read per
plan). It only has any effect for casks whose installer scripts invoke
`sudo`.

**Read-only commands** (background checks; never need a password):

| Purpose | Argv | Timeout |
|---|---|---|
| Detect a Homebrew install | `<brew> --version` | 30 s |
| List installed formulae + casks (`inventory`) | `<brew> info --installed --json=v2` | 120 s |
| Refresh Homebrew's local package index (`maybe_update`) | `<brew> update` | see below |
| List outdated formulae + casks (`check_updates`) | `<brew> outdated --json=v2`, plus `--greedy` when the "include self-updating apps" setting is on | 120 s |
| Qualify the names `outdated` reported (once per `check_updates`) | `<brew> info --installed --json=v2` | 120 s |
| Search by name | `<brew> search {query}` | 30 s |
| Search by name + description | `<brew> search --desc {query}` | 30 s |
| List installed formulae depending on a formula (uninstall preview) | `<brew> uses --installed {name}` | 120 s |

`brew update` runs at most once per six hours per prefix (`update_ttl`).
A refresh waits up to two minutes for it (`UPDATE_PATIENCE`) and then
leaves it running rather than killing it — a `brew update` stopped halfway
can leave Homebrew's git checkout locked; only after thirty minutes
(`UPDATE_BACKSTOP`) is it stopped. While one is running, `inventory`,
`check_updates` and the uninstall preview do not read the catalogue at
all (`AdapterError::IndexUpdating`): the pages keep the previous answer
and say the index is updating, and refresh again when it ends. A `brew
update` that failed is reported as a note on the source (the list may be
out of date), not as a failed source. The search query passes
`validate_package_name`.

**Write commands** (only run after the user reviews and confirms a plan
preview):

| Purpose | Argv | Timeout | Needs a password |
|---|---|---|---|
| Install a formula | `<brew> install --formula {name}` | 1800 s | No |
| Install a cask | `<brew> install --cask {name}` | 1800 s | Sometimes — some cask installers invoke `sudo`; `SUDO_ASKPASS` is passed through when set |
| Uninstall a formula | `<brew> uninstall --formula {name}` | 1800 s | No |
| Uninstall a cask | `<brew> uninstall --cask {name}` | 1800 s | Sometimes — some cask uninstalls invoke `sudo` (removing a `pkgutil` receipt, a launch daemon, a kernel extension) |
| Upgrade one formula | `<brew> upgrade --formula {name}` | 1800 s | No |
| Upgrade one cask | `<brew> upgrade --cask {name}` | 1800 s | Sometimes — as for install |

Every one of these argvs is exactly the verb, the kind flag and the name
(`test_plan_never_passes_zap_force_or_ignore_dependencies` in the same
file). Canager never passes `--zap`, `--force` or `--ignore-dependencies`
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
Canager's or anyone's — overlapped its `brew uses` read
(`probe_homebrew_update_lock`): the directory is `stat`ed, the file is
opened read-only and never created, and `fcntl(F_GETLK)` asks whether the
lock is held without taking it.

## npm

Adapter: `NpmAdapter` in `crates/canager-core/src/adapters/npm.rs`.
Verified against npm 12.0.2 (`adapters/meta/npm.toml`).

**Detect.** `npm` is the first `npm` on `PATH`. Canager runs `<npm>
prefix -g` (30 s) to learn the global prefix, which is the instance's
identity, and `<npm> --version` (30 s), then asks `access(2)` whether the
current user can write `{prefix}/lib/node_modules` — or, when that does
not exist yet, `{prefix}/lib` or `{prefix}` (`real_prefix_is_writable`).
A prefix this user cannot write (a Node installed from nodejs.org's
package leaves a root-owned one) makes the instance read-only. An npm that
will not answer `prefix -g` is still listed, as not responding.

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

## pipx

Adapter: `PipxAdapter` in `crates/canager-core/src/adapters/pipx.rs`.
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

On a pipx older than 1.16, which has no `list --outdated`, Canager
instead asks PyPI about each installed tool: `GET
https://pypi.org/pypi/{name}/json` (30 s each), the name percent-encoded.
A tool PyPI does not answer for is listed as "could not check", never as
an error for the whole source. pipx has no search command Canager uses.

**Write commands:**

| Purpose | Argv | Timeout | Needs a password |
|---|---|---|---|
| Install | `<pipx> install {name}` | 600 s | No |
| Uninstall | `<pipx> uninstall {name}` | 600 s | No |
| Upgrade | `<pipx> upgrade {name}` | 600 s | No |

## uv

Adapter: `UvAdapter` in `crates/canager-core/src/adapters/uv.rs`.
Verified against uv 0.12.17 (`adapters/meta/uv.toml`).

**Detect.** `uv` is the first `uv` on `PATH`; `<uv> --version` (30 s). No
environment variables are added to any uv command, and Canager makes no
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
path only. uv has no tool-search command Canager uses.

**Write commands:**

| Purpose | Argv | Timeout | Needs a password |
|---|---|---|---|
| Install | `<uv> tool install {name}` | 600 s | No |
| Uninstall | `<uv> tool uninstall {name}` | 600 s | No |
| Upgrade | `<uv> tool upgrade {name}` | 600 s | No |

## pip (read-only)

Adapter: `PipAdapter` in `crates/canager-core/src/adapters/pip.rs`.
Verified against pip 26.2.1 (`adapters/meta/pip.toml`).

**Detect.** For each of `python3.14`, `python3.13`, `python3.12`,
`python3.11`, `python3.10`, `python3` and `python` found on `PATH`
(`PipAdapter::CANDIDATE_INTERPRETERS`), Canager canonicalises the path so
two names for one interpreter count once, and runs `<python> -m pip
--version` (30 s). Every pip instance is read-only by design. No
environment variables are added, and Canager makes no network request of
its own for pip: `pip list --outdated` reaches PyPI itself.

**Read-only commands:**

| Purpose | Argv | Timeout |
|---|---|---|
| Version | `<python> -m pip --version` | 30 s |
| List packages (`inventory`) | `<python> -m pip list --format=json` | 60 s |
| List packages nothing else depends on (`inventory`, to tell dependencies apart) | `<python> -m pip list --format=json --not-required` | 60 s |
| List outdated packages (`check_updates`) | `<python> -m pip list --outdated --format=json` | 60 s |

**Write commands: none.** `PipAdapter::plan` refuses every install,
uninstall and upgrade before building an argv, so no pip write command
can be previewed, let alone run; the pages show no such button for a pip
package. pip has no search command Canager uses.

## Cargo

Adapter: `CargoAdapter` in `crates/canager-core/src/adapters/cargo.rs`.
Verified against cargo 1.98.1 (`adapters/meta/cargo.toml`).

**Detect.** `cargo` is the first `cargo` on `PATH`; `<cargo> --version`
(30 s). Canager also looks for `cargo-binstall` on the same `PATH` and
remembers the path found for plans. `CARGO_HOME` is read as cargo itself
reads it: unset or an empty value means the default `~/.cargo`; an
absolute value is the Cargo home; a relative value names a folder
relative to cargo's own working directory, which Canager cannot know, so
Canager then lists no Cargo source rather than guess.

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
in (a missing file means nothing is installed). For each crate it also
records the program the crate installed, `<CARGO_HOME>/bin/<binary>` (the
binary named after the crate when there is one, else the first the record
lists), which the Unknown page uses to place that program under Cargo
rather than list it. `check_updates` reads the
same file and, for each crate installed from the registry, asks crates.io
once: `GET https://crates.io/api/v1/crates/{name}` (30 s), the name
percent-encoded. Crates installed from a git repository or a local path
are never looked up; they are listed as "could not check" with that
reason. Cargo has no search command Canager uses.

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
binary. It is the only `--force` Canager passes to any tool, and it never
goes to Homebrew.

## Ollama

Adapter: `OllamaAdapter` in `crates/canager-core/src/adapters/ollama/mod.rs`.
Verified against Ollama 0.34.1 (`adapters/meta/ollama.toml`).

**Detect.** `ollama` is the first `ollama` on `PATH`; `<ollama> --version`
(30 s) — never `ollama list`, which on macOS launches Ollama.app as a side
effect, and a background refresh must never launch an application. The
daemon is asked over HTTP instead: `GET {host}/api/tags` (10 s), where
`{host}` is `OLLAMA_HOST` from the environment, normalised to an absolute
http(s) URL (a bare `host:port` gets `http://` in front; a value that
does not make an http(s) URL is ignored and the default used), or
Ollama's default `http://127.0.0.1:11434` (`DEFAULT_HOST`). Canager also
checks whether `/Applications/Ollama.app` or `~/Applications/Ollama.app`
is a directory: a daemon on this Mac that does not answer while the app
is there is reported as not running, with an Open Ollama button; anything
else that does not answer is reported as not responding, with no button.
No environment variables are added to any ollama command.

One `OLLAMA_HOST` survives that normalisation and is then never asked:
an `https://` `OLLAMA_HOST` is refused by the https allowlist in the
Network section, which exempts `http` only, so Canager never sends the
request, and the daemon is reported exactly as one that did not answer —
not responding, or, when the address is this Mac and Ollama.app is there,
not running with an Open Ollama button that cannot help, since the next
request is refused the same way. Nothing on screen says that it was
Canager that refused. Recorded in `docs/superpowers/backlog.md`.

**Read-only reads:**

| Purpose | Request or argv | Timeout |
|---|---|---|
| Version | `<ollama> --version` | 30 s |
| Is the daemon answering (detect) | `GET {host}/api/tags` | 10 s |
| List pulled models (`inventory`) | `GET {host}/api/tags` | 30 s |
| Is a model current (`check_updates`, per model) | `GET https://registry.ollama.ai/v2/{namespace}/{name}/manifests/{tag}` with `Accept: application/vnd.docker.distribution.manifest.v2+json` | 30 s |

For each pulled model `check_updates` reads the local manifest file
`~/.ollama/models/manifests/registry.ollama.ai/{namespace}/{name}/{tag}`
and compares its layer digests with the registry's. The three name parts
come out of the daemon's `/api/tags` answer, so before any path is built
each must be a plain path segment (`contained_manifest_path`: nothing
absolute, no `..`), and in the URL each is percent-encoded. The registry
manifest is always fetched from `registry.ollama.ai`, whatever registry
the model was pulled from. Ollama has no search command Canager uses.

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
is what will contact it, under Ollama's own configuration.

**The Open Ollama button** runs `/usr/bin/open -a Ollama`
(`open_ollama_app_argv` in `src-tauri/src/ipc.rs`), with its stdin,
stdout and stderr pointed at `/dev/null`, only when the user presses it and only when
Ollama.app was found; it waits up to 20 seconds for `open` to report
whether LaunchServices accepted the request. It is the one launch in the
app that is not a package-manager command, and it never happens during a
refresh.

## Claude Code

Adapter: `StandaloneAdapter` over the `CLAUDE` recipe in
`crates/canager-core/src/adapters/standalone/` (`recipes.rs` is the data,
`mod.rs` the behaviour, `route.rs` the recognition). Verified against
Claude Code 2.1.282 (the version in `adapters/meta/standalone-claude.toml`
and the name of the recorded fixture directory). The first source that is
not a package manager: the row is one tool, installed by its own installer
(`curl -fsSL https://claude.ai/install.sh | bash`, run by the user —
Canager never runs it), and the one item under it is the tool itself.

**Detect.** Canager looks at the fixed path the installer writes,
`~/.local/bin/claude` — never a `claude` found through `PATH`, which on a
Mac with the Homebrew cask earlier on `PATH` would be that copy instead —
and checks with `lstat`, `readlink` and `realpath` that it is a symbolic
link whose own text points into `~/.local/share/claude` (the installer's
`versions/<version>` store) and that resolves there; a `claude` that
reaches that folder only through another link outside it is not the
installer's layout and is not listed (the Unknown page shows it). A
`claude` there that resolves into a `Cellar`, `Caskroom`, `node_modules`
or `corepack` directory is a package manager's copy (Homebrew's, npm's or
corepack's) and is not listed here; a plain file at that path is not this
route and is not listed either. A dangling link whose own text points
into `~/.local/share/claude` (the program files were removed by hand or
by another tool, or by an uninstall that stopped partway) is listed with
no version and a notice saying so, and its Uninstall moves the link to
the Trash (below). For a link that does resolve
into the root, Canager then runs `<claude> --version` (30 s) with
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
lists what is there. The update check that follows in the same refresh
runs nothing itself: it compares the version the second look read.

Canager also asks where `claude` would run from if typed in Terminal (the
first regular file named `claude` with executable bits in Canager's
`PATH`, and where it resolves). When that is this copy there is no
notice. When it is another file, Canager looks on down `PATH` the same
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
copy. Both looks are reads (`stat`, `realpath`; listed under Files
Canager reads); that is a notice, not a command.

**Environment Canager adds to version reads** (`CLAUDE.version.env`;
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
| Uninstall | none: Canager moves up to three paths to the Trash itself (below) | 120 s; Canager stops between items once it is spent | No |

Canager adds no environment override to `claude update`; the runner
inherits the app's ambient environment. `DISABLE_AUTOUPDATER=1` stops the
background check, and manual updates still work with it set. Immediately
before starting it, Canager looks at `~/.local/bin/claude` once more, the
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
not read, so Canager assumes nothing about what a run stopped partway
leaves behind, and its preview promises nothing. Cancel: allowed
(`KillThenReconcile`) — the runner stops the process group, Canager reads
`<claude> --version` again, and the operation is reported as unconfirmed
regardless of that reading (the same rule as every stopped upgrade). If
it exits 0 but afterwards the launcher is dangling or its version cannot
be read, verification fails and the outcome is also unconfirmed. If it
exits 0 and the version did not move (Claude Code already updated itself,
or reports "up to date"), the operation is reported as needing attention
whenever a version before it could be read, as for every source. When
none could (`--version` did not answer just before the update), there is
nothing to compare, and an update that exits 0 is reported as a success
if a version can be read afterwards — even when `claude update` found
nothing to install. There is no install: the installer is Anthropic's,
not Canager's.

**Uninstall.** Claude Code has no uninstall command. Anthropic's own
instructions ("Uninstall Claude Code → Native" on
code.claude.com/docs/en/setup) are two `rm` commands; Canager runs
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
Canager runs as; it must be what the list describes — the
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
there but that Canager cannot confirm is the tool's — the wrong kind of
thing, a link elsewhere, a folder on the way that is a link — stays,
and the preview lists it among what is kept. Not yours, or would take a
kept path along, refuses whether the path is optional or not. The
preview also records what each path is — its
device, inode and kind, from `lstat` — and Canager keeps that with the
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
last, is moved, Canager also looks for every other listed path once
more: one that is there again — the program files recreated during a
pause by a Claude Code still running, say — stops the uninstall before
the launcher (`Fault::PathChanged`, naming that path), so the row stays
and a fresh preview lists what came back. Canager checks each item
immediately before moving it; a program running as you that swaps the
item in that instant could still race it. The launcher is last, so a
stop partway — macOS refusing an item (its own words are shown), Cancel,
or Canager stopping between items once the 120 s budget is spent (a move
already under way is always finished first) — always leaves it: a stop
before the first move changes nothing, and the row stays as it was; once
the program files are in the Trash, the next refresh shows the
launcher-only row, and its Uninstall lists them as already gone and
moves the rest. Afterwards Canager looks for the launcher again
(`reconcile_after_uninstall`): the uninstall is reported as succeeded
only when it is gone, and as unconfirmed when Canager cannot tell (a
folder it may not read, say).

## Antigravity CLI

Adapter: `StandaloneAdapter` over the `AGY` recipe in
`crates/canager-core/src/adapters/standalone/` (`recipes.rs` is the data,
`mod.rs` the behaviour, `route.rs` the recognition, `removal.rs` the
uninstall). Verified against Antigravity CLI 1.2.11 (the version in
`adapters/meta/standalone-agy.toml` and the name of the recorded fixture
directory). The row is one tool, installed by Google's own installer
(`curl -fsSL https://antigravity.google/cli/install.sh | bash`, run by the
user — Canager never runs it), and the one item under it is the tool
itself.

**Detect.** Canager looks at the fixed path the installer writes,
`~/.local/bin/agy` — never an `agy` found through `PATH` — and checks with
`lstat` and `realpath` that it is a regular file: the installer copies the
binary there, and a link of that name is somebody else's (the Homebrew
cask's `agy` is a link into its Caskroom, and is Homebrew's row). There is
no launcher-only state: the file *is* the program. Canager then runs
`<agy> --version` (30 s) with `AGY_CLI_DISABLE_AUTO_UPDATE=true` in its
environment, the switch Google documents for its background updater. On
the recorded version (1.2.11, 2026-09-26), `--version` alone did not reach
the updater at all — no new log file under
`~/.gemini/antigravity-cli/log`, `updater/update_status.json` untouched, no
updater process, checked around the very read the fixture records — so
the switch is a belt on top of that; a run with a prompt is what writes a
log and starts the updater. The version is the first token of the first
non-empty line (`1.2.11`).

Canager also asks where `agy` would run from if typed in Terminal, as it
does for Claude Code, and says so under the source. That is a notice, not
a command.

**Environment Canager adds to version reads** (`AGY.version.env`):

    AGY_CLI_DISABLE_AUTO_UPDATE=true

**Read-only commands and requests** (background checks; never need a
password):

| Purpose | Argv or request | Timeout |
|---|---|---|
| Detect, and inventory (whose reading the update check compares) | `<agy> --version`, with `AGY_CLI_DISABLE_AUTO_UPDATE=true` | 30 s |
| Newest published version (`check_updates`), on Apple silicon only | `GET https://antigravity-cli-auto-updater-974169037036.us-central1.run.app/manifests/darwin_arm64.json` — the manifest the installer and the updater read; its top-level `version` | 30 s |

On an Intel Mac, or when Canager itself runs under Rosetta (it then
reports `x86_64`), no request is made and the row says the check is not
yet verified there: only the Apple-silicon manifest has been fetched. An
update is listed only when the manifest's version is greater than the
installed one, comparing dot-separated integers; a failed request, a
non-200 answer or a body that is not such a manifest is "could not check",
never an error for the source.

**Write commands**: none. Antigravity CLI installs its updates itself in
the background (at most every 15 minutes, by Google's documentation and
this Mac's own log), and its `agy update` subcommand is undocumented, has
no options and has never been run — so Canager offers no Update button: a
newer version is listed with the badge "Updates itself" and a sentence
that says to open the tool once and quit it. `Session::issue_plan` refuses
the upgrade as well, and so does the adapter.

**Uninstall** (only after the user reviews and confirms a preview; no
command runs): Canager moves to the Trash, in this order, any backup copy
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
Canager never moves anything out of), and `~/.zshrc` and `~/.zprofile`,
where the installer adds its `PATH` line (Canager never edits a startup
file, and does not read these to find the line). The whole uninstall has
120 s, as Claude Code's does. There is no vendor uninstall document; the
list is the installer script's own path plus the cask's `zap`, and the
fixture README says so.

## Grok Build

Adapter: `StandaloneAdapter` over the `GROK` recipe in
`crates/canager-core/src/adapters/standalone/`. Verified against Grok
Build 1.0.41 (the version in `adapters/meta/standalone-grok.toml` and the
name of the recorded fixture directory). The row is one tool, installed by
xAI's own installer (`curl -fsSL https://x.ai/cli/install.sh | bash`, run
by the user — Canager never runs it), and the one item under it is the
tool itself.

**Detect.** Canager looks at the fixed path the installer writes,
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
Uninstall removes what is left. For a link that resolves, Canager runs
`<grok> --version` (30 s) with no added environment (none is documented).
Whether `--version` runs grok's launch-time updater, and whether that
updater installs or only checks, are both unverified; on the recorded
version (1.0.41, 2026-09-26) `--version` left `~/.grok/bin`,
`~/.grok/downloads` and `~/.grok/version.json`'s timestamp unchanged and
wrote nothing under `~/.grok`, as the fixture README records around the
very read it holds. The version is the second token of the first
non-empty line (`grok 1.0.41 (4220f3b224a6)`).

Canager also asks where `grok` would run from if typed in Terminal and
says so under the source, as it does for Claude Code. The `grok` of the
formula above, and that of npm's package `grok-cli` (a third-party
wrapper), are not Grok Build. Canager tells where a `grok` resolves — a
Homebrew directory, an npm one or anywhere else — not which program it
is, so the notice calls a `grok` that comes first another program with
that name, which may or may not be Grok Build, and never another copy.
That is a notice, not a command.

**Read-only commands** (background checks that install nothing and never
need a password; grok's own check writes inside `~/.grok`, below):

| Purpose | Argv | Timeout |
|---|---|---|
| Detect, inventory (the version the update check lists as current), and the reading before and after an update | `<grok> --version` | 30 s |
| Newest published version (`check_updates`) | `<grok> update --check --json` — grok's own check; its `--help` describes `--check` as "Check for updates without installing" | 60 s |

Grok's own check prints one JSON object; Canager believes its
`updateAvailable` and shows its `latestVersion`, comparing nothing itself
(the channel is the tool's own, "Native"). A check that exits non-zero,
prints something that is not that JSON, does not finish in 60 seconds, or
answers with a non-null `error` field (grok could not find out — say,
offline) is "could not check" with a short reason, never "up to date" and
never an error for the source. The reason quotes grok's `error` text when
it gave one, and says so when Canager could not run the check, when it did
not finish in 60 seconds, or when it did not print that JSON; any other
end than exit code 0 is worded as every other lookup that runs a command
words it: the first line of grok's stderr or, when there is none, how the
check ended (that `grok update --check --json` exited with code 1, say).
Canager makes no network request of its own for grok; the check's
connection is grok's, under grok's
configuration (`~/.grok/config.toml`, which Canager does not read). The
check writes inside `~/.grok` each time it runs, so every refresh causes
those writes — grok's, not Canager's ("Files Canager writes"). On the
recorded run (2026-09-26) it replaced `~/.grok/version.json`, whose
`checked_at` became the time of the check; added two lines to grok's own
log, `~/.grok/logs/unified.jsonl`, recording that it loaded its saved
login (`~/.grok/auth.json`, which Canager never reads); and touched the 27
files of the user guide grok ships, `~/.grok/docs/user-guide` (their
modification times moved; no file was added or removed). Whether grok
installs updates on its own (`auto_update = true` means "check for updates
on launch") is unverified, so the row is not described as self-updating.

**Write commands** (only run after the user reviews and confirms a plan
preview):

| Purpose | Argv | Timeout | Needs a password |
|---|---|---|---|
| Upgrade | `<grok> update` | 1800 s | No |
| Uninstall | none: Canager moves up to eight paths to the Trash itself (below) | 120 s; Canager stops between items once it is spent | No |

`grok update` downloads the new version into `~/.grok/downloads` and
re-points the `bin/` links, leaving the old download in place (the
installer's layout; the update's own steps were not read). Immediately
before starting it, Canager looks at `~/.grok/bin/grok` once more, the way
Detect does (no command runs): it must still be one link straight into
`~/.grok` that resolves there; if it has gone, dangles, is a plain file or
now points elsewhere, the update is not started, and the operation reports
the launcher as changed since the preview. Cancel: allowed
(`KillThenReconcile`) — the runner stops the process group, Canager reads
`<grok> --version` again, and the operation is reported as unconfirmed
regardless of that reading. An update that exits 0 with the version
unchanged is reported as needing attention, as for every source. **How
`grok update` behaves when nothing can answer a prompt (Canager gives it
no terminal and a closed stdin) has not been observed by this project**;
the author records it on a CI runner before this step merges, and this
paragraph then says what was seen.

**Uninstall** (only after the user reviews and confirms a preview; no
command runs): Canager moves to the Trash, in this order,
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
very file `~/.grok/bin/grok` runs. An optional path Canager cannot
confirm is grok's own — a `~/.local/bin/agent` that belongs to another
program, say, or a link of yours to a plugin's or a skill's program
inside `~/.grok` — stays and the preview says so. The launcher is last: once
`~/.grok/downloads` is in the Trash, a run that stops leaves a
launcher-only row that a second Uninstall finishes, as for Claude Code.
The whole uninstall has 120 s, as Claude Code's does. It keeps `~/.grok`
itself — `config.toml`, `auth.json` (the login), `sessions/`, `memory/`,
`skills/`, `plugins/` — and `~/.zshrc`, where the installer wrote its
marked block. A `/usr/local/bin/grok` or `/usr/local/bin/agent` is outside
your home folder, so Canager never touches it: when it is a link into
`~/.grok` that leads nowhere once the paths above are in the Trash — the
installer's fallback, to grok's download or through `~/.grok/bin/grok`,
or one that leads nowhere already — the preview says it becomes a dead
link; when it is something else (Homebrew's `grok-build` link on an Intel
Mac, another program's `agent`, or a link to a plugin's program in the
`~/.grok` this uninstall keeps, which still works afterwards), the preview
says nothing about it. There is no vendor uninstall document and no
`grok uninstall`; the list is grok's own README ("File Locations") plus
its install script, and the fixture README says so.

## rustup

Adapter: `StandaloneAdapter` over the `RUSTUP` recipe in
`crates/canager-core/src/adapters/standalone/` (`recipes.rs` is the data,
`rustup.rs` what its uninstall does, when Canager may offer it, and what to
say about it). Verified against rustup 1.29.1 (the version in
`adapters/meta/standalone-rustup.toml` and the name of the recorded fixture
directory). The Rust toolchain installer, installed by its own script
(`curl … https://sh.rustup.rs | sh`, run by the user — Canager never runs
it); the one item under it is rustup itself. The toolchains it manages, and
the programs `cargo install` installs, are not rows of this source: the
first are outside phase 4, the second are Cargo's.

**Detect.** Canager looks at the fixed path the installer writes,
`$CARGO_HOME/bin/rustup` — `CARGO_HOME` from the environment Canager was
started with (see "Which Rust" below), read the way rustup and cargo read
it: an empty value means the default `~/.cargo`, a relative value names a
folder relative to the tool's own working directory, which Canager cannot
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
earlier self update, if there is one. Canager also asks where `rustup`
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
update` and `rustup toolchain install`, which Canager never runs.

**While rustup is being updated or uninstalled, Canager does not run it.**
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
says so; the operation bar offers no Cancel; while it is still queued it
can be cancelled, since nothing has started), and it holds the Cargo
source's lock as well as its own. If it exits 0 and the version did not
move, the operation is reported as needing attention, as for every source.
A run stopped by the timeout is reported as unconfirmed, whatever the
version reads before and after say: an upgrade stopped partway is never
called done on the strength of a version number.

`rustup self uninstall -y` is rustup's official uninstall (`-y` skips its
own confirmation prompt, which would otherwise read end-of-file from the
`/dev/null` standard input and stop). **Canager offers it only when Rust
lives in its standard folders**: `CARGO_HOME` and `RUSTUP_HOME` (from the
environment Canager was started with, read as rustup reads them) resolve to
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
following, and Canager refuses at any of them rather than keep a list of
the names rustup follows. Canager will not ask it to delete a place the
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
`~/.cargo/.crates2.json` records — the same file the Cargo source reads);
that rustup will edit your shell startup files; and each startup file that
will still speak of Cargo's env file afterwards. It is not cancellable once
running, for the same reason as the update, and holds the same two locks
(it deletes the record the Cargo source's inventory reads). Afterwards
Canager looks for `~/.cargo/bin/rustup` again and reads no version: an
exit 0 with it gone is reported as succeeded, an exit 0 with it still there
as needing attention, and a run stopped by the timeout is judged by the
same look — gone is succeeded, still there is unconfirmed.
`--no-modify-path` is not passed: rustup removing its own line beats
leaving one that makes every shell reading that file print an error.

**Which Rust.** rustup runs with the environment Canager itself was
started with: at launch Canager restores only `PATH` from your login shell,
and every command it runs inherits the rest. Canager reads `CARGO_HOME`,
`RUSTUP_HOME` and `ZDOTDIR` from that same environment — the one the
rustup it runs will see, so the two always agree about which folders are
meant. A `RUSTUP_HOME` or `CARGO_HOME` exported only in a shell startup
file is therefore not seen by either: the preview and the uninstall act on
the default folders, and a Rust kept only where the shell says is left
alone, not deleted; a `CARGO_HOME` exported only there also means Canager
looks for rustup under `~/.cargo` and does not list one installed elsewhere.

**Shell startup files.** Canager never edits one. rustup's uninstall removes
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
`config.fish`. So before the uninstall Canager reads those eight files —
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
every line above it stands alone. Canager reads each of those lines with
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
Canager runs nothing and reads only the variable it was started with, so
a `ZDOTDIR` set only inside a zsh startup file is not modelled, and a zsh
whose files live under such a `ZDOTDIR` is not read.

## Unknown-source scan (phase 4, step F): read-only, no command runs

The *Unknown* page lists command-line programs that none of the sources
above installed. Producing that list runs no command at all.
`scan_unknown` (`crates/canager-core/src/scan/mod.rs`) reads directory
entries and file metadata and nothing else:

| It looks at | How |
|---|---|
| `~/.local/bin`, `~/bin`, `/usr/local/bin`, `~/.cargo/bin` (and `$CARGO_HOME/bin` when that variable is set), `~/go/bin`, `~/.bun/bin`, `~/.deno/bin`, plus every `PATH` entry under your home folder (`candidate_dirs`) | `read_dir`, one level deep — a subdirectory is never entered; a directory that does not exist, or that cannot be read, is skipped silently; two names for one directory are read once (`scan_dirs`) |
| each entry | `lstat`, `readlink`, `realpath`, `stat` (`examine`): what kind of file it is, where a link points, its size and date, who owns it. A file with no execute bit is not listed. Nothing's *contents* are read, and `file(1)` is not run. A broken link, while a source's own executable is a link that leads nowhere too, also gets `lstat`, `readlink` and `realpath` on the folders and links its text leads through, to see where it would lead (`dead_end`) |

It stops after 2000 entries or 10 seconds (`ScanBudget::default`) and
says so on the page, with the number it stopped at. It never runs, opens,
moves or deletes anything it finds. It takes no lock and is not part of a
refresh (`Session::scan_unknown` in
`crates/canager-core/src/session/scan.rs`): it runs when the page opens,
again when the sources' state changes while the page is open, and when
you press *Scan again* — always against the sources' last known state —
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
`~/.gemini/antigravity-cli`; Grok Build's `~/.grok`).
A regular file in a tool's own bin directory whose name is one of the
backup patterns that tool's recipe declares — `agy.<time>.old` in
`~/.local/bin`, the copies Antigravity's updater leaves — is that tool's
while the tool is installed (`Recipe.backup_globs`, rule 4); once the tool
is gone the pattern goes with it and such a file is listed.
Everything else is listed, with where a broken link pointed, the app a
program runs inside, and whether an installer with administrator rights
put it there.

Canager reads one path per cask, the first `app` stanza's, so two cask
shapes are still listed here although Homebrew installed them: a command
that lives neither inside that `.app` nor under `Caskroom` (one a `pkg`
put on the disk, or one inside a second `.app` of the same cask), and a
cask whose `brew info` entry carries no absolute `target` for its `app`.

## Files Canager reads

All read-only, none saved anywhere else, none uploaded:

- Homebrew: whether the three candidate `brew` paths exist;
  `<prefix>/var/homebrew/locks` and the `update` lock file in it, during
  the uninstall preview (Homebrew's section).
- npm: whether `{prefix}/lib/node_modules`, `{prefix}/lib` or `{prefix}`
  is writable, via `access(2)`.
- pip: the canonical path of each interpreter found, to count it once.
- Cargo: `<CARGO_HOME>/.crates2.json`; whether `cargo-binstall` is on
  `PATH`.
- Ollama: whether `/Applications/Ollama.app` or `~/Applications/Ollama.app`
  is a directory; `~/.ollama/models/manifests/registry.ollama.ai/{namespace}/{name}/{tag}`
  for each pulled model.
- Claude Code: whether `~/.local/bin/claude` exists and where it links to
  (`lstat`, `readlink`, `realpath`, also for the folder the link is in
  and for `~/.local/share/claude`); for the notice under the source, each
  `PATH` directory's `claude` in `PATH`'s order (`stat`; `realpath` for
  each that is a regular file with executable bits) until the first such
  file, and, when that one does not resolve to this copy, on down `PATH`
  the same way until one does or `PATH` ends;
  `~/.claude/settings.json`, for the one key `autoUpdatesChannel` (read
  and discarded; a missing file or key means `latest`).
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
  `agy.<time>.old` backup, which Canager finds among the names in
  `~/.local/bin` (the Unknown page's rule 4 goes by the same names); and
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
  nothing else — no version is read.
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
  opened only when `stat` says it is a regular file (links followed), its
  device and inode taken from the open file (`fstat`) so that two names
  of one file share one copy, read whole, and only searched for a line
  about Cargo's env file and for whether the lines above it stand alone,
  as its section says; nothing else under
  `RUSTUP_HOME` is ever read. After an uninstall: whether
  `$CARGO_HOME/bin/rustup` is still there (`lstat`, `realpath`), and
  nothing else — no version is read.
- The Unknown page's scan: the entries of the bin directories its section
  lists, one level deep, and each entry's metadata and link target — never
  a file's contents.
- Canager's own `settings.json` in its application data directory
  (`settings::load`; a missing or unreadable file means default settings).

## Files Canager writes

One: `settings.json` in Canager's application data directory
(`settings::save`, written to a `settings.json.tmp.<n>` beside it and
renamed into place, so a crash mid-write cannot leave it corrupt; the
directory is created if it is missing). Nothing else on the Mac is
written or deleted by Canager itself. It moves files in one case: a
confirmed uninstall of a tool that has no uninstall command (Claude Code,
Antigravity CLI or Grok Build) moves the paths its preview listed to the
Trash (next section). The programs Canager runs write their own files as
they run — Grok Build's own update check (`grok update --check --json`,
Grok Build's section), for one, writes inside `~/.grok` on every refresh:
on the recorded run it replaced `~/.grok/version.json` with the time of
the check, added two lines to its log and touched the user guide it
ships. Those writes are grok's, not Canager's. Every other change to what
is installed is made by the tool named in the preview, running the
command shown there.

## Moving files to the Trash

`RealTrasher` (`crates/canager-core/src/trash/real.rs`) is the only code
in Canager that changes a file on the Mac other than its own settings.
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
one gap remains: Canager checks each item immediately before moving it;
a program running as you that swaps the item in that instant could still
race it. Each move is written to the operation log with where the item
now is (`LogNote::MovedToTrash`); an item macOS refuses stops the
uninstall there, with macOS's own reason (`LogNote::TrashFailed`); and
when the time budget runs out between items, the log names the item the
uninstall stopped before and the budget it ran out of
(`LogNote::OutOfTime`). After each move Canager waits 3 seconds
(`removal::PUT_BACK_SETTLE`) before it moves anything else, and before it
reports the uninstall finished. That holds across uninstalls: up to three
operations run at once and each path-list uninstall locks only its own
tool, so two tools' uninstalls can run side by side, and their moves take
turns on one shared clock (`removal::LastMove`) — an item waits while an
item of the other uninstall is waiting or moving, then until 3 seconds
after the last move Canager made, and only then gets its last check and
its move. Cancel ends a wait, and no wait outlasts the uninstall's time
budget; time spent waiting for another uninstall's moves comes out of it
too. The second finding below says why. A debug build of Canager, never
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
  calls, and two uninstalls in Canager are one process too — hence the
  3-second pause between any two of Canager's moves, not only between one
  uninstall's: it makes Put Back likely for every item, not certain, and
  an item without the record can still be dragged back out of the Trash
  by hand. The runs that recorded every item also kept running for 3
  seconds after the last call, and the record is written after the call
  returns — with Full Disk Access, a process that quit at once lost the
  later records — so Canager waits after an uninstall's last move too,
  and quitting Canager while an uninstall is still running may leave the
  item it moved last without Put Back. Why macOS behaves this way is not
  known: the pause is a measurement on one Mac, not a documented
  guarantee.
- A plain `rename` into `~/.Trash` from the same process succeeded too
  (24 runs out of 24), where the design had expected it to be refused:
  the Trash's protection covers listing it, not adding to it, so a `mv`
  could have reached it. Canager does not use one anyway: a renamed item
  gets no Put Back record, and one `mv` of Claude Code's two paths named
  `claude` collides on the name — `mv -n` skips the second and still
  reports success.

Not verified by that app: a click on Put Back itself (the records were
checked, not used), a build of Canager itself, a symbolic link whose
target is gone — which is what every Claude Code uninstall moves last:
the launcher, after the program files it points to — other macOS
versions, and Intel Macs.

`crates/canager-core/tests/standalone_uninstall_test.rs` has an
`#[ignore]`d test that makes five throwaway items — a file, a folder, a
link to each, and a link to nothing — moves them with the real call, and
checks that each lands in `~/.Trash` as itself; CI runs it. It runs from
a terminal or a CI runner, not from a Finder-launched app without Full
Disk Access, so it checks the move, not Put Back.

## Network: Canager only connects to these hosts

Every request goes through `RealHttpClient`
(`crates/canager-core/src/http/real.rs`), and it refuses, before opening a
connection, any `https` request whose host is not on this list
(`ALLOWED_HTTPS_HOSTS`, checked by `host_allowed` at the top of `send`):

| Host | What is fetched | By |
|---|---|---|
| `crates.io` | `GET /api/v1/crates/{name}` — the newest stable version of one crate | Cargo's `check_updates` |
| `pypi.org` | `GET /pypi/{name}/json` — the newest version of one package | pipx's `check_updates`, on pipx < 1.16 only |
| `registry.ollama.ai` | `GET /v2/{namespace}/{name}/manifests/{tag}` — one model's manifest | Ollama's `check_updates` |
| `downloads.claude.ai` | `GET /claude-code-releases/latest` or `/stable` — the newest published Claude Code version on that channel, answered as one bare version number | Claude Code's `check_updates` (`StandaloneAdapter`) |
| `static.rust-lang.org` | `GET /rustup/release-stable.toml` — the newest published rustup version, a two-line TOML file (`version = '…'`) | rustup's `check_updates` (`StandaloneAdapter`) |
| `antigravity-cli-auto-updater-974169037036.us-central1.run.app` | `GET /manifests/darwin_arm64.json` — the newest published Antigravity CLI version for Apple silicon, as the JSON manifest its installer and its updater read (`version`, `url`, `sha512`; only `version` is used) | Antigravity CLI's `check_updates` (`StandaloneAdapter`), only when Canager itself runs on Apple silicon — on an Intel Mac no request is made and the row says the check is not yet verified there |

Plain `http` is exempt from the list for one caller: the Ollama daemon at
`OLLAMA_HOST` or `http://127.0.0.1:11434` (`GET /api/tags`), which may be
a machine the user named. The exemption is by scheme, not by caller: an
`https://` `OLLAMA_HOST` is refused like any other https host that is not
in the table, before any connection, and that Ollama is shown as a daemon
that did not answer (its section says exactly how). Recorded in
`docs/superpowers/backlog.md`.

Every request: TLS through rustls; the header `User-Agent:
canager/<version>`; no other header of Canager's own, except `Accept` on
the Ollama registry request — the HTTP library adds what the protocol
needs, `Host` and `Accept: */*`, and nothing else; no cookies, no
credentials, nothing about this Mac in the request; a timeout per request
(listed in each source's table: 30 s unless stated, and the daemon check
in Ollama's detect is 10 s); a response body limit of 8 MiB
(`MAX_RESPONSE_BYTES`); and no redirect is ever followed — a 3xx is an
error. Nothing is ever sent by any method but `GET`.

Three things are outside that client and worth saying out loud. The
window itself cannot make a network request: its content security policy
is `connect-src 'self'` (`src-tauri/tauri.conf.json`). The Tauri opener
plugin — the one that opens a URL or a path in another application — is
registered (`run()` in `src-tauri/src/lib.rs`) and the main window is
permitted to call it (`opener:default` in
`src-tauri/capabilities/default.json`), but nothing in the front end
calls it: no homepage link, no "reveal in Finder"; when one ships, this
paragraph changes. And the Tauri updater
plugin is compiled in and configured with the endpoint
`https://github.com/Brulek/Canager/releases/latest/download/latest.json`
(`src-tauri/tauri.conf.json`, `plugins.updater`), but nothing in Canager
calls it yet, so no request to it is made; when app self-update ships,
this paragraph changes.

The tools Canager runs make their own connections — `brew`, `npm`, `pip`,
`pipx`, `uv`, `cargo`, `cargo-binstall`, `ollama pull`, `claude update`,
`rustup self update`, `grok update --check --json` and `grok update` each
reach whatever index, registry or release server they are configured to
use. Those are the tools' connections, under the tools' configuration;
Canager neither chooses nor sees them.

## What Canager never does

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
  Canager offers that only when the preview can name exactly those two.
- Never runs `agy update` (undocumented, never observed), and never runs
  `grok update` from a refresh: the refresh runs `grok update --check
  --json`, which grok's own help describes as checking without
  installing; `grok update` runs only after a confirmed preview.
- Never passes `--zap`, `--force` or `--ignore-dependencies` to Homebrew
  (the brew plan test), and never runs a bare `brew upgrade`.
- Never runs a `brew` command as root.
- Never runs a write command from a refresh, and never runs one without a
  preview the user confirmed within the last ten minutes.
- Never launches an application from a refresh; `open -a Ollama` runs
  only when the button is pressed.
- Never opens a tool to make it update itself: a self-updating tool's row
  tells the user how, and Canager runs nothing.
- Never asks for, stores or types a password; `SUDO_ASKPASS` is passed
  through to Homebrew only when it was already set.
- Never deletes a file and never empties the Trash. Never writes a file
  on the Mac itself other than its own `settings.json` (the programs it
  runs write their own files — Grok Build's update check writes inside
  `~/.grok` on every refresh, as its section says), and moves files
  only to the Trash, only for an uninstall the user confirmed, and only
  the paths its preview listed; never edits a shell startup file — rustup's
  own uninstall edits its startup line and deletes its two folders
  permanently, and the preview says so.
- Never moves anything outside the home folder, anything directly in the
  home folder or in a folder many tools share there (`~/.local`,
  `~/.config`, `~/.cache`, `~/Library`, `~/.cargo`), anything reached
  through a folder that is a link, anything that does not belong to the
  user, or anything that is not what the tool's uninstall list describes
  (for Claude Code, Anthropic's removal steps plus its installer's
  download cache; for Antigravity CLI and Grok Build, which publish no
  removal steps, Canager's own reading of how each was installed);
  never moves the settings, login and history Claude Code keeps
  in `~/.claude` (of that folder only its download cache,
  `~/.claude/downloads`) or `~/.claude.json`, the login, sessions, memory
  and settings Grok Build keeps in `~/.grok` (of that folder only
  `downloads/`, `bundled/`, `completions/` and the two links in `bin/`),
  or anything in Antigravity CLI's `~/.gemini/antigravity-cli` — nor
  `~/.gemini` itself, which Gemini CLI shares — nor anything they lead to.
- Never connects to an `https` host that is not on the list above, and
  never follows a redirect.
- Never reports an operation as succeeded on the tool's exit code alone:
  the inventory is re-read afterwards, and a package still present after
  an uninstall, one missing after an install, or an upgraded version that
  did not move is reported as needing attention — the last whenever a
  version before could be read; when Homebrew's index was updating and
  the reading before was refused, presence afterwards is all there is to
  go on (Homebrew's section).
