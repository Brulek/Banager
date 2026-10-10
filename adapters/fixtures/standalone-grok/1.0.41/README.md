# Grok Build 1.0.41 fixtures (native installer route, `standalone-grok`)

Recorded 2026-09-26 on the author's MacBook (macOS 27.0, arm64) by running the
commands below. `version.txt` and `update-check.json` are the commands' output
byte for byte. `layout.txt` is `ls -lan` (numeric owner and group) with the home
folder's absolute path replaced by `~` (`sed "s|$HOME|~|g"`) — the only two
edits. The recording installed nothing and changed no link and no download;
grok's own check wrote inside `~/.grok` (below).

Commands (both grok runs with standard input `/dev/null`, as Banager's runner
gives it):
- `~/.grok/bin/grok --version` -> `version.txt` (`grok <version> (<hash>)`; the
  second token is the version; no environment variable — none is documented)
- `~/.grok/bin/grok update --check --json` -> `update-check.json` (grok's own
  read-only check: its `--help` line for `--check` reads "Check for updates
  without installing" — read during the phase 4 research, grok.md §4, and not
  run again for this recording; one JSON object whose `updateAvailable`
  Banager believes and whose `latestVersion` it shows)
- `ls -lan ~/.grok/bin ~/.grok/downloads | sed "s|$HOME|~|g"` -> `layout.txt`
  (`bin/grok` and `bin/agent` are relative links,
  `../downloads/grok-1.0.41-macos-aarch64`, into the root; 3 downloads were
  present: the linked one and two older ones)

The version read did not change the install (spec §3.4, checked around the one
`--version` run of this recording, whose output is `version.txt`): `ls -lan` of
`~/.grok/bin` and `~/.grok/downloads` and `readlink ~/.grok/bin/grok` were
unchanged ten seconds after `--version`, and unchanged ten seconds after the
check except for the two `..` lines — `~/.grok` itself, whose timestamp moved
when the check replaced `version.json`. `~/.grok/version.json` (which records
`checked_at`) had mtime 1790383762 before, 1790383762 after `--version`, and
1790388144 after the check. Nothing under `~/.grok` was modified between the
start of `--version` and the start of the check (`find ~/.grok -newer`, run
afterwards): the version read wrote nothing there. Whether `--version` alone
runs grok's launch-time updater is UNVERIFIED, and whether that updater
installs or only checks is UNVERIFIED too (grok.md §5, open question 2): this
read left no trace of it, and the unchanged layout is the whole of what this
recording can say about installing.

What the check wrote, all inside `~/.grok` and all grok's own: it replaced
`version.json`, whose `checked_at` became the time of the check; it added two
lines to its log, `~/.grok/logs/unified.jsonl`, which by their own words are
about loading its saved login, `~/.grok/auth.json`; and the modification times
of the 27 files of the user guide it ships, `~/.grok/docs/user-guide/*.md`,
moved to the moment it started (the files were not recreated). Banager runs
this check on every refresh, so every refresh causes these writes;
`docs/what-we-run.md` says so.

The recording ran no `grok update` without `--check`, no bare `grok` and no
`grok update --help`. How `grok update` behaves with its input closed has not
been observed by this project; the author records it on a CI runner before this
step merges (docs/superpowers/plans/2026-09-25-phase-4-step-d-grok-agy.md, "The
author's pre-merge verification").

Optional paths at recording time: present `~/.grok/bundled`,
`~/.grok/completions` and `~/.config/fish/completions/grok.fish`; absent
`~/.local/bin/grok`, `~/.local/bin/agent`, `/usr/local/bin/grok` and
`/usr/local/bin/agent`. The Homebrew cask `grok-build` is not installed on this
Mac (neither `/opt/homebrew/Caskroom/grok-build` nor `/opt/homebrew/bin/grok`
exists), so that route is not recorded. `~/.zshrc` carries the installer's
marked block (2 marker lines).

## Uninstall list

Nothing here was recorded for the uninstall: Banager runs no command for it.
The list in `crates/banager-core/src/adapters/standalone/recipes.rs`
(`GROK.uninstall`) is not a vendor document — xAI publishes none and there is
no `grok uninstall` (grok.md §6). It is the README grok ships ("File
Locations") plus its install script: Banager moves the two optional fallback
links the installer makes when `~/.grok/bin` is not on PATH (first: their link
text is unverified, so they go while every folder it could pass through is
still there — a precaution), `~/.grok/downloads`, `~/.grok/bundled` and
`~/.grok/completions` (optional), the fish completion the installer also
writes (optional), then the two links the installer put in `~/.grok/bin` —
`agent`, and `grok` last. The folder `~/.grok/bin` itself is not moved: the
installer put it on `PATH`, so it may hold the user's own scripts, and it stays,
emptied, inside `~/.grok`. It keeps `~/.grok` (`config.toml`, `auth.json`,
`sessions/`, `memory/`, `skills/`, `plugins/`: the de-facto `rm -rf ~/.grok`
would take the login, sessions and memory, which spec Q4 keeps) and `~/.zshrc`,
and reports — never touches — a `/usr/local/bin/grok` or `/usr/local/bin/agent`
when it is a link into `~/.grok` (never Homebrew's link or another program's
file), since that one becomes a dead link.
