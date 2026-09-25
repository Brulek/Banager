# rustup 1.29.1 fixtures (native installer route, `standalone-rustup`)

Recorded 2026-09-26 on the author's Mac (host name withheld), macOS 27.0,
arm64, by running the commands below and saving their output byte for
byte. Nothing here is hand-written, and one file was edited after
recording, in one way that is stated below (`layout.txt`); if a parser
disagrees with one of these files, the parser is wrong.

Commands (all read-only; `rustup update`, `rustup self update`, `rustup self
uninstall`, `rustup toolchain list` and `rustup check` were **not** run — the
only rustup invocation is `--version`, run once, under
`RUSTUP_AUTO_INSTALL=0`, the switch every version read Canager makes
carries):
- `RUSTUP_AUTO_INSTALL=0 ~/.cargo/bin/rustup --version` -> `version.txt`
  (stdout: `rustup <version> (<hash> <date>)`; the version is the second
  token) and `version-stderr.txt` (stderr: the two `info:` lines rustup
  prints after it, which the version read never looks at — fed to the
  parser by mistake, the second token of the first line is a word, not a
  version; the second line names the active toolchain's `rustc`)
- `curl -sS https://static.rust-lang.org/rustup/release-stable.toml` ->
  `release-stable.toml` (the file `rustup self update` itself reads: two TOML
  lines, `schema-version` and `version`; its `version` equals the installed
  one, so this Mac was current on the recording day)
- `ls -1 ~/.rustup/toolchains` -> `toolchains.txt` (one entry name per line:
  what the uninstall preview lists as the toolchains that go, read from the
  directory, never from a rustup command; 1 on the recording day,
  `stable-aarch64-apple-darwin`)
- `ls -la ~/.cargo/bin` -> `layout.txt` (corroboration only, not a parser
  input: `rustup` is a regular file; 13 entries are relative symbolic links
  to it — rustup's proxies, `TOOLS` + `DUP_TOOLS` in its `src/lib.rs`, which
  the Unknown page's rule 1 attributes; the other entry (`hexyl`) is the
  rest of the directory: what the uninstall preview names as deleted with
  it (`rustup::bin_programs_rustup_removes`) and, where `.crates2.json`
  records it, what cargo's own inventory places). One substitution was made
  after recording, so that this directory names no account: the account
  name in the owner column became `user`. Nothing else in it changed: the
  permissions, the link counts, the group `staff`, the sizes, the dates and
  the names are as recorded, and no absolute path was there to substitute
  (`ls -la` of a directory prints names relative to it, and the proxies'
  link text is relative, `cargo -> rustup`).

Layout on this Mac: `drwxr-xr-x@ ~/.cargo` and `drwxr-xr-x@ ~/.rustup` —
both real directories, the standard layout, which is the only one Canager
offers the uninstall for (`rustup::standard_roots`); 0 of `CARGO_HOME`,
`RUSTUP_HOME` and `ZDOTDIR` were set in the recording shell. Homebrew's
rustup formula (`Cellar/rustup` under `/opt/homebrew` or `/usr/local`, the
signal `rustup::homebrew_rustup_present` reads): "No such file or directory"
for both, so it is not installed, and the preview's Homebrew line was not
exercised on this Mac.

Shell startup files on the recording day (`grep -n 'cargo/env'` over the
eight files Canager reads, home spelled `~`):
- `~/.zshenv:1:. "$HOME/.cargo/env"`
- `~/.profile:1:. "$HOME/.cargo/env"`
- `~/.zshrc:17:. "$HOME/.cargo/env"`

The files themselves are personal and are not recorded; the startup-file
rule is tested on synthetic files (`adapters/standalone/rustup.rs`). Read
against that rule, the first two are the line rustup itself writes to files
its cleanup visits, and the third is the same line in a file rustup never
edits (`~/.zshrc`), which the preview would name as one that will still
load Cargo's env file afterwards.

The `.crates2.json` parse this recipe's warnings depend on is not recorded
here: it is `adapters/fixtures/cargo/1.98.1/crates2.json`, recorded 2026-09-20
on the same Mac.

What `rustup self uninstall` removes was read from rustup's source at tag
`1.29.1` (GitHub `rust-lang/rustup`), the tag whose commit hash
`version.txt` carries; the line numbers are in `adapters/standalone/rustup.rs`.
