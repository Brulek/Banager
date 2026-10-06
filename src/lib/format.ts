import type { Attention, Fault, Outcome } from "./types";

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
 *  `BanagerFailed`, which reason it carries, since each has its own
 *  sentence. A `Failed` whose tool said nothing on stderr gets its own
 *  sentence too, rather than "Failed: " and a blank. */
export function outcomeKey(outcome: Outcome): string {
  // Every unit variant by name, not `return outcome`: a new one must fail
  // to compile here until it has a sentence, rather than reach the screen
  // as a raw key like `operations.outcome.Skipped`.
  if (typeof outcome === "string") {
    switch (outcome) {
      case "Succeeded":
      case "Cancelled":
      case "Unconfirmed":
        return outcome;
      default: {
        const unhandled: never = outcome;
        return unhandled;
      }
    }
  }
  if ("NeedsAttention" in outcome) return `NeedsAttention.${attentionKey(outcome.NeedsAttention)}`;
  if ("Failed" in outcome) return outcome.Failed.summary.trim() ? "Failed" : "FailedSilent";
  if ("BanagerFailed" in outcome) return `BanagerFailed.${faultKey(outcome.BanagerFailed)}`;
  const unhandled: never = outcome;
  return unhandled;
}

/** The variant name of one way reconcile contradicted a command's success
 *  -- or, `BackAfterUninstall`, what a path-list uninstall's own last look
 *  found -- listed for the same reason as `faultKey`'s: each needs a
 *  sentence in both locales under `operations.outcome.NeedsAttention`, and a
 *  template string would take a new one without a word. */
function attentionKey(attention: Attention): string {
  switch (attention) {
    case "NotInstalledAfterInstall":
    case "StillInstalledAfterUninstall":
    case "GoneAfterUpgrade":
    case "UnchangedAfterUpgrade":
    case "BackAfterUninstall":
      return attention;
    default: {
      const unhandled: never = attention;
      return unhandled;
    }
  }
}

/**
 * The key of what to do next about an outcome, or null when its sentence
 * says it all: the copy table's 〔抽屉〕 halves. The operation bar says how
 * an operation ended in a few words (`outcomeKey`); the log drawer, where
 * the log is, adds this under them. Every key spelled out, so the
 * reachability test finds each one, and a switch with no default at every
 * level, so a new variant fails `tsc` here until it is sorted.
 */
export function outcomeDetailKey(outcome: Outcome): string | null {
  if (typeof outcome === "string") {
    switch (outcome) {
      case "Succeeded":
      case "Cancelled":
        return null;
      case "Unconfirmed":
        return "operations.outcome.UnconfirmedDetail";
      default: {
        const unhandled: never = outcome;
        return unhandled;
      }
    }
  }
  if ("NeedsAttention" in outcome) {
    switch (outcome.NeedsAttention) {
      case "UnchangedAfterUpgrade":
        return "operations.outcome.NeedsAttention.UnchangedAfterUpgradeDetail";
      case "BackAfterUninstall":
        return "operations.outcome.NeedsAttention.BackAfterUninstallDetail";
      case "NotInstalledAfterInstall":
      case "StillInstalledAfterUninstall":
      case "GoneAfterUpgrade":
        return null;
      default: {
        const unhandled: never = outcome.NeedsAttention;
        return unhandled;
      }
    }
  }
  // The tool's own words are the sentence; a tool that said nothing gets
  // pointed at its output.
  if ("Failed" in outcome) return outcome.Failed.summary.trim() ? null : "operations.outcome.FailedSilentDetail";
  if ("BanagerFailed" in outcome) {
    const fault = outcome.BanagerFailed;
    if (typeof fault === "string") {
      switch (fault) {
        case "Panicked":
          return "operations.outcome.BanagerFailed.PanickedDetail";
        case "HomebrewSettingsChanged":
          return "operations.outcome.BanagerFailed.HomebrewSettingsChangedDetail";
        // Its sentence says it is an internal error and that nothing
        // changed; there is nothing to do next.
        case "Internal":
          return null;
        default: {
          const unhandled: never = fault;
          return unhandled;
        }
      }
    }
    if ("HomebrewStillUpdating" in fault) return "operations.outcome.BanagerFailed.HomebrewStillUpdatingDetail";
    if ("PathChanged" in fault) return "operations.outcome.BanagerFailed.PathChangedDetail";
    if ("FormulaChanged" in fault) return "operations.outcome.BanagerFailed.FormulaChangedDetail";
    if ("ProgramMissing" in fault || "SpawnFailed" in fault) return null;
    const unhandled: never = fault;
    return unhandled;
  }
  const unhandled: never = outcome;
  return unhandled;
}

