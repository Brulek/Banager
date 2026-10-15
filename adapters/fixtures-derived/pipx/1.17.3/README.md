# pipx 1.17.3 — made from a recording

Not a recording: see `../../README.md` for what this folder is.

## `list-outdated-pinned.txt` is EDITED — read this before trusting it

Nothing on this Mac is pinned in pipx (on 2026-09-24 `pipx list --pinned` printed
`nothing has been installed with pipx 😴`), and pinning a tool to record one would change the user's pipx,
which this project does not do. So the pinned case could not be recorded. It was built by hand from the
recording `adapters/fixtures/pipx/1.17.3/list-outdated.txt` (`pipx list --outdated`, recorded on
2026-09-20; see that folder's README), and **the only edit is ` [pinned]` (a space, then `[pinned]`)
inserted after `cowsay`**:

| File | Line 1 |
|---|---|
| `list-outdated.txt` (recorded) | `cowsay: 5.0 -> 6.1` |
| `list-outdated-pinned.txt` (edited) | `cowsay [pinned]: 5.0 -> 6.1` |

Everything else, the versions and the trailing newline included, is byte-for-byte `list-outdated.txt`. The
test `test_pinned_fixture_differs_from_the_recording_only_by_the_marker` in
`crates/banager-core/src/adapters/pipx.rs` checks exactly that, so this table cannot silently stop being
true.

Where the edited shape comes from, in pipx 1.17.3's own source
(`/opt/homebrew/Cellar/pipx/1.17.3/libexec/lib/python3.14/site-packages/pipx/` on the recording Mac):
- The line is built by `_package_message` as
  `f"{subject}{' [pinned]' if package.pinned else ''}: {package.version} -> {package.latest_version}"`
  (`commands/outdated.py:238-244`). The marker goes between the name and the colon, and nothing else on
  the line changes.
- A pinned tool is still listed, with the same two versions. `list --outdated` calls `list_outdated`
  (`main.py:1370`), which leaves `upgradable_only` at its default `False` (`commands/outdated.py:31-35`,
  `:72`), and a pinned package is dropped only when `upgradable_only` is set (`commands/outdated.py:191-192`).
  Both versions come from the same pip query whether the package is pinned or not
  (`_collect_outdated`, `commands/outdated.py:214-235`).
- The line has this shape in every pipx that has `list --outdated`: `_package_message` reads the same at
  every tag from 1.16.0 (the first with `commands/outdated.py`) through 1.17.6, checked against
  github.com/pypa/pipx on 2026-09-24.

A really pinned `cowsay` would also say `"pinned": true` in the recorded `list.json`. That file was not
edited, and no edited copy of it is here, because Banager does not read `pinned` from it: `PipxMainPackage`
in `crates/banager-core/src/adapters/pipx.rs` has only `package` and `package_version`.
