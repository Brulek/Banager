# Samples made from a recording

`adapters/fixtures/` holds recordings only: a tool's output saved byte for
byte on a real Mac. Some cases cannot be recorded without changing the
recording Mac's own tools — a pinned package, when nothing there is pinned
and pinning one would change the user's Homebrew or pipx, which this
project does not do. A sample for such a case is made by editing a
recording, and it lives here, apart from the recordings (the author's
decision R7, 2026-10-06).

Each sample here:

- is in `adapters/fixtures-derived/<id>/<version>/`, for a registered
  adapter id, beside a recording of the same version in
  `adapters/fixtures/<id>/<version>/`, and has a name no recording there
  has;
- is a copy of one of those recordings with as few values changed as the
  case needs, and the `README.md` in its folder names every one of them,
  says why each was changed, and cites the tool's own source for the
  shape the edit gives;
- has a test that puts the edited values back and compares the result
  with the recording, so the README cannot silently stop being true.

`test_every_derived_fixture_has_a_readme_and_the_recording_it_was_made_from`
in `crates/banager-core/tests/fixtures_layout_test.rs` checks the layout.

| Sample | Made from | Checked by |
|---|---|---|
| `brew/7.0.6/outdated-pinned.json` | `adapters/fixtures/brew/7.0.6/outdated.json` | `outdated_pinned_fixture_differs_from_the_recording_only_in_the_pin_fields` (`crates/banager-core/tests/brew_fixtures.rs`) |
| `pipx/1.17.3/list-outdated-pinned.txt` | `adapters/fixtures/pipx/1.17.3/list-outdated.txt` | `test_pinned_fixture_differs_from_the_recording_only_by_the_marker` (`crates/banager-core/src/adapters/pipx.rs`) |
