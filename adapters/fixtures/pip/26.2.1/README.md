# pip 26.2.1 fixtures

Recorded 2026-09-20 on BrulekdeMacBook-Pro.local (macOS 27, Apple Silicon) by running the commands below and
saving their output byte for byte. Nothing here is hand-written or edited — if
a parser disagrees with one of these files, the parser is wrong.

Commands (this interpreter is Homebrew Python, which is PEP 668 externally managed):
- `pip3 list --format=json` -> `list.json`
- `pip3 list --outdated --format=json` -> `list-outdated.json`
- `pip3 list --not-required --format=json` -> `list-not-required.json`

`--not-required` means "nothing else installed depends on this". It is **not**
the same as "the user asked for this", so the adapter maps it to
`InstallReason::Unknown`, never to `Requested`.
