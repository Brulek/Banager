# Claude Code 2.1.282 fixtures (native installer route, `standalone-claude`)

Recorded 2026-09-25 on BrulekdeMacBook-Pro.local, macOS 27.0, arm64.
Command output files are saved byte for byte. The recording itself upgraded nothing.

Why 2.1.282: minutes before these commands ran, a scheduled job on this Mac (the owner's
own daily updater, not Canager and not this recording) ran `claude update` at 06:00 local
time, which moved the launcher from 2.1.281 to 2.1.282 at 06:00:16 (its log: "Successfully
updated from 2.1.281 to version 2.1.282"). Every file here was recorded after that and agrees
on 2.1.282: the version line, the launcher's link text, `layout.txt` (where the 06:00 entries
are that update's) and the `latest` pointer.

Commands:
- `DISABLE_AUTOUPDATER=1 ~/.local/bin/claude --version` -> `version.txt`
- `curl --fail --silent --show-error https://downloads.claude.ai/claude-code-releases/latest` -> `latest.txt`
- `curl --fail --silent --show-error https://downloads.claude.ai/claude-code-releases/stable` -> `stable.txt`
- `ls -la ~/.local/bin/claude ~/.local/share/claude/versions` -> `layout.txt`

Both pointers answered direct HTTP 200; curl's checked status trailer is excluded from the saved bodies.
Canager adds the documented background-check switch to version reads. Whether bare --version starts that check was not observed. Manual updates work with the switch set.
The recording ran no claude update, claude install, or bare claude.
Launcher link text: '/Users/brulek/.local/share/claude/versions/2.1.282'.
brew list --cask claude-code: exit=1; stdout=''.
npm list -g --depth=0 @anthropic-ai/claude-code: exit=1; stdout='/opt/homebrew/lib\n└── (empty)'.
These command-scoped observations do not rule out other prefixes. Shared-exclusion and PATH cases use synthetic unit tests.
autoUpdatesChannel key: absent; settings contents are not recorded. Channel tests use inline JSON in a temporary home.
The dotted comparison decides whether either pointer is newer; no channel ordering is assumed by this recording.
