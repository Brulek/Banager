/**
 * A day as a short month and day in `language`, without the year:
 * 「11月1日」, "Nov 1". The one style for a near date -- the Overview's
 * next check, a snooze's end, an update history line from another day,
 * and a tool installed this year.
 */
export function shortDateText(date: Date, language: string): string {
  return new Intl.DateTimeFormat(language, { month: "short", day: "numeric" }).format(date);
}
