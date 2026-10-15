# opencode standalone install — what Banager's recipe is built from

Nothing in this directory was recorded from a run of opencode: Banager never
runs `opencode` (it reads no version at all, `VersionSource::NotRead`). The
directory name is the day the layout was read, not an opencode version, and
`adapters/meta/standalone-opencode.toml` lists no `verified_versions` for the
same reason.

Source: the official install script, fetched as text and read, never
executed, on 2026-10-01:

    curl -sL https://opencode.ai/install -o opencode-install.sh.txt

13,690 bytes, SHA-256
`fc3c1b2123f49b6df545a7622e5127d21cd794b15134fc3b66e1ca49f7fb297e`. The
script itself is not copied here; these are the lines the recipe
(`OPENCODE` in `crates/banager-core/src/adapters/standalone/recipes.rs`)
rests on, by line number in that copy:

- 68-69: `INSTALL_DIR=$HOME/.opencode/bin` and `mkdir -p "$INSTALL_DIR"` — a
  fixed folder; no environment variable moves it.
- 327-346 (`download_and_install`): the release archive is downloaded and
  unpacked in a temporary folder under `$TMPDIR`, then
  `mv "$tmp_dir/opencode" "$INSTALL_DIR"` and `chmod 755` — the launcher
  `~/.opencode/bin/opencode` is one regular file, the whole program. The
  temporary folder is removed.
- 348-352 (`install_from_binary`, `--binary <path>`): the same file, copied.
- 221-235 (`check_version`): the only way the script learns an installed
  version is by running `opencode --version`. It writes no file that names
  the version, so Banager, which does not run opencode, has none to read.
- 183-204: the newest version comes from
  `api.github.com/repos/anomalyco/opencode/releases/latest` and the
  download from `github.com` — hosts not on Banager's list, so Banager does
  not look for updates for this install (`Latest::Unchecked`).
- 362-439 (`add_to_path`): unless `--no-modify-path`, the script appends a
  `# opencode` line and `export PATH=$HOME/.opencode/bin:$PATH` to the first
  shell file it finds (`~/.zshrc`, `~/.zshenv`, … for zsh, line 386).

The script writes nothing else into `~/.opencode`. On the author's Mac
(2026-10-01, names only) that folder also holds `package.json`,
`package-lock.json`, `.gitignore` and `node_modules/`, dated a minute after
`bin/` was made (2026-09-18 11:58 and 11:59). The script writes none of
them, so they are opencode's own, and they name a plugin package
(`@opencode-ai/plugin`) and its version, not the program's. Banager does
not read them.

opencode's documentation (opencode.ai/docs/config, "Autoupdate", read the
same day): opencode downloads new updates itself when it starts, unless its
`autoupdate` setting is `false` or `"notify"`. Banager does not read that
setting, so the row says it updates itself by default, and offers no Update
button and no uninstall (the author's decision).

Tests build their own layouts in temporary folders
(`tests/standalone_opencode_test.rs`) and never write here.
