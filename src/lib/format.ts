import type { Attention, Fault, HistoryResult, Outcome } from "./types";
import type { FailureCause } from "./failureCause";

/**
 * `names` as a sentence lists them, in the user's language: 「Homebrew、npm
 * 和 uv」, "Homebrew, npm and uv" -- or, `"or"`, any one of them: "typing
 * node or npm" (r21 C6). One name alone; none, nothing. `t` is whatever
 * `useTranslation()` gives.
 */
export function namesInSentence(
  t: (key: string, options?: Record<string, string>) => string,
  names: readonly string[],
  joiner: "and" | "or" = "and",
): string {
  if (names.length <= 1) return names[0] ?? "";
  return t(joiner === "or" ? "common.listOr" : "common.listAnd", {
    list: names.slice(0, -1).join(t("common.listSeparator")),
    last: names[names.length - 1],
  });
}

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
 *  found; `UpdatedButStepFailed`, an update installed though its tool
 *  failed after it, with the version it moved to, or, for a model, none
 *  (`UpdatedButStepFailedNoVersion`), and, where the step was Homebrew's
 *  link (cause `notLinked`), `UpdatedButNotLinked`, the new version not
 *  linked (skeptic of r35 U2, 1) -- listed for the same reason as
 *  `faultKey`'s: each needs a sentence in both locales under
 *  `operations.outcome.NeedsAttention`, and a template string would take a
 *  new one without a word. */
function attentionKey(attention: Attention): string {
  if (typeof attention !== "string") {
    if ("UpdatedButStepFailed" in attention) {
      const { version, cause } = attention.UpdatedButStepFailed;
      if (version === null) return "UpdatedButStepFailedNoVersion";
      return cause === "notLinked" ? "UpdatedButNotLinked" : "UpdatedButStepFailed";
    }
    const unhandled: never = attention;
    return unhandled;
  }
  switch (attention) {
    case "NotInstalledAfterInstall":
    case "StillInstalledAfterUninstall":
    case "GoneBeforeUpgrade":
    case "GoneAfterUpgrade":
    case "UnchangedAfterUpgrade":
    case "BackAfterUninstall":
    case "NotLinkedAfterLink":
      return attention;
    default: {
      const unhandled: never = attention;
      return unhandled;
    }
  }
}

/**
 * Whether an update ended installed though its tool failed after the
 * version it reads had moved (`Attention::UpdatedButStepFailed`, r35 U2):
 * its row is held as one that worked, it is out of every count, and
 * 「最近的更新记录」 lists it whatever the last check offers -- an outcome of
 * this window's or a result the history kept, the same.
 */
export function isUpdatedButStepFailed(outcome: Outcome | HistoryResult | null): boolean {
  return stepFailedOf(outcome) !== null;
}

/**
 * The version an update installed though a step after it failed moved to,
 * as Banager read it after the update (`Attention::UpdatedButStepFailed`);
 * null for any other outcome, and for a model, whose digest is never shown.
 */
export function stepFailedVersion(outcome: Outcome | HistoryResult | null): string | null {
  return stepFailedOf(outcome)?.version ?? null;
}

/**
 * What an update installed though a step after it failed carries
 * (`Attention::UpdatedButStepFailed`): the version it moved to, the
 * tool's failure's cause, and its first error line -- an outcome of this
 * window's or a result the history kept, the same; null for any other.
 * A cause or a line a record written before they were kept lacks is null.
 */
