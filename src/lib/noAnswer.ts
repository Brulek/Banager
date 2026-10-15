/**
 * Why a source did not answer, in words (`InstanceStatus.no_answer`,
 * `NoAnswer` in crates/banager-core/src/model.rs), and the fix a source's
 * notice offers for it. Finding (1) of the 2026-10-07 run: npm's launcher,
 * `#!/usr/bin/env node`, found no `node` after `brew upgrade node@22` could
 * not link its new version (npm's own self-update had put its `npm` where
 * the formula's link was), and every page said only 「npm没有响应」 -- "not
 * responding", which is true only of a source that ran and did not answer
 * in time.
 *
 * A source that ran out of time (`TimedOut`), or whose reason the core
 * could not tell, keeps the "not responding" words it always had. One that
 * could not start, or ran and failed, is said as that: its notice's title
 * and sentence (`noAnswerNotice`), its rows' chip detail (`noAnswerRow`)
 * and its state word in Diagnostics and Check Tool Setup
 * (`NO_ANSWER_WORDS`). A source whose launcher could not find a program a
 * keg-only Homebrew formula has (`link_fixes`) is offered Fix…, which
 * previews `brew link --formula --force <formula>` (`LinkFixSheet`).
 *
 * Only for a package manager's source -- the ones whose `detect` says why
 * (Homebrew, npm, pipx, uv, pip, Cargo). Ollama's models and a tool with
 * its own installer keep their own notices' words. No `t()` here, as in
 * src/lib/sources.ts, so the rule can be tested on its own.
 */
import type { LinkFix, ManagerInstance, NoAnswer, NoAnswerKind } from "./types";
import type { SourceNoticeSpec } from "./sources";

/** The reasons said in words of their own: all but `TimedOut`. */
type SaidKind = Exclude<NoAnswerKind, "TimedOut">;

/**
 * Why `instance` did not answer, where that is more than "not
 * responding": it is `NotResponding`, the core says why, and it is not that
 * it ran out of time. `null` otherwise, and for Ollama and a tool with its
 * own installer, whose notices say what they say of themselves.
 */
export function saidNoAnswer(instance: ManagerInstance): (NoAnswer & { kind: SaidKind }) | null {
  if (instance.status.unavailable !== "NotResponding") return null;
  if (instance.adapter_id === "ollama" || instance.adapter_id.startsWith("standalone-")) return null;
  const why = instance.status.no_answer ?? null;
  if (why === null || why.kind === "TimedOut") return null;
  return { ...why, kind: why.kind };
}

/** The notice titles, by reason. */
export const NO_ANSWER_TITLE_KEYS: Record<SaidKind, string> = {
  CouldNotStart: "noAnswer.title.CouldNotStart",
  ExitedWithError: "noAnswer.title.ExitedWithError",
};

/** A source's state in a word, by reason (`sourceStateWords`, src/lib/diagnostics.ts). */
export const NO_ANSWER_WORDS: Record<SaidKind, string> = {
  CouldNotStart: "noAnswer.statusWord.CouldNotStart",
  ExitedWithError: "noAnswer.statusWord.ExitedWithError",
};

/**
 * The notice a source that could not start or ran and failed needs, in
 * place of "not responding" (`sourceNoticesFor`), or `null` for any other.
 * Its sentence says why, then -- over its rows, as "not responding" does
 * -- that they are its last answer. A missing program a formula has is
 * named with the newest of those formulae, and the notice's button is
 * Fix… (`linkFix`); otherwise Check Again, which is what can help.
 */
export function noAnswerNotice(
  instance: ManagerInstance,
  sourceLabel: string,
  installedCount: number,
): SourceNoticeSpec | null {
  const why = saidNoAnswer(instance);
  if (why === null) return null;
  const fix = why.link_fixes[0];
  const sentence: Sentence =
    why.missing_program === null ? why.kind : fix === undefined ? "missing" : "missingFix";
  const keys = DESCRIPTION_KEYS[sentence];
  return {
    // The same id as the "not responding" notice it takes the place of.
    id: `${instance.id}:unreachable`,
    variant: "warning",
    diagnostic: why.diagnostic,
    diagnosticCause: why.cause,
    titleKey: NO_ANSWER_TITLE_KEYS[why.kind],
    descriptionKey: installedCount > 0 ? keys.withRows : keys.withoutRows,
    values: {
      source: sourceLabel,
      ...(why.missing_program === null ? {} : { program: why.missing_program }),
      ...(fix === undefined ? {} : { formula: fix.key.name }),
      ...(installedCount > 0 ? { count: installedCount } : {}),
    },
    action:
      fix === undefined
        ? { id: "checkAgain", labelKey: "header.checkAgain" }
        : { id: "linkFix", labelKey: "noAnswer.fix", instanceId: instance.id },
  };
}

/** Which sentence a notice says: of a missing program, with or without a formula that has it, or of its reason. */
type Sentence = SaidKind | "missing" | "missingFix";

/** Each sentence, over no rows and over the source's last answer's rows (`count`). */
const DESCRIPTION_KEYS: Record<Sentence, { withoutRows: string; withRows: string }> = {
  missing: {
    withoutRows: "noAnswer.description.missing",
    withRows: "noAnswer.description.missingWithRows",
  },
  missingFix: {
    withoutRows: "noAnswer.description.missingFix",
    withRows: "noAnswer.description.missingFixWithRows",
  },
  CouldNotStart: {
    withoutRows: "noAnswer.description.CouldNotStart",
    withRows: "noAnswer.description.CouldNotStartWithRows",
  },
  ExitedWithError: {
    withoutRows: "noAnswer.description.ExitedWithError",
    withRows: "noAnswer.description.ExitedWithErrorWithRows",
  },
};

/** A row's detail, by reason; a missing program's names it. */
const ROW_KEYS: Record<SaidKind | "missing", string> = {
  missing: "noAnswer.row.missing",
  CouldNotStart: "noAnswer.row.CouldNotStart",
  ExitedWithError: "noAnswer.row.ExitedWithError",
};

/**
 * A row's "Can't update now" detail for a source that could not start or
 * ran and failed (`unavailableDetail`, src/components/updateDetails.tsx),
 * or `null` for any other: 「npm无法运行：找不到它需要的node。」
 */
export function noAnswerRow(
  instance: ManagerInstance | undefined,
): { key: string; values: Record<string, string> } | null {
  const why = instance === undefined ? null : saidNoAnswer(instance);
  if (why === null) return null;
  return why.missing_program === null
    ? { key: ROW_KEYS[why.kind], values: {} }
    : { key: ROW_KEYS.missing, values: { program: why.missing_program } };
}

/** The formulae a source's reason offers to link, newest first; none for any other. */
export function linkFixesOf(instance: ManagerInstance | undefined): LinkFix[] {
  const why = instance === undefined ? null : saidNoAnswer(instance);
  return why === null || why.missing_program === null ? [] : why.link_fixes;
}
