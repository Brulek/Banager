# Homebrew 7.0.6 fixtures

Recorded on: BrulekdeMacBook-Pro (Apple Silicon, macOS 27.0)
Date: 2026-09-24
Homebrew version: 7.0.6 (`brew --version`)

- `outdated.json` — `HOMEBREW_NO_AUTO_UPDATE=1 brew outdated --json=v2`,
  unedited, real output captured verbatim on the machine and date above.
  (`HOMEBREW_NO_AUTO_UPDATE` only stops `brew` from refreshing its own
  catalogue first; it does not change what `outdated` prints.)

## `outdated-pinned.json` is EDITED — read this before trusting it

Nothing on this Mac is pinned (`brew list --pinned` printed nothing), and
pinning a package to record one would change the user's Homebrew, which this
project does not do. So the pinned case could not be recorded. It was built
from `outdated.json` above by hand, and **only these four values differ**:

| Entry | Field | Recorded | Edited to |
|---|---|---|---|
| formula `glib` | `pinned` | `false` | `true` |
| formula `glib` | `pinned_version` | `null` | `"2.88.3"` |
| cask `onyx` | `pinned` | `false` | `true` |
| cask `onyx` | `pinned_version` | `null` | `"5.0.2"` |

Everything else is byte-for-byte `outdated.json`. The test
`outdated_pinned_fixture_differs_from_the_recording_only_in_the_pin_fields`
in `crates/banager-core/tests/brew_fixtures.rs` checks exactly that, so this
table cannot silently stop being true.

Why `pinned_version` was edited too, not only `pinned`: Homebrew never prints
one without the other. It writes `pinned: f.pinned?, pinned_version:
f.pinned_version` (`Library/Homebrew/cmd/outdated.rb:196-200` in 7.0.6; casks
at `cask/cask.rb:472-478`), and `pinned_version` is `nil` exactly when the
package is not pinned (`formula_pin.rb:50-52`, `cask/cask.rb:350-352`). A
pinned entry with `pinned_version: null` is a shape `brew` cannot produce.
The value used is the version each package is installed at in the recording,
which is what a pin holds: the formula's pin is a symlink to its installed
keg (`formula_pin.rb:14-16`), and the cask's to its Caskroom version
directory (`cask/cask.rb:318-357`).

Banager reads `pinned` and nothing else from these two fields
(`OutdatedItem` in `crates/banager-core/src/adapters/brew/parse.rs`).

## `receipts/` — cask install receipts (`INSTALL_RECEIPT.json`)

What `brew uninstall --cask` runs is what Homebrew recorded when it
installed the cask: `Caskroom/<token>/.metadata/INSTALL_RECEIPT.json`, whose
`uninstall_artifacts` is `Cask#artifacts_list(uninstall_only: true)` — one
`{ "<stanza>": [<its arguments>] }` object per artifact that has an
uninstall phase, plus `zap` — and whose `uninstall_flight_blocks` says whether
the cask has Ruby that runs around its uninstall (Homebrew 7.0.6-70,
`cask/tab.rb:31-45`, `cask/cask.rb:709-732`). Banager reads these in the
uninstall preview (`crates/banager-core/src/adapters/brew/cask_receipt.rs`).

**Recorded, unedited** (copied on 2026-09-28 from this Mac's
`/opt/homebrew/Caskroom/<token>/.metadata/INSTALL_RECEIPT.json`, byte for
byte; the five casks installed here): `claudebar.json` (`quit`, `app`,
`zap`), `codexbar.json` (`quit`, `app`, `binary` with a `target`, `zap`),
`libreoffice.json` (`app`, eight `binary`, `command_wrapper`, `zap`),
`onyx.json` (`app`, `zap`), `package-manager-manager.json` (`app`). Their
receipts were written by Homebrew 6.0.16 to 7.0.4, as each file's
`homebrew_version` says.

## `receipts/` files that are CONSTRUCTED — read this before trusting them

No cask installed on this Mac records a `delete`, `trash`, `script`,
`pkgutil`, `launchctl`, `login_item`, `kext`, `signal`, `rmdir`, `artifact`
or uninstall step, and installing one to record it would change the user's
Mac, which this project does not do. So these receipts were built, not
recorded: each is a real cask's definition from the catalogue this
Homebrew downloaded (`~/Library/Caches/Homebrew/api/internal/packages.arm64_golden_gate.jws.json.payload`,
generated for Homebrew 7.0.6-70), turned into the shape the recorded ones
above have:

- Only the stanzas `artifacts_list(uninstall_only: true)` keeps are kept:
  `pkg`, `installer`, `stage_only` and `generated_script` have no uninstall
  phase and are dropped.
- Each catalogue entry `[":<stanza>", <args…>]` becomes
  `{ "<stanza>": [<args…>] }`: a list of positional arguments is spread, a
  hash of keyword arguments appended — `to_args` of the artifact
  (`cask/artifact/abstract_artifact.rb`), which is how the recorded
  `codexbar.json` spells `binary` with a `target` and `claudebar.json`
  spells `uninstall` and `zap`. Keys lose their leading `:`.
