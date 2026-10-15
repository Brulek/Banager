# Samples made, not recorded

`adapters/fixtures/` holds recordings: what a tool printed, or a file it
wrote, saved on a real Mac. Each folder's README names the commands and
says where a recording was changed at all; the only such changes are in
four `layout.txt` listings (`standalone-claude`, `standalone-rustup`,
`standalone-grok`, `standalone-agy`), where the account name or the home
folder's path was masked so that no account is named in the repository.

Some cases cannot be recorded without changing the recording Mac's own
tools — a pinned package, when nothing there is pinned and pinning one
would change the user's Homebrew or pipx, or a cask receipt with a stanza
no cask installed there has, when installing one would change the Mac,
which this project does not do. A sample for such a case is made, and it
lives here, apart from the recordings (the author's decision R7,
2026-10-06). It is made in one of two ways:

- **edited**: a copy of a recording with as few values changed as the
  case needs. The `README.md` in its folder names every changed value and
  says why it was changed, and a test puts the recording's values back and
  compares the result with the recording, so the README cannot silently
  stop being true;
- **built** (Homebrew cask receipts only): a real cask's definition from
  the catalogue that Homebrew downloaded on the recording Mac, turned into
  a receipt's shape by the rules that folder's README states.

Each sample is in `adapters/fixtures-derived/<id>/<version>/`, for a
registered adapter id, beside a recording of the same version in
`adapters/fixtures/<id>/<version>/`. Its folder's README names it, and no
recording there has its path.
`test_every_derived_fixture_has_a_readme_and_the_recording_it_was_made_from`
in `crates/banager-core/tests/fixtures_layout_test.rs` checks the layout.

| Sample | Made from | Checked by |
|---|---|---|
| `brew/7.0.6/outdated-pinned.json` | edited: `adapters/fixtures/brew/7.0.6/outdated.json` | `outdated_pinned_fixture_differs_from_the_recording_only_in_the_pin_fields` (`crates/banager-core/tests/brew_fixtures.rs`) |
| `brew/7.0.6/receipts/uninstall-flight-block.json`, `brew/7.0.6/receipts/unknown-stanza.json` | edited: `adapters/fixtures/brew/7.0.6/receipts/package-manager-manager.json` | `the_two_edited_receipts_differ_from_the_recording_only_where_this_readme_says` (`crates/banager-core/src/adapters/brew/cask_receipt.rs`) |
| the other 25 in `brew/7.0.6/receipts/` | built: Homebrew 7.0.6's cask catalogue, one cask each (table in `brew/7.0.6/README.md`) | no test can compare them with the catalogue, which is not in the repository; `the_receipts_among_the_recordings_are_the_five_recorded_on_this_mac` (`cask_receipt.rs`) keeps them out of `adapters/fixtures/` |
| `pipx/1.17.3/list-outdated-pinned.txt` | edited: `adapters/fixtures/pipx/1.17.3/list-outdated.txt` | `test_pinned_fixture_differs_from_the_recording_only_by_the_marker` (`crates/banager-core/src/adapters/pipx.rs`) |
