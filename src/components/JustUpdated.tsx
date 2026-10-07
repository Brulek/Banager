import { FollowUpWarnings } from "./FollowUpWarnings";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { artifactKeyId } from "../store/ui";
import {
  FAILURE_CAUSE_KEYS,
  causeKeepsItsLine,
  failureDetail,
  faultFailure,
  outcomeCause,
  type FailureCause,
} from "../lib/failureCause";
import { ALREADY_UPDATED_KEYS, outcomeSentence, outcomeTone } from "../lib/operations";
import { calendarDaysBetween, shortDateText, shortTimeText } from "../lib/shortDate";
import type { AlreadyUpdated, ArtifactKey, FollowUpWarning, Attention, HistoryResult, OpSummary, Outcome } from "../lib/types";
import { PasswordRecovery } from "./PasswordRecovery";
import { InfoDetail } from "./InfoDetail";
import { OutcomeIcon } from "./OutcomeIcon";
import { ToolAvatar } from "./ToolAvatar";
import { BUTTON } from "./ui/controls";
import { GROUP } from "./ui/group";

/**
 * How an update 「最近的更新记录」 lists ended: it worked -- `already`
 * saying how, for one already at its new version when its turn came
 * (`AlreadyUpdated`, r6 y3-batch); it did not, with the cause in a word
 * where the tool's own words gave one (`failureCause`), and, kept by the
 * history where they gave none or where the cause's words point at them
 * (`causeKeepsItsLine`), the tool's first error line (`detail`); or
 * the tool said it worked and Banager found nothing changed, or could not
 * confirm it -- the row's 「结果不符」.
 */
export type JustUpdatedEnding =
  | { kind: "succeeded"; already?: AlreadyUpdated; warnings?: FollowUpWarning[] }
  | { kind: "failed"; cause: FailureCause | null; detail?: string }
  | { kind: "attention"; outcome: "Unconfirmed" | { NeedsAttention: Attention } };

/** `{ kind: "succeeded" }`, with how it was done where it was already done. */
function succeeded(already: AlreadyUpdated | null | undefined, warnings: FollowUpWarning[]): JustUpdatedEnding {
  return { kind: "succeeded", ...(already ? { already } : {}), ...(warnings.length ? { warnings } : {}) };
}

/**
 * The ending of an update this window ran, or null for one 「最近的更新记录」
 * does not list: one cancelled, or not finished. `alreadyUpdated` is its
 * summary's (`OpSummary.already_updated`).
 */
export function endingOfOutcome(
  outcome: Outcome | null,
  alreadyUpdated: AlreadyUpdated | null = null,
  warnings: FollowUpWarning[] = [],
): JustUpdatedEnding | null {
  if (outcome === null) return null;
  switch (outcomeTone(outcome)) {
    case "success":
      return succeeded(alreadyUpdated, warnings);
    case "failure": {
      // Banager's own failure, as the history keeps it (`faultFailure`).
      if (typeof outcome !== "string" && "BanagerFailed" in outcome) {
        const { cause, detail } = faultFailure(outcome.BanagerFailed);
        return detail === null ? { kind: "failed", cause } : { kind: "failed", cause, detail };
      }
      // A tool's words no cause names, or a cause whose words point at
      // them (`causeKeepsItsLine`): its first error line, as the history
      // keeps it (`failureDetail`).
      const cause = outcomeCause(outcome);
      const detail =
        (cause === null || causeKeepsItsLine(cause)) && typeof outcome !== "string" && "Failed" in outcome
          ? failureDetail(outcome.Failed.summary)
          : null;
      return detail === null ? { kind: "failed", cause } : { kind: "failed", cause, detail };
    }
    case "attention":
      return {
        kind: "attention",
        outcome: typeof outcome !== "string" && "NeedsAttention" in outcome ? outcome : "Unconfirmed",
      };
    case "cancelled":
      return null;
  }
}

/**
 * The ending of an update the history kept, or null for one cancelled
 * (`listedResult`). `alreadyUpdated` is the record's
 * (`HistoryRecord.already_updated`).
 */
