# Codex standalone install — what Banager's recipe is built from

Nothing in this directory was recorded from a Mac: Codex's own installer has
not been run on the author's Mac, and Banager never runs `codex` (it reads
the version from a link, `VersionSource::ReleaseLink`). The directory name
is the day the layout was read, not a Codex version, and
`adapters/meta/standalone-codex.toml` lists no `verified_versions` for the
same reason.

Source: the official install script, fetched as text and read, never
executed, on 2026-10-01:

    curl -s -L https://chatgpt.com/codex/install.sh -o install.sh.txt

`chatgpt.com/codex/install.sh` redirected (HTTP 200 after the redirect) to
`https://releases.openai.com/codex/install.sh`, 34,564 bytes, SHA-256
`150e3cf675682efeaac115aa3747add3f27887896d04ce6d0b56478d8b428bf6`. The
script itself is not copied here; these are the lines the recipe
(`CODEX` in `crates/banager-core/src/adapters/standalone/recipes.rs`) rests
on, by line number in that copy:

- 16-18: `BIN_DIR="${CODEX_INSTALL_DIR:-$HOME/.local/bin}"`, the launcher
  `$BIN_DIR/codex`, and the helper `$BIN_DIR/codex-code-mode-host`.
- 19-20: `CODEX_HOME_DIR="${CODEX_HOME:-$HOME/.codex}"`,
  `STANDALONE_ROOT="$CODEX_HOME_DIR/packages/standalone"` — the recipe's
  root. (Line 22 switches to `packages/app-server-daemon` for a
  daemon-only install, which writes no launcher and is not listed.)
- 24-25: `RELEASES_DIR="$STANDALONE_ROOT/releases"`,
  `CURRENT_LINK="$STANDALONE_ROOT/current"`.
- 33: `AUTO_UPDATE_VERSION="$STANDALONE_ROOT/auto-update-version"`.
- 1037-1042 (`update_current_link`) and 1271: `current` is re-pointed, as a
  symbolic link with absolute text, at `$RELEASES_DIR/$release_name`.
- 1140-1141: `release_name="$resolved_version-$vendor_target"`; on a Mac the
  target is `aarch64-apple-darwin` or `x86_64-apple-darwin` (lines
  1116-1124), so the folder name minus that ending is the version.
- 1054-1069 (`update_visible_command`): `~/.local/bin/codex` links to
  `$CURRENT_LINK/bin/codex` (or `$CURRENT_LINK/codex` for an older
  layout), and on macOS `codex-code-mode-host` to
  `$CURRENT_LINK/bin/codex-code-mode-host`.
- 1272-1277: the script writes the release name to `auto-update-version`
  when it installs the latest release and deletes the file for a pinned
  one; 1203-1222: a scheduled update re-runs the script only while that
  file names the release `current` points at. So Banager calls the row
  self-updating only when the file names the current release.
- 920-945 (`handle_conflicting_install`): an npm (`@openai/codex`) or
  Homebrew cask (`codex`) copy may stay installed beside this one, and the
  script says the `PATH` order decides which `codex` runs.

The newest version comes from `releases.openai.com` or GitHub (lines 10,
322-335), neither of which is on Banager's host list, so Banager does not
look for updates for this install (`Latest::Unchecked`), offers no Update
button, and offers no uninstall (`UninstallBlocked::NoSafeMethod`; a
move-to-Trash list is the author's decision D5).

Tests build their own layouts in temporary folders
(`release_link.rs`, `tests/standalone_codex_test.rs`) and never write here.
