/**
 * What the Unknown page says when Show in Finder did not show a program,
 * from `reveal_in_finder`'s refusal (src-tauri/src/reveal.rs), in the
 * `{"kind": ...}` envelope its refusals use:
 * - `changed_since_scan`: the row's program is gone, or was replaced -- an
 *   app that updates itself replaces its files -- or leads somewhere else
 *   since the scan, so only a new scan says what is there now: "It changed
 *   after the last scan. Scan again.";
 * - anything else -- a path the scan did not resolve, or Finder failing --
 *   "Couldn't show it in Finder".
 */
export function revealFailureKey(error: Error | null): "unknownReveal.changedSinceScan" | "unknown.showInFinderFailed" {
  return refusalKind(error) === "changed_since_scan" ? "unknownReveal.changedSinceScan" : "unknown.showInFinderFailed";
}

/** The `kind` of the envelope `error.message` carries, or `null` for any other text. */
function refusalKind(error: Error | null): string | null {
  if (error === null) return null;
  let parsed: unknown;
  try {
    parsed = JSON.parse(error.message);
  } catch {
    return null;
  }
  if (typeof parsed !== "object" || parsed === null) return null;
  const kind = (parsed as Record<string, unknown>).kind;
  return typeof kind === "string" ? kind : null;
}
