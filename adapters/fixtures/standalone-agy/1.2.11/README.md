# Antigravity CLI 1.2.11 fixtures (native installer route, `standalone-agy`)

Recorded 2026-09-26 on the author's MacBook (macOS 27.0, arm64) by running the
commands below. `version.txt`, `manifest-darwin_arm64.json` and
`update_status.json` are the commands' output byte for byte. `layout.txt` is
`ls -lan` (numeric owner and group) with the home folder's absolute path
replaced by `~` (`sed "s|$HOME|~|g"`) — the only two edits, so no user name or
home path enters the repository. The recording wrote nothing into the tool's
folders (below).

Commands:
- `AGY_CLI_DISABLE_AUTO_UPDATE=true ~/.local/bin/agy --version < /dev/null`
  -> `version.txt` (one bare version and a newline; the switch is the one
  Google's troubleshooting page documents for the background updater, and
  standard input is `/dev/null`, as Canager's runner gives it)
- `curl --fail --silent --show-error https://antigravity-cli-auto-updater-974169037036.us-central1.run.app/manifests/darwin_arm64.json`
  -> `manifest-darwin_arm64.json` (a direct 200, `application/json`, no
  redirect; the manifest the installer and the updater read; only its
  `version` is used by Canager)
- `cat ~/.gemini/antigravity-cli/updater/update_status.json` -> `update_status.json`
- `ls -lan ~/.local/bin/agy | sed "s|$HOME|~|g"` -> `layout.txt` (a regular
  file, not a link: the installer copies the binary there). Its date,
  2026-09-25, says the launcher was replaced after the phase 4 spec read
  1.2.10 from it on this Mac.

The read did not reach the updater (spec §3.4, checked around the one
`--version` run of this recording, whose output is `version.txt`):
`~/.gemini/antigravity-cli/log` held 2666 files before and 2666 after;
`updater/update_status.json`'s mtime was 1790387335 before and 1790387335
after (09:48:55 local time, twelve minutes before the recording); no `agy` or
`antigravity` process was running before the read, and none had appeared ten
seconds after it (a before/after diff of `ps -axo pid,ppid,comm`). Nothing under
`~/.gemini/antigravity-cli` had been modified since the recording began when
`find -newer` looked a few minutes later. Since `--version` does not reach the
updater either way, this recording cannot show what the switch itself does; it
stays on the read as Google's documented belt. A run with a prompt is what
writes a log and spawns the updater (agy.md §4).

The manifest answered `version` 1.2.11, the version the launcher printed: this
Mac was current on the recording day. The recording ran no `agy update` and no
bare `agy`.

Other observations, for the reader (not recorded as files): 0 `agy.<time>.old`
backups in `~/.local/bin` at recording time (the updater's transient leftover,
spec §3.5); `~/.cache/antigravity/staging` existed and held 0 entries; `~/.zshrc`
and `~/.zprofile` each carry the `# Added by Antigravity CLI installer` marker
(1 each). No Homebrew `antigravity-cli` cask is installed on this Mac (neither
`/opt/homebrew/Caskroom/antigravity-cli` nor `/opt/homebrew/bin/agy` exists), so
the cask route is not recorded; the shared-exclusion and PATH cases use
synthetic unit tests.

## Uninstall list

Nothing here was recorded for the uninstall: Canager runs no command for it.
The list in `crates/canager-core/src/adapters/standalone/recipes.rs`
(`AGY.uninstall`, `AGY.backup_globs`) is not a vendor document — Google
publishes none and there is no `agy uninstall` (agy.md §5). It is the install
script's own path (`TARGET_DIR=$HOME/.local/bin`, `BINARY_PATH=$TARGET_DIR/agy`,
read from the script) plus the Homebrew cask's `zap` stanza, which trashes only
`~/.gemini/antigravity-cli`. Canager moves `~/.local/bin/agy` (the whole
program) after any `agy.<time>.old` beside it; it keeps `~/.gemini/antigravity-cli`
(conversations, history and the program's own state together; no vendor list
separates them), `~/.cache/antigravity` (the installer's staging folder, directly
in `~/.cache`, which Canager never moves anything out of — spec §6.3 listed it for
removal; the step D plan's ruling 1 keeps it), and the two shell files.
