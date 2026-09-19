# uv 0.12.17 fixtures

Recorded 2026-09-20 on BrulekdeMacBook-Pro.local (macOS 27, Apple Silicon) by running the commands below and
saving their output byte for byte. Nothing here is hand-written or edited — if
a parser disagrees with one of these files, the parser is wrong.

Commands:
- `uv tool list --show-paths` -> `tool-list-show-paths.txt` (`name vX.Y.Z (path)` then one `- binary (path)` line per executable)
- `uv tool list --outdated` -> `tool-list-outdated.txt` (`name vOLD [latest: NEW]` then its binary lines)

`ruff` was deliberately pinned to 0.15.0 before recording so the outdated output
is non-empty. With nothing outdated, `uv tool list --outdated` prints **nothing
at all** — not a message, not a newline. With no tools installed at all it
prints `No tools installed`.
