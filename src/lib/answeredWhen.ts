import type { Translate } from "./diagnostics";
import { shortDateText } from "./shortDate";
import type { SourceNoticeSpec } from "./sources";

/** How far ahead of a caller's clock a stamp may be and still count as now. */
const MINUTE_MS = 60_000;

/** How many calendar days, in the Mac's time zone, `later` is after `earlier`. */
function calendarDaysBetween(earlier: Date, later: Date): number {
  const start = new Date(earlier.getFullYear(), earlier.getMonth(), earlier.getDate());
  const end = new Date(later.getFullYear(), later.getMonth(), later.getDate());
  return Math.round((end.getTime() - start.getTime()) / 86_400_000);
}

/**
 * When a source last answered (`ManagerInstance.answered_at`, Unix
 * seconds), as a sentence says it, seen at `nowMs`: the time in the Mac's
 * own clock style for `language` (`timeStyle: "short"`: 09:12 in Chinese,
 * 9:12 AM in English -- as the next automatic check and an update's
 * finishing time say theirs), on 「今天」 / "today", 「昨天」 / "yesterday",
 * or else a date without the year, 「9月30日」 / "on Sep 30": the snapshot
 * lives as long as the window, which can stay open for days.
 *
 * Only ever a time Banager stamped. A time up to a minute ahead of `nowMs`
 * is now -- a caller's clock may be that far behind -- and one further
 * ahead, stamped before the Mac's clock was set back, has its date said
 * rather than a 「今天」 or 「昨天」 the clock cannot vouch for, on any day.
 */
export function answeredWhen(t: Translate, at: number, nowMs: number, language: string): string {
  const atMs = at * 1000;
  const then = new Date(atMs);
  const time = new Intl.DateTimeFormat(language, { timeStyle: "short" }).format(then);
  const date = () => t("sourceNotice.answered.date", { date: shortDateText(then, language), time });
  if (atMs - nowMs > MINUTE_MS) return date();
  const days = calendarDaysBetween(then, new Date(Math.max(atMs, nowMs)));
  if (days === 0) return t("sourceNotice.answered.today", { time });
  if (days === 1) return t("sourceNotice.answered.yesterday", { time });
  return date();
}

/**
 * A notice's interpolation values, with `when` -- when its source last
 * answered, worded for `nowMs` (`answeredWhen`) -- where the notice says
 * one (`SourceNoticeSpec.answeredAt`). Every place that renders a notice's
 * words goes through this: its description would otherwise show a bare
 * `{{when}}`.
 */
export function noticeValues(
  t: Translate,
  notice: Pick<SourceNoticeSpec, "values" | "answeredAt">,
  nowMs: number,
  language: string,
): SourceNoticeSpec["values"] {
  if (notice.answeredAt === undefined) return notice.values;
  return { ...notice.values, when: answeredWhen(t, notice.answeredAt, nowMs, language) };
}
