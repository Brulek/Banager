# Homebrew 7.0.6 — made from a recording

Not a recording: see `../../README.md` for what this folder is.

## `outdated-pinned.json` is EDITED — read this before trusting it

Nothing on this Mac is pinned (`brew list --pinned` printed nothing), and
pinning a package to record one would change the user's Homebrew, which this
project does not do. So the pinned case could not be recorded. It was built
by hand from the recording `adapters/fixtures/brew/7.0.6/outdated.json`
(`HOMEBREW_NO_AUTO_UPDATE=1 brew outdated --json=v2`, recorded on 2026-09-24
on Homebrew 7.0.6; see that folder's README), and **only these four values
differ**:

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
