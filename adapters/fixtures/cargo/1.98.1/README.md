# cargo 1.98.1 fixtures

Recorded 2026-09-20 on BrulekdeMacBook-Pro.local (macOS 27, Apple Silicon) by running the commands below and
saving their output byte for byte. Nothing here is hand-written or edited — if
a parser disagrees with one of these files, the parser is wrong.

Commands:
- `~/.cargo/.crates2.json` copied to `crates2.json`
- `cargo install --list` -> `install-list.txt`

`hexyl` was installed solely to produce a non-empty file. **The package name,
version and source live in the JSON *key*** — `"hexyl 0.17.0 (registry+https://
github.com/rust-lang/crates.io-index)"` — and not in the value, which carries
only `bins`, `features`, `profile`, `rustc`, `target` and `version_req`. A
parser that looks for a `name` field inside the value will find nothing.

`install-list.txt` is recorded from `cargo install --list`. The adapter reads
`.crates2.json` instead — the text output begins with two unrelated
workspace-profile warning lines, which is exactly why. This file is kept as
corroboration that the `.crates2.json` parse agrees with what cargo itself
reports, not as a parser input.