/** Interpolation values for `operations.outcome.<outcomeKey(outcome)>`. */
export function outcomeArgs(outcome: Outcome): Record<string, unknown> {
  if (typeof outcome === "string") return {};
  if ("NeedsAttention" in outcome) return {};
  if ("Failed" in outcome) return { summary: outcome.Failed.summary.trim() };
  if ("BanagerFailed" in outcome) return faultArgs(outcome.BanagerFailed);
  const unhandled: never = outcome;
  return unhandled;
}

/**
 * The variant name of one of Banager's own failure reasons. Each `Fault`
 * variant needs a case here and a sentence in both locales under
 * `operations.outcome.BanagerFailed`: a reason that arrived with no words
 * would be this project's signature defect, defined and never rendered.
 */
function faultKey(fault: Fault): string {
  if (typeof fault === "string") {
    switch (fault) {
      case "Panicked":
      case "HomebrewSettingsChanged":
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
  if ("HomebrewStillUpdating" in fault) return "HomebrewStillUpdating";
  if ("PathChanged" in fault) return "PathChanged";
  if ("FormulaChanged" in fault) return "FormulaChanged";
  const unhandled: never = fault;
  return unhandled;
}

/** The data a `Fault` carries, for its sentence: a path, or the operating
 *  system's own reason -- never prose of Banager's. */
function faultArgs(fault: Fault): Record<string, unknown> {
  if (typeof fault === "string") return {};
  if ("ProgramMissing" in fault) return { program: fault.ProgramMissing.program };
  if ("SpawnFailed" in fault) return { detail: fault.SpawnFailed.detail };
  if ("HomebrewStillUpdating" in fault) return { minutes: fault.HomebrewStillUpdating.minutes };
  if ("PathChanged" in fault) return { path: fault.PathChanged.path };
  if ("FormulaChanged" in fault) return { name: fault.FormulaChanged.name };
  const unhandled: never = fault;
  return unhandled;
}

/**
 * How long ago something happened, in the one unit the page header says
 * it in: under a minute is "just now", then whole minutes, whole hours,
 * whole days, each rounded down. A time after `nowMs` -- a check that
 * finished after the clock the caller holds last ticked -- is "just now"
 * too, never a negative count.
 */
export type Elapsed =
  | { unit: "justNow" }
  | { unit: "minutes"; count: number }
  | { unit: "hours"; count: number }
  | { unit: "days"; count: number };

export function elapsedSince(thenSeconds: number, nowMs: number): Elapsed {
  const seconds = Math.floor(nowMs / 1000 - thenSeconds);
  if (seconds < 60) return { unit: "justNow" };
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return { unit: "minutes", count: minutes };
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return { unit: "hours", count: hours };
  return { unit: "days", count: Math.floor(hours / 24) };
}

/**
 * A byte count as the user reads it in Finder: 1000-based units, at most
 * one decimal, no trailing ".0". Units are symbols, not words, so they
 * are the same in both locales and this needs no `t()`.
 */
export function formatBytes(bytes: number): string {
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = bytes;
  let unit = 0;
  while (unit < units.length - 1 && value >= 1000) {
    value /= 1000;
    unit += 1;
  }
  // 999.97 KB rounds to "1000.0 KB" at one decimal; that is 1 MB.
  if (unit < units.length - 1 && Number(value.toFixed(1)) >= 1000) {
    value /= 1000;
    unit += 1;
  }
  const text = unit === 0 ? String(value) : value.toFixed(1).replace(/\.0$/, "");
  return `${text} ${units[unit]}`;
}
