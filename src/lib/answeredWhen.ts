import type { Translate } from "./diagnostics";
import { shortDateText } from "./shortDate";
import type { SourceNoticeSpec } from "./sources";

/** How far the minute clock (`useMinuteClock`) can run behind the time. */
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
 * is today -- the minute clock runs that far behind -- and one further
 * ahead, a Mac clock set back since, has its date said rather than a day
 * the clock cannot vouch for.
 */
export function answeredWhen(t: Translate, at: number, nowMs: number, language: string): string {
  const atMs = at * 1000;
  const then = new Date(atMs);
  const reference = new Date(atMs > nowMs && atMs - nowMs <= MINUTE_MS ? atMs : nowMs);
  const time = new Intl.DateTimeFormat(language, { timeStyle: "short" }).format(then);
  const days = calendarDaysBetween(then, reference);
  if (days === 0) return t("sourceNotice.answered.today", { time });
  if (days === 1) return t("sourceNotice.answered.yesterday", { time });
  return t("sourceNotice.answered.date", { date: shortDateText(then, language), time });
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
