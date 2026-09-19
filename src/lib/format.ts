import type { Outcome } from "./types";

/** Display-only: renders one argv token so the preview cannot blur where one
 *  argument ends and the next begins. Uses an allow-list (the `shlex.quote`
 *  rule) rather than a deny-list, so a token carrying a shell metacharacter
 *  like `$`, `;` or a backtick is quoted too: an operator who copies this
 *  preview into a terminal must get the command they were shown.
 *  Nothing here is ever executed — submission only ever sends a PlanId. */
export function displayToken(token: string): string {
  if (token === "") return "''";
  if (/^[A-Za-z0-9_@%+=:,./-]+$/.test(token)) return token;
  return `'${token.replace(/'/g, `'\\''`)}'`;
}

/** The `operations.outcome.*` key suffix for an Outcome, mirroring its externally tagged variant name. */
export function outcomeKey(outcome: Outcome): string {
  if (typeof outcome === "string") return outcome;
  if ("NeedsAttention" in outcome) return "NeedsAttention";
  return "Failed";
}

/** Interpolation values for `operations.outcome.<outcomeKey(outcome)>`. */
export function outcomeArgs(outcome: Outcome): Record<string, unknown> {
  if (typeof outcome === "string") return {};
  if ("NeedsAttention" in outcome) return { message: outcome.NeedsAttention };
  return { summary: outcome.Failed.summary };
}
