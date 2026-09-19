# pipx 1.17.3 fixtures

Recorded 2026-09-20 on BrulekdeMacBook-Pro.local (macOS 27, Apple Silicon) by running the commands below and
saving their output byte for byte. Nothing here is hand-written or edited — if
a parser disagrees with one of these files, the parser is wrong.

Commands:
- `pipx list --json` -> `list.json` (version lives at `venvs.<name>.metadata.main_package.package_version`)
- `pipx list --outdated` -> `list-outdated.txt`

`cowsay` was deliberately pinned to 5.0 before recording so the outdated output
is non-empty. **`pipx list --outdated` prints prose, not machine-readable
output**: one `name: old -> new` line per outdated tool, and the literal
sentence `pipx found no available upgrades.` when there are none. The parser
must treat an unmatched line as "no updates", never as an error.
