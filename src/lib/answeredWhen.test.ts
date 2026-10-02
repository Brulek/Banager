import { describe, expect, it } from "vitest";
import i18n from "../i18n";
import { answeredWhen, noticeValues } from "./answeredWhen";

// Local times, so the words do not hang on the machine's time zone.
const NOW = new Date(2026, 9, 2, 10, 0).getTime();
const seconds = (date: Date) => date.getTime() / 1000;
// The Mac's own clock style, as Intl writes it here: "9:12 AM" (with
// whatever space this ICU puts before AM), 「09:12」.
const time = (date: Date, language: string) => new Intl.DateTimeFormat(language, { timeStyle: "short" }).format(date);

describe("answeredWhen", () => {
  it("says today, yesterday or the date, with the time in the Mac's own style", () => {
    const en = i18n.getFixedT("en");
    const zh = i18n.getFixedT("zh-CN");
    const today = new Date(2026, 9, 2, 9, 12);
    const yesterday = new Date(2026, 9, 1, 21, 40);
    const earlier = new Date(2026, 8, 30, 21, 40);
    expect(answeredWhen(en, seconds(today), NOW, "en")).toBe(`at ${time(today, "en")} today`);
    expect(answeredWhen(zh, seconds(today), NOW, "zh-CN")).toBe("今天09:12");
    expect(answeredWhen(en, seconds(yesterday), NOW, "en")).toBe(`at ${time(yesterday, "en")} yesterday`);
    expect(answeredWhen(zh, seconds(yesterday), NOW, "zh-CN")).toBe("昨天21:40");
    expect(answeredWhen(en, seconds(earlier), NOW, "en")).toBe(`on Sep 30 at ${time(earlier, "en")}`);
    expect(answeredWhen(zh, seconds(earlier), NOW, "zh-CN")).toBe("9月30日21:40");
  });

  it("goes by calendar days, not 24 hours: a minute before midnight is yesterday a minute after it", () => {
    const zh = i18n.getFixedT("zh-CN");
    const lateLastNight = seconds(new Date(2026, 9, 1, 23, 59));
    expect(answeredWhen(zh, lateLastNight, new Date(2026, 9, 1, 23, 59, 30).getTime(), "zh-CN")).toBe("今天23:59");
    expect(answeredWhen(zh, lateLastNight, new Date(2026, 9, 2, 0, 1).getTime(), "zh-CN")).toBe("昨天23:59");
    // Two calendar days back, though not 48 hours: its date.
    expect(answeredWhen(zh, lateLastNight, new Date(2026, 9, 3, 0, 1).getTime(), "zh-CN")).toBe("10月1日23:59");
  });

  it("calls a time just ahead of the minute clock today, and dates one a clock set back cannot place", () => {
    const zh = i18n.getFixedT("zh-CN");
    // The clock that moves `nowMs` on runs up to a minute behind.
    expect(answeredWhen(zh, seconds(new Date(2026, 9, 3, 0, 0, 20)), new Date(2026, 9, 2, 23, 59, 40).getTime(), "zh-CN")).toBe(
      "今天00:00",
    );
    // Stamped by a clock a day ahead of this one: no 「今天」 or 「昨天」 for it.
    expect(answeredWhen(zh, seconds(new Date(2026, 9, 3, 9, 12)), NOW, "zh-CN")).toBe("10月3日09:12");
  });
});

describe("noticeValues", () => {
  it("adds `when` only to a notice that says when its source last answered", () => {
    const en = i18n.getFixedT("en");
    const values = { source: "uv", count: 2 };
    expect(noticeValues(en, { values }, NOW, "en")).toBe(values);
    expect(noticeValues(en, {}, NOW, "en")).toBeUndefined();
    const at = new Date(2026, 9, 2, 9, 12);
    expect(noticeValues(en, { values, answeredAt: seconds(at) }, NOW, "en")).toEqual({
      source: "uv",
      count: 2,
      when: `at ${time(at, "en")} today`,
    });
  });
});
