# Homebrew 7.0.3 fixtures

Recorded on: BrulekMBA (Apple Silicon, macOS 27.0)
Date: 2026-09-17
Homebrew version: 7.0.3 (`brew --version`)

These files are unedited, real output from the commands named after each
file, captured verbatim on the machine and date above:

- `info-installed.json` — `brew info --installed --json=v2`
- `outdated.json` — `brew outdated --json=v2`
- `search-jq.txt` — `brew search jq`
- `search-desc-jq.txt` — `brew search --desc jq`
- `uses-jq.txt` — `brew uses --installed jq`
- `version.txt` — `brew --version`

`info-installed.json` and `outdated.json` reflect this machine's actual
installed package list. That is expected and acceptable — it contains no
secrets, only formula/cask names, versions, and installation metadata.

`uses-jq.txt` is empty, and that is the real recording: nothing installed on
either machine below depends on jq, so `brew uses --installed jq` printed
nothing. It is kept as the no-dependents case — the one where an uninstall
carries no dependency warning.

## Added later

Recorded on: BrulekdeMacBook-Pro (Apple Silicon, macOS 27.0)
Date: 2026-09-22
Homebrew version: 7.0.6 (`brew --version`)

- `uses-pcre2.txt` — `brew uses --installed pcre2`

Same convention as above: unedited, real output of the command named. It
covers the case `uses-jq.txt` cannot, a formula with installed dependents,
which is what makes `plan` attach a dependency warning to an uninstall.
Recorded against 7.0.6 rather than the 7.0.3 this directory is named for,
because that is the Homebrew on the machine at hand; `brew uses` prints a
plain list of formula names and the shape has not changed between the two.
Nothing here is hand-written — if you re-record it, run the real command
and redirect it into the file.
