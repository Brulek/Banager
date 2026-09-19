# npm 12.0.2 fixtures

Recorded 2026-09-20 on BrulekdeMacBook-Pro.local (macOS 27, Apple Silicon) by running the commands below and
saving their output byte for byte. Nothing here is hand-written or edited — if
a parser disagrees with one of these files, the parser is wrong.

Commands:
- `npm ls -g --depth=0 --json` -> `ls-global.json` (top level is a `dependencies` **object**, not an array)
- `npm outdated -g --json` -> `outdated-global.json` (object keyed by package name; the command exits 1 when anything is outdated, which is not an error)
- `npm search --json --searchlimit 20 jq` -> `search-jq.json`
- `npm view jq description --json` -> `view-jq-description.json`

`view-jq-description.json` is recorded from `npm view jq description --json`.
No parser reads it: this phase's npm adapter runs only `ls`, `outdated`,
`search` and the install/uninstall/upgrade commands, per the per-adapter
contract table. It is kept for whichever later phase populates
`SearchHit.description` / `InstalledArtifact.description`.
