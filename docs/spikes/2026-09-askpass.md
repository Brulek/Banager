# Spike: SUDO_ASKPASS for cask installs without a TTY

Date: 2026-09-18
Command under test: `SUDO_ASKPASS="$PWD/scripts/canager-askpass.sh" sudo -A -k true < /dev/null`

| Scenario | Expected | Observed |
|---|---|---|
| Correct password entered | Dialog appears; `sudo` succeeds; exit code 0 | _pending user: Step 3_ |
| Cancel clicked in the dialog | `sudo` fails cleanly with a nonzero exit code, no hang | _pending user: Step 4_ |
| Wrong password entered | Either `sudo` re-prompts via the dialog again, or fails with a nonzero exit code | _pending user: Step 5_ |

## Conclusion

_To be completed after the user runs Steps 3–5. State plainly whether `SUDO_ASKPASS` + this `osascript` dialog is viable for Canager's cask-install flow without a Terminal window open. If any scenario hung, required a TTY, or silently did nothing, that determines whether Task 11's `SUDO_ASKPASS` passthrough is usable as-is or whether cask installs needing `sudo` must fall back to "open Terminal and run this command" (see spec section 14, risk row 1)._
