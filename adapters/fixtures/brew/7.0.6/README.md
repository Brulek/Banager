# Homebrew 7.0.6 fixtures

Recorded on: BrulekdeMacBook-Pro (Apple Silicon, macOS 27.0)
Date: 2026-09-24
Homebrew version: 7.0.6 (`brew --version`)

- `outdated.json` — `HOMEBREW_NO_AUTO_UPDATE=1 brew outdated --json=v2`,
  unedited, real output captured verbatim on the machine and date above.
  (`HOMEBREW_NO_AUTO_UPDATE` only stops `brew` from refreshing its own
  catalogue first; it does not change what `outdated` prints.)

The pinned case is not here: nothing on this Mac is pinned, so it was made
by editing `outdated.json`, and it lives with the other samples made from
a recording, as `adapters/fixtures-derived/brew/7.0.6/outdated-pinned.json`
(that folder's README says exactly what was edited).

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

The other receipts the tests read are not recordings: 25 built from
Homebrew's catalogue and 2 edited from `package-manager-manager.json`, for
stanzas no cask installed here records. They live apart from these, in
`adapters/fixtures-derived/brew/7.0.6/receipts/` (the author's decision R7,
2026-10-06), and that folder's README says how each was made.
