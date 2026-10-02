import { describe, expect, it } from "vitest";
import { calendarDaysBetween, mediumDateText, shortDateText, shortTimeText } from "./shortDate";

describe("calendarDaysBetween", () => {
  it("counts calendar days in the Mac's time zone, not 24-hour spans", () => {
    const at = (day: number, hour: number, minute = 0) => new Date(2026, 8, day, hour, minute);
    expect(calendarDaysBetween(at(28, 0), at(28, 23, 59))).toBe(0);
    expect(calendarDaysBetween(at(28, 23, 59), at(29, 0, 1))).toBe(1);
    expect(calendarDaysBetween(at(29, 0, 1), at(28, 23, 59))).toBe(-1);
    expect(calendarDaysBetween(at(28, 12), at(30, 11))).toBe(2);
  });

  it("is 0 exactly for two times on the same day, as the history's 「今天」 reads it", () => {
    // Every hour of three weeks against a fixed time: 0 only where year,
    // month and day are the same.
    const fixed = new Date(2026, 2, 15, 14, 30);
    for (let hours = -10 * 24; hours <= 10 * 24; hours += 1) {
      const other = new Date(fixed.getTime() + hours * 3_600_000);
      const sameDay =
        other.getFullYear() === fixed.getFullYear() &&
        other.getMonth() === fixed.getMonth() &&
        other.getDate() === fixed.getDate();
      expect(calendarDaysBetween(other, fixed) === 0, other.toString()).toBe(sameDay);
    }
  });
});

describe("the date and time styles", () => {
  it("say a time, a near day and a day with its year as Intl does for each language", () => {
    const date = new Date(2025, 3, 19, 9, 12);
    for (const language of ["en", "zh-CN"]) {
      expect(shortTimeText(date, language)).toBe(new Intl.DateTimeFormat(language, { timeStyle: "short" }).format(date));
      expect(mediumDateText(date, language)).toBe(new Intl.DateTimeFormat(language, { dateStyle: "medium" }).format(date));
      expect(shortDateText(date, language)).toBe(
        new Intl.DateTimeFormat(language, { month: "short", day: "numeric" }).format(date),
      );
    }
    expect(mediumDateText(date, "zh-CN")).toBe("2025年4月19日");
    expect(mediumDateText(date, "en")).toBe("Apr 19, 2025");
  });
});