export function stepFailedOf(
  outcome: Outcome | HistoryResult | null,
): { version: string | null; cause: FailureCause | null; detail: string | null } | null {
  if (outcome === null || typeof outcome === "string" || !("NeedsAttention" in outcome)) return null;
  const attention = outcome.NeedsAttention;
  if (typeof attention === "string" || !("UpdatedButStepFailed" in attention)) return null;
  const { version, cause, detail } = attention.UpdatedButStepFailed;
  return { version, cause: cause ?? null, detail: detail ?? null };
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
    // The new version is in, the log says which step failed: never Retry,
    // which would install nothing and run no step. Its link, where that
    // was the step: what that means in Terminal, and that the log says
    // which file is in the way.
    if (typeof outcome.NeedsAttention !== "string") {
      if ("UpdatedButStepFailed" in outcome.NeedsAttention) {
        return outcome.NeedsAttention.UpdatedButStepFailed.cause === "notLinked"
          ? "operations.outcome.NeedsAttention.UpdatedButNotLinkedDetail"
          : "operations.outcome.NeedsAttention.UpdatedButStepFailedDetail";
      }
      const unhandled: never = outcome.NeedsAttention;
      return unhandled;
    }
    switch (outcome.NeedsAttention) {
      case "UnchangedAfterUpgrade":
        return "operations.outcome.NeedsAttention.UnchangedAfterUpgradeDetail";
      case "BackAfterUninstall":
        return "operations.outcome.NeedsAttention.BackAfterUninstallDetail";
      case "NotLinkedAfterLink":
        return "operations.outcome.NeedsAttention.NotLinkedAfterLinkDetail";
      case "NotInstalledAfterInstall":
      case "StillInstalledAfterUninstall":
      case "GoneBeforeUpgrade":
      case "GoneAfterUpgrade":
        return null;
      default: {
        const unhandled: never = outcome.NeedsAttention;
        return unhandled;
      }
    }
  }
  // The tool's own words are the sentence; a tool that said nothing gets
  // how to try again, then Copy Log (`outcomeStepKey`).
  if ("Failed" in outcome) return outcome.Failed.summary.trim() ? null : "operations.outcome.FailedSilentDetail";
  if ("BanagerFailed" in outcome) {
    const fault = outcome.BanagerFailed;
    if (typeof fault === "string") {
      switch (fault) {
        case "Panicked":
          return "operations.outcome.BanagerFailed.PanickedDetail";
        case "ChangedSinceShown":
          return "operations.outcome.BanagerFailed.ChangedSinceShownDetail";
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
    if ("LinkRollbackRisk" in fault) return "operations.outcome.BanagerFailed.LinkRollbackRiskDetail";
    if ("LinkTaken" in fault) return "operations.outcome.BanagerFailed.LinkTakenDetail";
    if ("ProgramMissing" in fault || "SpawnFailed" in fault) return null;
    const unhandled: never = fault;
    return unhandled;
  }
  const unhandled: never = outcome;
  return unhandled;
}

/**
 * The steps that end at Copy Log, in the house shape of a failure's step
 * under the log (`failureSteps.log.generic`): what to try once more --
 * Retry on the row, Fix… on the notice, or `{{again}}` by the kind of
 * operation (`TRY_AGAIN_KEYS`) -- then 「拷贝日志」, to send to someone who
 * can help. A sentence that only pointed at "the operation log" was said
 * over that very log, with nothing after it (r24 W4).
 */
const ENDS_AT_COPY_LOG: ReadonlySet<string> = new Set([
  "operations.outcome.NeedsAttention.UnchangedAfterUpgradeDetail",
  "operations.outcome.NeedsAttention.NotLinkedAfterLinkDetail",
  "operations.outcome.FailedSilentDetail",
]);

/**
 * What the log window says to do next about `outcome` (`outcomeDetailKey`),
 * for a log that has lines or none (`logHasLines`). Copy Log is off over a
 * log with no line -- a tool that printed nothing, or a window reloaded
 * since -- so a step that ends at it says only how to try again there.
 */
export function outcomeStepKey(outcome: Outcome, logHasLines: boolean): string | null {
  const key = outcomeDetailKey(outcome);
  if (key !== null && !logHasLines && ENDS_AT_COPY_LOG.has(key)) return "operations.outcome.emptyLogDetail";
  // An update installed though a step after it failed has nothing to try
  // again, and over a log with no line it has no lines to point at either.
  if (
    (key === "operations.outcome.NeedsAttention.UpdatedButStepFailedDetail" ||
      key === "operations.outcome.NeedsAttention.UpdatedButNotLinkedDetail") &&
    !logHasLines
  ) {
    return null;
  }
  return key;
}

/** Interpolation values for `operations.outcome.<outcomeKey(outcome)>`. */
export function outcomeArgs(outcome: Outcome): Record<string, unknown> {
  if (typeof outcome === "string") return {};
  if ("NeedsAttention" in outcome) {
    const attention = outcome.NeedsAttention;
    // The version the update moved to; none for a model's digest.
    if (typeof attention !== "string" && attention.UpdatedButStepFailed.version !== null) {
      return { version: attention.UpdatedButStepFailed.version };
    }
    return {};
  }
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
      case "ChangedSinceShown":
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
  if ("LinkRollbackRisk" in fault) return "LinkRollbackRisk";
  // y1-keg: the first path the sentence names, and how many more.
  if ("LinkTaken" in fault) return fault.LinkTaken.paths.length > 1 ? "LinkTakenMany" : "LinkTaken";
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
  if ("LinkRollbackRisk" in fault) return { name: fault.LinkRollbackRisk.name };
  if ("LinkTaken" in fault) {
    const { name, paths } = fault.LinkTaken;
    return { name, path: paths[0] ?? "", number: paths.length, others: paths.length - 1 };
  }
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