- The catalogue's placeholders are filled in as Homebrew does when it loads
  a cask from the catalogue (`api/cask_struct.rb:264-267`): `/$HOME` with
  the made-up home folder `/Users/someone`, `$HOMEBREW_PREFIX` with
  `/opt/homebrew`, `$APPDIR` with `/Applications`.
- The fields Banager does not read (`homebrew_version`, `time`, `source`,
  `built_on`, …) are filled in for shape only; `source.tap_git_head` is all
  zeros.

| File | From the catalogue's | What it records beyond what Homebrew put down |
|---|---|---|
| `microsoft-word.json` | `microsoft-word` 16.113.26092012 | `launchctl`, `quit`, `pkgutil` (two receipts, one of which other Office apps also list) |
| `duckietv.json` | `duckietv` 1.1.5 | `pkgutil`, `delete` (an app and a folder in `~/Library`) |
| `nvs.json` | `nvs` 1.7.1 | `trash` (`~/.nvs`) |
| `gpt4all.json` | `gpt4all` 3.10.0 | `script` (a path), `delete` in `~/Library` |
| `adobe-air.json` | `adobe-air` 51.3.3.1 | `script` (a hash with a relative `executable`), `rmdir` |
| `adobe-creative-cloud.json` | `adobe-creative-cloud` 6.10.0.252.41 | `early_script`, `launchctl`, `quit`, `signal`, `script`, `delete`, `rmdir` |
| `wireshark-chmodbpf.json` | `wireshark-chmodbpf` 4.6.9 | `early_script` alone (a hash: `/usr/sbin/installer` runs the vendor's uninstaller package), `pkgutil`; its `pkg` has no uninstall phase and is dropped |
| `gutenprint.json` | `gutenprint` 5.3.3 | `script` (a list of hashes), `pkgutil`, `delete` (globs) |
| `airscroll.json` | `airscroll` 1.3.3 | `login_item` |
| `airparrot.json` | `airparrot` 3.1.8 | `quit`, `kext` |
| `malus.json` | `malus` 5.0.1 | `rmdir` only |
| `dbeaver-community.json` | `dbeaver-community` 26.2.1 | `signal` only |
| `charles.json` | `charles` 5.2.1 | `launchctl`, `quit`, `delete`, an uninstall step `delete_keychain_certificate` |
| `openzfs.json` | `openzfs` 2.4.1 | an uninstall step `run`, `launchctl`, `pkgutil`, `postflight_steps` (`set_ownership`) |
| `miniconda.json` | `miniconda` py314_26.7.1-1 | an uninstall step `move`, `delete`, `postflight_steps` |
| `appvolume.json` | `appvolume` 0.1.38 | `launchctl`, `quit`, `pkgutil`, `delete`, an uninstall step `terminate_process` |
| `pycharm-edu.json` | `pycharm-edu` 2022.2.2,222.4345.35 | an uninstall step `remove` and nothing else: `charm` in each folder Homebrew looks for commands in (`base: search_path`), where the file holds a given text |
| `playdate-simulator.json` | `playdate-simulator` 3.1.2 | an uninstall step `remove` (`/usr/local/bin/arm-*`, where each is a link whose target holds `playdate`), `pkgutil`, `delete`, `trash`, `rmdir` |
| `autofirma.json` | `autofirma` 1.9.2 | `quit`, `pkgutil`, `delete`, two uninstall steps `delete_keychain_certificate` (`AutoFirma ROOT`, `127.0.0.1`) |
| `betwixt.json` | `betwixt` 1.6.1 | an uninstall step `delete_keychain_certificate` with a `matching_certificate` file |
| `little-snitch@4.json` | `little-snitch@4` 4.6.1 | `launchctl` and nothing Homebrew put down: its `installer manual:` has no uninstall phase and is dropped |
| `twelite-stage.json` | `twelite-stage` 202508,R2 | an `artifact` placed at `~/MWSTAGE` |
| `touchosc-editor.json` | `touchosc-editor` 1.8.9 | an `artifact` placed under `/$HOME`, so the home folder's absolute path |
| `graalvm-jdk.json` | `graalvm-jdk` 25.0.4 | an `artifact` placed in `/Library/Java/JavaVirtualMachines` |
| `font-fira-code.json` | `font-fira-code` 6.2 | nothing: seven `font` stanzas |

Two more are edited copies of the recorded `package-manager-manager.json`,
only `uninstall_artifacts` (and, for the first, `uninstall_flight_blocks`
and `source`) changed:

- `uninstall-flight-block.json`: Homebrew's own test cask of that name
  (`Library/Homebrew/test/support/fixtures/cask/aged_caskroom/uninstall-flight-block/`),
  an `app` and an `uninstall_preflight` block, which a receipt records by
  name only (`{ "uninstall_preflight": null }`, `artifacts_list`), with
  `uninstall_flight_blocks: true`.
- `unknown-stanza.json`: an `app` and `future_stanza`, a stanza no Homebrew
  has — what a newer Homebrew's record would look like to this Banager.