export function endingOfRecord(
  result: HistoryResult,
  alreadyUpdated: AlreadyUpdated | null = null,
  warnings: FollowUpWarning[] = [],
): JustUpdatedEnding | null {
  if (result === "Succeeded") return succeeded(alreadyUpdated, warnings);
  if (result === "Cancelled") return null;
  if (result === "Unconfirmed") return { kind: "attention", outcome: "Unconfirmed" };
  if ("NeedsAttention" in result) return { kind: "attention", outcome: result };
  const { cause, detail } = result.Failed;
  return detail ? { kind: "failed", cause, detail } : { kind: "failed", cause };
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
   * a model's is a digest, and beside 「未能更新」 or what did not add up
   * a version would read as the one it was updated to.
   */
  version: string | null;
  /** When it finished, in milliseconds, or null for one this window did not see finish. */
  finishedAt: number | null;
  /**
   * Whether Banager read the installed version before the update and after
   * it and the two differ (`HistoryRecord.verified`): said in the tooltip
   * of the row's own 「已更新」, which every update that worked says.
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
 * after a restart, shows View Steps for a recorded password stop --
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
  const sameDay = calendarDaysBetween(then, new Date(now)) === 0;
  const text = sameDay ? shortTimeText(then, language) : shortDateText(then, language);
  const title = new Intl.DateTimeFormat(language, { dateStyle: "medium", timeStyle: "short" }).format(then);
  return { text, title, today: sameDay };
}

/**
 * How a line says its update ended, in 11 after the outcome's 12 sign
 * (`OutcomeIcon`): the ✓ and 「已更新」 the row showed, or 「已确认更新」 where
 * Banager read the version change for itself -- or, for one already at its
 * new version when its turn came, how: 「已由前面的更新一并完成」, why behind
 * an ⓘ; the red ⚠︎ and 「未能更新」,
 * with the cause where the tool's words gave one --
 * 「未能更新：网络连接失败」 -- and what to do about it behind an ⓘ, or, where
 * they gave none, 「原因：」 and the first line of the tool's error the
 * history kept, behind the ⓘ (r6 y3-batch); the
 * orange ⚠︎ and what did not add up, in the outcome's own words
 * (「结果未确认」) -- but an update whose version Banager read unchanged
 * after it says what that means, 「没有更新成功：版本没有变」, and what it
 * read behind an ⓘ: 「显示已更新」 left a person asking whether it had
 * (walk-2 W2-12). Words, not colour, tell them apart.
 */
function EndingWords({ entry }: { entry: JustUpdatedEntry }) {
  const { t } = useTranslation();
  const { ending } = entry;
  let tone: "success" | "failure" | "attention";
  let words: string;
  let why: string | undefined;
  switch (ending.kind) {
    case "succeeded":
      tone = ending.warnings?.length ? "attention" : "success";
      if (ending.warnings?.length) {
        words = t("followUpWarning.succeeded");
        break;
      }
      if (ending.already) {
        // Already at its new version when its turn came: done, and how.
        words = t(ALREADY_UPDATED_KEYS[ending.already].word);
        why = t(ALREADY_UPDATED_KEYS[ending.already].why);
        break;
      }
      // 「已更新」, as the row and the operation bar say it (walk-3 W3-19);
      // that the version was read before and after is behind its ⓘ.
      words = t("updates.progress.succeeded");
      why = entry.verified
        ? t(entry.key.kind === "Model" ? "history.verifiedModelTitle" : "history.verifiedTitle")
        : undefined;
      break;
    case "failed":
      tone = "failure";
      words =
        ending.cause === null
          ? t("updates.progress.failed")
          : t("history.failedBecause", { cause: t(FAILURE_CAUSE_KEYS[ending.cause].word) });
      // The cause's line, and the tool's own where the line points at it
      // (「报错里写着是哪个文件」): the log that had it is gone.
      why =
        ending.cause !== null
          ? ending.detail
            ? t("runtimeGuard.then", {
                first: t(FAILURE_CAUSE_KEYS[ending.cause].line),
                then: t("batchResult.errorLine", { detail: ending.detail }),
              })
            : t(FAILURE_CAUSE_KEYS[ending.cause].line)
          : ending.detail
            ? t("batchResult.reason", { detail: ending.detail })
            : undefined;
      break;
    case "attention":
      tone = "attention";
      // What did not add up, in its own plain words, rather than the row's
      // short 「结果不符」, which says nothing here, where there is no log
      // one click away. A version read unchanged after the update is said
      // as what it means: it did not update.
      if (typeof ending.outcome !== "string" && ending.outcome.NeedsAttention === "UnchangedAfterUpgrade") {
        words = t("history.unchanged");
        why = t("history.unchangedTitle");
      } else {
        words = outcomeSentence(t, ending.outcome, "Upgrade");
        why = undefined;
      }
      break;
  }
  return (
    <span
      data-just-updated-ending={ending.kind}
      className="inline-flex shrink-0 items-center gap-1 whitespace-nowrap text-small text-foreground"
    >
      <OutcomeIcon tone={tone} size={12} />
      {words}
      {/* The why behind an ⓘ, not a tooltip: a keyboard and a screen
          reader reach it as a pointer does (decision I21e). Named by the
          line's tool, as every line may say the same words. */}
      {why === undefined ? null : (
        <InfoDetail label={t("common.detailsLabel", { title: entry.name })}>{why}</InfoDetail>
      )}
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
 * ended in 11 (`EndingWords`: 「已更新」 or 「已确认更新」, 「未能更新」 with
 * its cause, 「没有更新成功：版本没有变」, or what else did not add up), and
 * when it finished, 11 muted. Nothing to
 * select or press but an ending's ⓘ, View Steps for a recorded Homebrew
 * password stop, Clear, which hides what it lists,
 * after a restart too, until the next update ends, and, past
 * `JUST_UPDATED_SHOWN` lines, the "N More" line that shows the rest; it
 * is no part of the page's count or of Select all.
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
              {/* The source, for a screen reader: in sight the line shows
                  it only as the 10 mark on the corner of a tool's neutral
                  tile, and not at all beside a tool's own logo or app
                  icon, so two copies of one tool, updated from two
                  sources, are otherwise two lines alike. */}
              <span data-just-updated-source="" className="sr-only">
                {entry.sourceLabel}
              </span>
              {/* The version and the time each take a column, with or
                  without one, so that the ticks line up down the list. */}
              <span className="min-w-20 shrink-0 whitespace-nowrap text-right text-small tabular-nums text-muted">
                {entry.version}
              </span>
              <EndingWords entry={entry} />
              {entry.ending.kind === "succeeded" && entry.ending.warnings?.length ? (
                <FollowUpWarnings warnings={entry.ending.warnings} opId={entry.opId} name={entry.name} />
              ) : null}
              {entry.opId === null && entry.adapterId === "brew" && entry.ending.kind === "failed" && entry.ending.cause === "needsPassword" ? (
                <PasswordRecovery artifactKey={entry.key} name={entry.name} />
              ) : null}
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
