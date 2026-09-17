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
