import type { Fault, Outcome } from "./types";

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

/** The `operations.outcome.*` key suffix for an Outcome, mirroring its
 *  externally tagged variant name -- and, for `NeedsAttention` and
 *  `CanagerFailed`, which reason it carries, since each has its own
 *  sentence. A `Failed` whose tool said nothing on stderr gets its own
 *  sentence too, rather than "Failed: " and a blank. */
export function outcomeKey(outcome: Outcome): string {
  if (typeof outcome === "string") return outcome;
  if ("NeedsAttention" in outcome) return `NeedsAttention.${outcome.NeedsAttention}`;
  if ("Failed" in outcome) return outcome.Failed.summary.trim() ? "Failed" : "FailedSilent";
  if ("CanagerFailed" in outcome) return `CanagerFailed.${faultKey(outcome.CanagerFailed)}`;
  const unhandled: never = outcome;
  return unhandled;
}

/** Interpolation values for `operations.outcome.<outcomeKey(outcome)>`. */
export function outcomeArgs(outcome: Outcome): Record<string, unknown> {
  if (typeof outcome === "string") return {};
  if ("NeedsAttention" in outcome) return {};
  if ("Failed" in outcome) return { summary: outcome.Failed.summary.trim() };
  if ("CanagerFailed" in outcome) return faultArgs(outcome.CanagerFailed);
  const unhandled: never = outcome;
  return unhandled;
}

/**
 * The variant name of one of Canager's own failure reasons. Each `Fault`
 * variant needs a case here and a sentence in both locales under
 * `operations.outcome.CanagerFailed`: a reason that arrived with no words
 * would be this project's signature defect, defined and never rendered.
 */
function faultKey(fault: Fault): string {
  if (typeof fault === "string") {
    switch (fault) {
      case "Panicked":
      case "SourceGone":
      case "Unsupported":
      case "Internal":
        return fault;
      default: {
        const unhandled: never = fault;
        return unhandled;
      }
    }
  }
  if ("ProgramMissing" in fault) return "ProgramMissing";
  if ("SpawnFailed" in fault) return "SpawnFailed";
  const unhandled: never = fault;
  return unhandled;
}

/** The data a `Fault` carries, for its sentence: a path, or the operating
 *  system's own reason -- never prose of Canager's. */
function faultArgs(fault: Fault): Record<string, unknown> {
  if (typeof fault === "string") return {};
  if ("ProgramMissing" in fault) return { program: fault.ProgramMissing.program };
  if ("SpawnFailed" in fault) return { detail: fault.SpawnFailed.detail };
  const unhandled: never = fault;
  return unhandled;
}
