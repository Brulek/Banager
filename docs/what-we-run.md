# What Canager Runs (Phase 0–1: Homebrew only)

Canager never invokes a shell. Every command below is a fixed argv array
run directly against the resolved Homebrew binary (one of
`BrewAdapter::CANDIDATE_PATHS`). The only user-controlled input in any of
these commands is a single validated argument — a formula/cask name or a
search query, checked by `validate_package_name` against
`^[A-Za-z0-9@._+/-]+$`, and never allowed to start with `-`.

## Environment applied to every invocation

    HOMEBREW_NO_AUTO_UPDATE=1
    HOMEBREW_NO_ENV_HINTS=1
    HOMEBREW_NO_INSTALL_CLEANUP=1
    NO_COLOR=1

Install and upgrade (formula and cask) pass through `SUDO_ASKPASS` when it
is already set in Canager's process environment; it only has any effect
for casks whose installer scripts invoke `sudo` — Canager never sets it on
its own behalf.

Canager refuses to run any `brew` command at all when the current
process's effective user ID is 0 (root).

## Read-only commands (background checks; never require a password)

| Purpose | Argv | Timeout |
|---|---|---|
| Detect a Homebrew install | `<brew> --version` | 30 s |
| List installed formulae + casks | `<brew> info --installed --json=v2` | 120 s |
| Refresh Homebrew's local package index (TTL: 6 hours) | `<brew> update` | 120 s |
| List outdated formulae + casks | `<brew> outdated --json=v2` | 120 s |
| Search by name | `<brew> search {query}` | 30 s |
| Search by name + description | `<brew> search --desc {query}` | 30 s |
| List installed formulae depending on a formula (uninstall-safety check) | `<brew> uses --installed {name}` | 120 s |

## Write commands (only run after the user reviews and confirms a plan preview)

| Purpose | Argv | Timeout | Needs a password |
|---|---|---|---|
| Install a formula | `<brew> install --formula {name}` | 1800 s | No |
| Install a cask | `<brew> install --cask {name}` | 1800 s | Sometimes — some cask installers invoke `sudo`; `SUDO_ASKPASS` is passed through when set |
| Uninstall a formula | `<brew> uninstall --formula {name}` | 1800 s | No |
| Uninstall a cask | `<brew> uninstall --cask {name}` | 1800 s | Sometimes — some cask uninstalls invoke `sudo` (e.g. removing a `pkgutil` receipt, a launch daemon, or a kernel extension) |
| Upgrade one formula | `<brew> upgrade --formula {name}` | 1800 s | No |
| Upgrade one cask | `<brew> upgrade --cask {name}` | 1800 s | Sometimes — some cask installers invoke `sudo`; `SUDO_ASKPASS` is passed through when set |

Every write command is followed by one run of `<brew> info --installed
--json=v2` to check what it actually did. An upgrade is usually also
preceded by one, so the version installed before can be compared with the
version installed after (`run_operation` in
`crates/canager-core/src/ops/mod.rs`): an upgrade that exits 0 and leaves
the version where it was is reported as needing attention, not as a
success.

The exception is an upgrade started while a `brew update` that a refresh
began is still running. The reading before is then refused at once, with no
`brew info` run (`IndexUpdating`, from `BrewAdapter::inventory`), so there is
nothing to compare: an upgrade that exits 0 is reported as a success
whenever the package is still installed afterwards, whether or not its
version moved. The upgrade command itself still waits for that `brew update`
to finish first (`BrewAdapter::execute`).

An upgrade that was cancelled or timed out partway is reported as
unconfirmed, whatever either reading says. Stopped partway, Homebrew can
read as the new version before the upgrade has finished (a formula's new
keg is in place before it is linked), or as the old version after it has
already moved a cask's old app out of /Applications. The comment on the
stopped-command arm of `run_operation` gives Homebrew's lines.

Canager never passes `--ignore-dependencies` to `brew uninstall`, and never
runs a bare `brew upgrade` — upgrades are always one invocation per
confirmed artifact, never "upgrade everything" in a single command.

`<brew>` above is always the absolute path `BrewAdapter::detect` found on
disk (one of `/opt/homebrew/bin/brew`, `/usr/local/bin/brew`,
`/home/linuxbrew/.linuxbrew/bin/brew`), never a bare `brew` resolved
through a shell `PATH` lookup.
