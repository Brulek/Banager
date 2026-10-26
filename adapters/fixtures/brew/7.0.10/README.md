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
- `search-formula-gpg.txt`, `search-cask-gpg.txt`, `search-desc-gpg.txt` —
  `brew search --formula gpg`, `--cask gpg` and `--desc gpg`, each exit 0
  (recorded 2026-10-27, 03:02). The formula list has `gpg`, `gpg1` and
  `gpg2`, aliases of gnupg and gnupg@1.4, as names of their own.
- `info-formula-gpg.json`, `info-formula-jq.json` — `brew info --json=v2
  --formula {names}`, exit 0, over the formula names the search lists for
  `gpg` and for `jq`, in that order: what names each formula as Homebrew
  does (`name`, `aliases`, `oldnames`).
