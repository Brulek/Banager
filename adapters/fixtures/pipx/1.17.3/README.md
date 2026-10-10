# pipx 1.17.3 fixtures

Recorded 2026-09-20 on BrulekdeMacBook-Pro.local (macOS 27, Apple Silicon) by running the commands below and
saving their output byte for byte. Nothing here is hand-written or edited. If a parser disagrees with
one of the recorded files, the parser is wrong. The pinned case, which could not be recorded, was made by
editing `list-outdated.txt`, and lives with the other samples made from a recording, as
`adapters/fixtures-derived/pipx/1.17.3/list-outdated-pinned.txt` (that folder's README says exactly what
was edited).

Commands:
- `pipx list --json` -> `list.json` (version lives at `venvs.<name>.metadata.main_package.package_version`)
- `pipx list --outdated` -> `list-outdated.txt`

`cowsay` was deliberately installed at 5.0 before recording so the outdated output is non-empty
(`"package_or_url": "cowsay==5.0"`, `list.json:37`). That is a version specifier, not a `pipx pin`: the
same entry says `"pinned": false` (`list.json:39`). **`pipx list --outdated` prints prose, not
machine-readable output**: one `name: old -> new` line per outdated tool, and the literal
sentence `pipx found no available upgrades.` when there are none. The parser
must treat an unmatched line as "no updates", never as an error.
