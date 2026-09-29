import { useId } from "react";
import { useTranslation } from "react-i18next";
import { artifactKeyId } from "../store/ui";
import type { ArtifactKey, OpSummary } from "../lib/types";
import { CheckIcon } from "./icons";
import { ToolAvatar } from "./ToolAvatar";
import { BUTTON } from "./ui/controls";
import { GROUP } from "./ui/group";

/** One update the Updates page's "Just updated" lists, as it shows it. */
export interface JustUpdatedEntry {
  /** The update's operation. */
  opId: number;
  key: ArtifactKey;
  /** The source's adapter id and name, for the avatar. */
  adapterId: string;
  sourceLabel: string;
  /** The name its row had. */
  name: string;
  /** The version it has now, or null where there is none to show honestly: a model's is a digest. */
  version: string | null;
  /** When it finished, in milliseconds, or null for one this window did not see finish. */
  finishedAt: number | null;
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
 * operation is an update that succeeded -- this session's, since the
 * backend lists every operation the session has run. One that failed,
 * was cancelled or asks to be checked keeps its row, with its outcome and
 * its log, and is not listed here; neither is a tool uninstalled since,
 * whose newest operation is the uninstall.
 *
 * Only once its row no longer shows it: until the check after it lands, a
 * finished update's row stays, with its tick where its Update button was,
 * and then the tick moves here -- never in both places, never in neither.
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
        op.outcome === "Succeeded" &&
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
 * -- 「06:38」, "6:38 AM" -- on the day it is read, and the date on any
 * other, since a window left open overnight still lists yesterday's.
 * Both in full in the `title`.
 */
export function finishedText(finishedAt: number, now: number, language: string): { text: string; title: string } {
  const then = new Date(finishedAt);
  const today = new Date(now);
  const sameDay =
    then.getFullYear() === today.getFullYear() &&
    then.getMonth() === today.getMonth() &&
    then.getDate() === today.getDate();
  const text = sameDay
    ? new Intl.DateTimeFormat(language, { timeStyle: "short" }).format(then)
    : new Intl.DateTimeFormat(language, { month: "numeric", day: "numeric" }).format(then);
  const title = new Intl.DateTimeFormat(language, { dateStyle: "medium", timeStyle: "short" }).format(then);
  return { text, title };
}

export interface JustUpdatedProps {
  entries: JustUpdatedEntry[];
  onClear: () => void;
}

/**
 * 「刚更新的」: the tools updated this session, at the top of the Updates
 * page, so that an update that worked does not simply vanish from the
 * list. A grouped container (spec §3.10) under its title -- 13 bold, with
 * a small grey Clear beside it -- of quiet lines, not rows: 28 high, the
 * 20 icon, the name in 13, the version it has now in 11 muted, the ✓ and
 * 「已更新」 the row showed, in 11, and when it finished, 11 muted. Nothing
 * to select or press but Clear, which hides the section until the next
 * update succeeds; it is no part of the page's count or of Select all.
 */
export function JustUpdated({ entries, onClear }: JustUpdatedProps) {
  const { t, i18n } = useTranslation();
  const headingId = useId();
  const now = Date.now();
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
      <ul aria-labelledby={headingId} className={`py-1 ${GROUP}`}>
        {entries.map((entry) => {
          const finished =
            entry.finishedAt === null ? null : finishedText(entry.finishedAt, now, i18n.language);
          return (
            <li key={entry.opId} className="flex h-7 items-center gap-2 px-2.5">
              <ToolAvatar
                size="compact"
                adapterId={entry.adapterId}
                sourceLabel={entry.sourceLabel}
                iconKey={entry.key}
              />
              <span title={entry.name} className="min-w-0 flex-1 truncate text-body text-foreground">
                {entry.name}
              </span>
              {/* The version and the time each take a column, with or
                  without one, so that the ticks line up down the list. */}
              <span className="min-w-20 shrink-0 whitespace-nowrap text-right text-small tabular-nums text-muted">
                {entry.version}
              </span>
              <span className="inline-flex shrink-0 items-center gap-1 whitespace-nowrap text-small text-foreground">
                <CheckIcon size={12} className="shrink-0 text-success" />
                {t("updates.progress.succeeded")}
              </span>
              <span className="w-16 shrink-0 whitespace-nowrap text-right text-small tabular-nums text-muted">
                {entry.finishedAt !== null && finished !== null ? (
                  <time dateTime={new Date(entry.finishedAt).toISOString()} title={finished.title}>
                    {finished.text}
                  </time>
                ) : null}
              </span>
            </li>
          );
        })}
      </ul>
    </section>
  );
}
