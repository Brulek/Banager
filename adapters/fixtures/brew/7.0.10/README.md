# Homebrew 7.0.10 fixtures

Recorded on: BrulekdeMacBook-Pro (Apple Silicon, macOS 27.0)
Date: 2026-10-27
Homebrew version: 7.0.10-37-g1ef73bb (`brew --version`)

Each is real output, captured verbatim with `HOMEBREW_NO_AUTO_UPDATE=1`
(which only stops `brew` refreshing its catalogue first), stdout not a
terminal, as Banager runs it:

- `search-formula-jq.txt` — `brew search --formula jq`, exit 0.
- `search-cask-jq.txt` — `brew search --cask jq`, exit 0.
- `search-desc-jq.txt` — `brew search --desc jq`, exit 0.
- `search-cask-claude.txt` — `brew search --cask claude`, exit 0. Its last
  line, `gautham-v/tap/claudebar`, is a cask of a third-party tap this Mac
  has tapped: tap-qualified, as Homebrew names every cask outside its own.
- `search-formula-none.stderr.txt` — the stderr of `brew search --formula
  zzzznotapackage`, which exits 1 with no stdout: how a search with no
  result ends.
