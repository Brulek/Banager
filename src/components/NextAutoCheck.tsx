import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";

/**
 * The line under Settings' 「每天自动检查」 switch, while it is on: when the
 * daily check is next expected -- 「下次自动检查：今天21:10左右」
 * (critique §2 item 8). The time is the shell's
 * (`Snapshot::next_auto_check_at`, from `auto_check::next_check_due`): a day
 * after the last check that counted, the user's own Check again included,
 * so someone who checks by hand every day sees the next one move on, not a
 * "last automatic check" days ago that reads as broken. Said as "about":
 * the check starts at the first of the task's looks after it, which come
 * every 15 minutes of the Mac being awake, and only while Banager runs --
 * which the switch's own description says (「Banager运行时…」), so this
 * line does not say it again in brackets (src/i18n/copy-rules.test.ts).
 *
 * Nothing while `at` is not known (`null`: no check has counted yet, the
 * check at launch still under way).
 *
 * Two edges the line does not chase, both hedged by 「左右」 / 「很快」: once
 * the time has passed it says 「很快」 until the next snapshot, however long
 * the check waits -- a round under way, a Mac just woken; and with the
 * Mac's clock set back a minute or more (`SET_BACK_SLACK_SECS`), `tick`
 * checks at its next look while this still shows the later time
 * (`next_check_due` follows a clock that moves forward).
 */
export function NextAutoCheck({ at, className }: { at: number | null | undefined; className?: string }) {
  const { t, i18n } = useTranslation();
  const now = useMinute();
  if (at === null || at === undefined) return null;
  return (
    <p data-next-auto-check="" className={className}>
      {nextAutoCheckText(t, at, now, i18n.language)}
    </p>
  );
}

/**
 * The line's words for a check due at `at` (Unix seconds), seen at `nowMs`:
 * 「很快」 once the time has come -- the next look runs it -- and else the
 * time, on 「今天」, 「明天」 or a date, in the Mac's own clock style for
 * `language` (`timeStyle: "short"`: 21:10, 9:10 PM).
 */
export function nextAutoCheckText(t: TFunction, at: number, nowMs: number, language: string): string {
  const due = new Date(at * 1000);
  if (due.getTime() <= nowMs) return t("nextAutoCheck.soon");
  const time = new Intl.DateTimeFormat(language, { timeStyle: "short" }).format(due);
  const days = calendarDaysBetween(new Date(nowMs), due);
  if (days === 0) return t("nextAutoCheck.today", { time });
  if (days === 1) return t("nextAutoCheck.tomorrow", { time });
  const date = new Intl.DateTimeFormat(language, { month: "short", day: "numeric" }).format(due);
  return t("nextAutoCheck.date", { date, time });
}

/** How many calendar days, in the Mac's time zone, `to` is after `from`. */
function calendarDaysBetween(from: Date, to: Date): number {
  const start = new Date(from.getFullYear(), from.getMonth(), from.getDate());
  const end = new Date(to.getFullYear(), to.getMonth(), to.getDate());
  return Math.round((end.getTime() - start.getTime()) / 86_400_000);
}

/** The time now, moved on once a minute, so 「今天」 turns to 「很快」 on time. */
function useMinute(): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 60_000);
    return () => clearInterval(id);
  }, []);
  return now;
}
