# What Canager Runs

Every command Canager runs, every file it reads or writes, every host it
connects to and every environment variable it sets, for the seven sources
it manages today: Homebrew, npm, pipx, uv, pip (read-only), Cargo and
Ollama. Each sentence describes what the code does now and names the
function it describes, so it can be checked against
`crates/canager-core/src/adapters/` rather than believed.
`crates/canager-core/tests/what_we_run_test.rs` checks the parts a test
can: a section per registered source, every host on the https allowlist,
every environment variable Homebrew's and npm's commands are given, the
three Homebrew flags this file promises are never passed, and that the
unknown-source scan's section states the two limits the code enforces.

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
`CARGO_HOME` and `OLLAMA_HOST` from Canager's environment and the
effective user id from the process. Every source
but Homebrew finds its executable with `resolve_exe`: the first directory
on that `PATH` containing a regular file of that name. Homebrew is looked
for at three fixed paths instead (its section). The path that was found
is the one previewed and the one run.

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
no refresh runs a write command, launches an application or asks for a
password.

**An operation** is previewed first: `plan` builds the exact argv and the
front end shows it (`plan_operation` in `src-tauri/src/ipc.rs`; the front
end never builds an argv and sends back only the id of a plan Rust
issued). The plan can be confirmed for ten minutes (`PLAN_LIFETIME` in
`crates/canager-core/src/session/plans.rs`), after which it has to be
previewed again. Before a plan is built, `Session::issue_plan` refuses an
operation on a source that is read-only or not answering, and an upgrade
or uninstall the tool itself reports it will refuse (a pinned package) —
the buttons the pages hide are backed by that refusal, not only by the
page. On confirmation `run_operation` (`crates/canager-core/src/ops/mod.rs`)
takes the plan's locks, runs the command, and then re-reads the inventory
to check what actually happened; an upgrade is also preceded by a reading,
so the version before can be compared with the version after. An install
after which the package is not present, an uninstall after which it still
is, and an upgrade that exits 0 with the version unchanged are all
reported as needing attention, never as success. The one case with less
to go on: when the reading before an upgrade was refused — on Homebrew,
while a `brew update` a refresh left running is still going (Homebrew's
section) — there is nothing to compare, and an upgrade that exits 0 is
reported as a success whenever the package is still present afterwards,
whether or not its version moved. A command that was
cancelled or timed out is reported as unconfirmed unless the reading after
settles it (`run_plan` in `crates/canager-core/src/adapters/mod.rs`, then
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
(the `Unknown` arm of `run_operation`). This is the one way an exit-0
upgrade whose version did not move is not reported as needing attention.

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
remembers the path found for plans. The Cargo home is `CARGO_HOME` from
the environment, else `~/.cargo`. No environment variables are added to
any cargo command.

**Read-only reads.** `inventory` runs no command: it reads
`<CARGO_HOME>/.crates2.json`, the file `cargo install` keeps its records
in (a missing file means nothing is installed). `check_updates` reads the
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

## Unknown-source scan (phase 4, step F): read-only, no command runs

The *Unknown* page lists command-line programs that none of the sources
above installed. Producing that list runs no command at all.
`scan_unknown` (`crates/canager-core/src/scan/mod.rs`) reads directory
entries and file metadata and nothing else:

| It looks at | How |
|---|---|
| `~/.local/bin`, `~/bin`, `/usr/local/bin`, `~/.cargo/bin` (and `$CARGO_HOME/bin` when that variable is set), `~/go/bin`, `~/.bun/bin`, `~/.deno/bin`, plus every `PATH` entry under your home folder (`candidate_dirs`) | `read_dir`, one level deep — a subdirectory is never entered; a directory that does not exist, or that cannot be read, is skipped silently; two names for one directory are read once (`scan_dirs`) |
| each entry | `lstat`, `readlink`, `realpath`, `stat` (`examine`): what kind of file it is, where a link points, its size and date, who owns it. A file with no execute bit is not listed. Nothing's *contents* are read, and `file(1)` is not run |

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
resolve to `rustup`); it resolves under a path a source reported
installing (a file or a directory: a uv or pipx tool's shim resolves into
that tool's environment, and a Homebrew cask's command in `<prefix>/bin`
— `code`, `docker` — resolves into the `.app` the cask moved into
`/Applications`, which `brew info --installed --json=v2` names beside the
cask's `app` stanza); or it resolves under a directory a source owns
(`owned_roots`: Homebrew's `Cellar`, `Caskroom` and `opt`; npm's
`lib/node_modules` under its global prefix; Ollama's `~/.ollama`).
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
written, moved or deleted by Canager itself: every change to what is
installed is made by the tool named in the preview, running the command
shown there.

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
`pipx`, `uv`, `cargo`, `cargo-binstall` and `ollama pull` each reach
whatever index or registry they are configured to use. Those are the
tools' connections, under the tools' configuration; Canager neither
chooses nor sees them.

## What Canager never does

- Never runs a shell for any command, and never pipes a download into one
  (`curl … | sh`). The one shell run is the `PATH` read at launch, above.
- Never runs an installer script, and never reruns one to update a tool.
- Never passes `--zap`, `--force` or `--ignore-dependencies` to Homebrew
  (the brew plan test), and never runs a bare `brew upgrade`.
- Never runs a `brew` command as root.
- Never runs a write command from a refresh, and never runs one without a
  preview the user confirmed within the last ten minutes.
- Never launches an application from a refresh; `open -a Ollama` runs
  only when the button is pressed.
- Never asks for, stores or types a password; `SUDO_ASKPASS` is passed
  through to Homebrew only when it was already set.
- Never writes, moves or deletes a file on the Mac itself, other than its
  own `settings.json`; never edits a shell startup file.
- Never connects to an `https` host that is not on the list above, and
  never follows a redirect.
- Never reports an operation as succeeded on the tool's exit code alone:
  the inventory is re-read afterwards, and a package still present after
  an uninstall, one missing after an install, or an upgraded version that
  did not move is reported as needing attention — the last whenever a
  version before could be read; when Homebrew's index was updating and
  the reading before was refused, presence afterwards is all there is to
  go on (Homebrew's section).
