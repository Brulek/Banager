/**
 * The locale a date or a time is written in: a language id (`en`,
 * `zh-CN`, `zh-Hant`), or null for the Mac's own formats -- its region's
 * order and its clock -- as the web view's `Intl` gives them with no
 * locale named (`formattingLocaleOf` in src/i18n/language.ts).
 */
export type FormattingLocale = string | null;

/**
 * A day as a short month and day in `language`, without the year:
 * 「11月1日」, "Nov 1". The one style for a near date -- the Overview's
 * next check, a snooze's end, an update history line from another day,
 * and a tool installed this year.
 */
export function shortDateText(date: Date, language: FormattingLocale): string {
  if (!Number.isFinite(date.getTime())) return "";
  return new Intl.DateTimeFormat(language ?? undefined, { month: "short", day: "numeric" }).format(date);
}

/**
 * A time of day in the clock style of `language`, the Mac's own for null
 * (`timeStyle: "short"`): 09:12 in Chinese, 9:12 AM in English, 14:30 on
 * a British Mac. The one
 * style for when a source last answered, the next automatic check and an
 * update's finishing time.
 */
export function shortTimeText(date: Date, language: FormattingLocale): string {
  if (!Number.isFinite(date.getTime())) return "";
  return new Intl.DateTimeFormat(language ?? undefined, { timeStyle: "short" }).format(date);
}

/**
 * A day with its year in `language` (`dateStyle: "medium"`):
 * 「2025年4月19日」, "Apr 19, 2025" -- the day a tool was installed, when a
 * file last changed, which "3 days ago" would blur.
 */
export function mediumDateText(date: Date, language: FormattingLocale): string {
  if (!Number.isFinite(date.getTime())) return "";
  return new Intl.DateTimeFormat(language ?? undefined, { dateStyle: "medium" }).format(date);
}

/** How many calendar days, in the Mac's time zone, `later` is after `earlier`. */
export function calendarDaysBetween(earlier: Date, later: Date): number {
  const start = new Date(earlier.getFullYear(), earlier.getMonth(), earlier.getDate());
  const end = new Date(later.getFullYear(), later.getMonth(), later.getDate());
  return Math.round((end.getTime() - start.getTime()) / 86_400_000);
}
