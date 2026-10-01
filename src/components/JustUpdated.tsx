import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { artifactKeyId } from "../store/ui";
import { FAILURE_CAUSE_KEYS, outcomeCause, type FailureCause } from "../lib/failureCause";
import { outcomeSentence, outcomeTone } from "../lib/operations";
import type { ArtifactKey, Attention, HistoryResult, OpSummary, Outcome } from "../lib/types";
import { OutcomeIcon } from "./OutcomeIcon";
import { ToolAvatar } from "./ToolAvatar";
import { BUTTON } from "./ui/controls";
import { GROUP } from "./ui/group";

/**
 * How an update 「最近的更新记录」 lists ended: it worked; it did not, with the
 * cause in a word where the tool's own words gave one (`failureCause`); or
 * the tool said it worked and Banager found nothing changed, or could not
 * confirm it -- the row's 「结果不符」.
 */
export type JustUpdatedEnding =
  | { kind: "succeeded" }
  | { kind: "failed"; cause: FailureCause | null }
  | { kind: "attention"; outcome: "Unconfirmed" | { NeedsAttention: Attention } };

/**
 * The ending of an update this window ran, or null for one 「最近的更新记录」
 * does not list: one cancelled, or not finished.
 */
export function endingOfOutcome(outcome: Outcome | null): JustUpdatedEnding | null {
  if (outcome === null) return null;
  switch (outcomeTone(outcome)) {
    case "success":
      return { kind: "succeeded" };
    case "failure":
      return { kind: "failed", cause: outcomeCause(outcome) };
    case "attention":
      return {
        kind: "attention",
        outcome: typeof outcome !== "string" && "NeedsAttention" in outcome ? outcome : "Unconfirmed",
      };
    case "cancelled":
      return null;
  }
}

/** The ending of an update the history kept, or null for one cancelled (`listedResult`). */
export function endingOfRecord(result: HistoryResult): JustUpdatedEnding | null {
  if (result === "Succeeded") return { kind: "succeeded" };
  if (result === "Cancelled") return null;
  if (result === "Unconfirmed") return { kind: "attention", outcome: "Unconfirmed" };
  if ("NeedsAttention" in result) return { kind: "attention", outcome: result };
  return { kind: "failed", cause: result.Failed.cause };
}

/** One update the Updates page's "Just updated" lists, as it shows it. */
export interface JustUpdatedEntry {
  /** Its React key: unique across this window's operations and the history's. */
  id: string;
  /**
   * The update's operation in this window, which Clear takes off, or null
   * for one the history kept from before (src/lib/history.ts).
   */
  opId: number | null;
  key: ArtifactKey;
  /** The source's adapter id and name, for the avatar. */
  adapterId: string;
  sourceLabel: string;
  /** The name its row had. */
  name: string;
  /**
   * The version it has now, or null where there is none to show honestly:
   * a model's is a digest, and beside 「未能更新」 or 「结果不符」 a version
   * would read as the one it was updated to.
   */
  version: string | null;
  /** When it finished, in milliseconds, or null for one this window did not see finish. */
  finishedAt: number | null;
  /**
   * Whether Banager read the installed version before the update and after
   * it and the two differ (`HistoryRecord.verified`): 「已核实」 then, and
   * otherwise the row's 「已更新」.
   */
  verified: boolean;
  /** How it ended (`JustUpdatedEnding`). */
  ending: JustUpdatedEnding;
}

export interface JustUpdatedFilter {
  /** The operations a row still shows, with its tick where its Update button was (`useUpdateOperationFor`). */
  shownInRows: ReadonlySet<number>;
  /** What Clear took off (`clearedJustUpdated`). */
  cleared: readonly number[];
  /** When each operation finished (`opFinishedAt`), for the order. */
  finishedAt: Readonly<Record<number, number>>;
}

/**
 * The updates "Just updated" lists, newest first: every tool whose newest
 * operation is a finished update that was not cancelled (`endingOfOutcome`)
 * -- this session's, since the backend lists every operation the session
 * has run. One that worked, and also one that failed or asks to be
 * checked: a person should know of those too, and an update that failed
 * and then worked is listed as the one that worked. Not one cancelled,
 * nor a tool uninstalled since, whose newest operation is the uninstall.
 *
 * Only once its row no longer shows it: until the check after it lands, a
 * finished update's row stays, with its tick where its Update button was,
 * and then the tick moves here -- never in both places, never in neither.
 * One that failed or asks to be checked keeps its row, with its outcome,
 * its log and Retry, for as long as that update is still offered; it is
 * listed here only once the row has gone. (A failure the history kept
 * from an earlier launch is listed beside its row on purpose: that row,
 * after a restart, is a plain update that does not know the last try --
 * `recentUpdates` in src/lib/history.ts.)
 * Nothing Clear took off; the order is by when each finished, and an
 * update this window did not see finish -- one from before it was opened
 * -- comes after the ones it did, newest first by its number.
 */
