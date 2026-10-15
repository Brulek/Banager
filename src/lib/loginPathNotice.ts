/**
 * The Overview's warning when the login shell's `PATH` could not be read
 * (`get_system_facts`' `login_path`: crates/banager-core/src/runner/
 * login_path.rs). Banager then looks for sources along the few folders an
 * app opened from Finder starts with, and those Terminal finds -- npm,
 * pipx, uv, Cargo and the rest -- may be missing from every list. Check
 * Tool Setup says it too (`setupCheck.terminal.loginNotRead`), but only
 * there no one looks; said here, a row of the Overview's problems, with
 * Check Again, which reads the shell once more.
 *
 * Null while the facts are not in, and whenever `PATH` is the login
 * shell's.
 */
import type { SourceNoticeSpec } from "./sources";
import type { SystemFacts } from "./types";

export function loginPathNotice(facts: Pick<SystemFacts, "login_path"> | null | undefined): SourceNoticeSpec | null {
  if (facts === null || facts === undefined || facts.login_path) return null;
  return {
    id: "login-path-unread",
    variant: "warning",
    titleKey: "loginPathNotice.title",
    descriptionKey: "loginPathNotice.description",
    action: { id: "checkAgain", labelKey: "header.checkAgain" },
  };
}