export function justUpdatedOps(operations: readonly OpSummary[], filter: JustUpdatedFilter): OpSummary[] {
  const newest = new Map<string, OpSummary>();
  for (const op of operations) {
    const id = artifactKeyId({ instance_id: op.instance_id, kind: op.artifact_kind, name: op.name });
    const seen = newest.get(id);
    if (seen === undefined || op.id > seen.id) newest.set(id, op);
  }
  const cleared = new Set(filter.cleared);
  const finished = (op: OpSummary) => filter.finishedAt[op.id] ?? Number.NEGATIVE_INFINITY;
  return [...newest.values()]
    .filter(
      (op) =>
        op.kind === "Upgrade" &&
        op.status === "Done" &&
        endingOfOutcome(op.outcome) !== null &&
        !filter.shownInRows.has(op.id) &&
        !cleared.has(op.id),
    )
    .sort((a, b) => {
      const byTime = finished(b) - finished(a);
      // Two unknown times make NaN, which says nothing either way.
      return Number.isNaN(byTime) || byTime === 0 ? b.id - a.id : byTime;
    });
}

/**
 * When an update finished, in the fewest words that stay true: the time
 * -- 「06:38」, "6:38 AM" -- on the day it is read, which the line says
 * as 「今天06:38」 (`today`: text-autospace draws the gap, or on macOS 13.3-15.3
 * the autospace post-processor puts one in, src/i18n/autospace.ts), and the date -- 「9月28日」, "Sep 28" -- on
 * any other, since a window left open overnight still lists yesterday's,
 * and the history lists the last 30 days. Both in full in the `title`.
 */
export function finishedText(
  finishedAt: number,
  now: number,
  language: string,
): { text: string; title: string; today: boolean } {
  const then = new Date(finishedAt);
  const today = new Date(now);
  const sameDay =
    then.getFullYear() === today.getFullYear() &&
    then.getMonth() === today.getMonth() &&
    then.getDate() === today.getDate();
  const text = sameDay
    ? new Intl.DateTimeFormat(language, { timeStyle: "short" }).format(then)
    : new Intl.DateTimeFormat(language, { month: "short", day: "numeric" }).format(then);
  const title = new Intl.DateTimeFormat(language, { dateStyle: "medium", timeStyle: "short" }).format(then);
  return { text, title, today: sameDay };
}

/**
 * How a line says its update ended, in 11 after the outcome's 12 sign
 * (`OutcomeIcon`): the ✓ and 「已更新」 the row showed, or 「已核实」 where
 * Banager read the version change for itself; the red ⚠︎ and 「未能更新」,
 * with the cause where the tool's words gave one --
 * 「未能更新：网络连接失败」 -- and what to do about it in the `title`; the
 * orange ⚠︎ and what did not add up, in the outcome's own words
 * (「显示已更新，但版本没有变化」). Words, not colour, tell them apart.
 */
function EndingWords({ entry }: { entry: JustUpdatedEntry }) {
  const { t } = useTranslation();
  const { ending } = entry;
  let tone: "success" | "failure" | "attention";
  let words: string;
  let title: string | undefined;
  switch (ending.kind) {
    case "succeeded":
      tone = "success";
      words = entry.verified ? t("history.verified") : t("updates.progress.succeeded");
      title = entry.verified
        ? t(entry.key.kind === "Model" ? "history.verifiedModelTitle" : "history.verifiedTitle")
        : undefined;
      break;
    case "failed":
      tone = "failure";
      words =
        ending.cause === null
          ? t("updates.progress.failed")
          : t("history.failedBecause", { cause: t(FAILURE_CAUSE_KEYS[ending.cause].word) });
      title = ending.cause === null ? undefined : t(FAILURE_CAUSE_KEYS[ending.cause].line);
      break;
    case "attention":
      tone = "attention";
      // What did not add up, in its own plain words -- 「显示已更新，但版本没有
      // 变化」 -- rather than the row's short 「结果不符」, which says nothing
      // here, where there is no log one click away.
      words = outcomeSentence(t, ending.outcome);
      title = undefined;
      break;
  }
  return (
    <span
      title={title}
      data-just-updated-ending={ending.kind}
      className="inline-flex shrink-0 items-center gap-1 whitespace-nowrap text-small text-foreground"
    >
      <OutcomeIcon tone={tone} size={12} />
      {words}
    </span>
  );
}

/**
 * How many lines 「最近的更新记录」 shows before the rest fold away under an
 * "N More" line: 30 days of updates can be dozens, and the list ends
 * with them.
 */
export const JUST_UPDATED_SHOWN = 6;

export interface JustUpdatedProps {
  entries: JustUpdatedEntry[];
  onClear: () => void;
}

/**
 * 「最近的更新记录」: the tools updated this session, and those the history
 * kept from the last 30 days (src/lib/history.ts), under the updates still
 * to install on the Updates page (at its top when there are none), as the
 * App Store's Update History is under Pending, so that an update does
 * not simply vanish from the list once it has ended -- nor after a
 * restart: one that worked, and one that did not or asks to be checked. A grouped container (spec §3.10) under its title -- 13 bold, with
 * a small grey Clear beside it -- of quiet lines, not rows: 28 high, the
 * 20 icon, the name in 13, the version it has now in 11 muted, how it
 * ended in 11 (`EndingWords`: 「已更新」 or 「已核实」, 「未能更新」 with
 * its cause, or 「结果不符」), and when it finished, 11 muted. Nothing to
 * select or press but Clear, which hides what it lists, after a restart
 * too, until the next update ends, and, past `JUST_UPDATED_SHOWN`
 * lines, the "N More" line that shows the rest; it is no part of the
 * page's count or of Select all.
 */
export function JustUpdated({ entries, onClear }: JustUpdatedProps) {
  const { t, i18n } = useTranslation();
  const headingId = useId();
  const listId = useId();
  const now = Date.now();
  const [expanded, setExpanded] = useState(false);
  const folds = entries.length > JUST_UPDATED_SHOWN;
  const shown = folds && !expanded ? entries.slice(0, JUST_UPDATED_SHOWN) : entries;
  return (
    <section aria-labelledby={headingId} data-just-updated="">
      <div className="mb-2 flex items-center gap-2 px-2.5">
        <h2 id={headingId} className="text-title text-foreground">
          {t("updates.justUpdated.title")}
        </h2>
        <button
          type="button"
          onClick={onClear}
          aria-label={t("updates.justUpdated.clearLabel")}
          className={BUTTON.small.grey}
        >
          {t("updates.justUpdated.clear")}
        </button>
      </div>
      <ul id={listId} aria-labelledby={headingId} className={`py-1 ${GROUP}`}>
        {shown.map((entry) => {
          const finished =
            entry.finishedAt === null ? null : finishedText(entry.finishedAt, now, i18n.language);
          return (
            <li key={entry.id} className="flex h-7 items-center gap-2 px-2.5">
              <ToolAvatar
                size="compact"
                adapterId={entry.adapterId}
                sourceLabel={entry.sourceLabel}
                iconKey={entry.key}
              />
              <span title={entry.name} className="min-w-0 flex-1 truncate text-body text-foreground">
                {entry.name}
              </span>
              {/* The source, which the avatar's mark is all the line shows
                  of, for a screen reader: two copies of one tool, updated
                  from two sources, are otherwise two lines alike. */}
              <span data-just-updated-source="" className="sr-only">
                {entry.sourceLabel}
              </span>
              {/* The version and the time each take a column, with or
                  without one, so that the ticks line up down the list. */}
              <span className="min-w-20 shrink-0 whitespace-nowrap text-right text-small tabular-nums text-muted">
                {entry.version}
              </span>
              <EndingWords entry={entry} />
              <span className="w-24 shrink-0 whitespace-nowrap text-right text-small tabular-nums text-muted">
                {entry.finishedAt !== null && finished !== null ? (
                  <time dateTime={new Date(entry.finishedAt).toISOString()} title={finished.title}>
                    {finished.today ? t("history.today", { time: finished.text }) : finished.text}
                  </time>
                ) : null}
              </span>
            </li>
          );
        })}
      </ul>
      {folds ? (
        <button
          type="button"
          aria-expanded={expanded}
          aria-controls={listId}
          onClick={() => setExpanded(!expanded)}
          className="mt-1 ml-2.5 inline-flex items-center rounded-sm text-small text-muted"
        >
          {expanded
            ? t("history.showFewer")
            : t("history.more", { count: entries.length - JUST_UPDATED_SHOWN })}
        </button>
      ) : null}
    </section>
  );
}
