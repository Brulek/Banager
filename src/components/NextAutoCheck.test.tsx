import { afterEach, describe, expect, it } from "vitest";
import i18n from "../i18n";
import zhCN from "../i18n/zh-CN.json";
import { nextAutoCheckText } from "./NextAutoCheck";

// Local times, so the words do not hang on the machine's time zone.
const NOW = new Date(2026, 9, 1, 13, 0).getTime();
const seconds = (date: Date) => date.getTime() / 1000;

describe("nextAutoCheckText", () => {
  afterEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("says today, tomorrow or the date, with the time in the Mac's own style", async () => {
    await i18n.changeLanguage("en");
    const t = i18n.t;
    const time = (date: Date) => new Intl.DateTimeFormat("en", { timeStyle: "short" }).format(date);
    const today = new Date(2026, 9, 1, 21, 10);
    const tomorrow = new Date(2026, 9, 2, 9, 20);
    const later = new Date(2026, 9, 4, 8, 0);
    // To the nearest half hour: "about" a minute would be a minute's precision.
    expect(nextAutoCheckText(t, seconds(today), NOW, "en")).toBe(
      `Next automatic check: about ${time(new Date(2026, 9, 1, 21, 0))} today`,
    );
    expect(nextAutoCheckText(t, seconds(tomorrow), NOW, "en")).toBe(
      `Next automatic check: about ${time(new Date(2026, 9, 2, 9, 30))} tomorrow`,
    );
    expect(nextAutoCheckText(t, seconds(later), NOW, "en")).toBe(
      `Next automatic check: about ${time(later)} on Oct 4`,
    );
  });

  it("says soon once the time has come, or is within half an hour: the next look runs it", async () => {
    await i18n.changeLanguage("en");
    // And within half an hour of it, rather than a time already past once rounded.
    for (const at of [NOW / 1000, NOW / 1000 - 3600, NOW / 1000 + 20 * 60]) {
      expect(nextAutoCheckText(i18n.t, at, NOW, "en")).toBe("Next automatic check: soon");
    }
  });

  it("says it in Chinese, by the copy rules: no space before a digit, no brackets", async () => {
    await i18n.changeLanguage("zh-CN");
    expect(nextAutoCheckText(i18n.t, seconds(new Date(2026, 9, 1, 21, 10)), NOW, "zh-CN")).toBe(
      "下次自动检查：今天21:00左右",
    );
    expect(nextAutoCheckText(i18n.t, seconds(new Date(2026, 9, 2, 9, 5)), NOW, "zh-CN")).toBe(
      "下次自动检查：明天09:00左右",
    );
    expect(nextAutoCheckText(i18n.t, seconds(new Date(2026, 9, 4, 8, 0)), NOW, "zh-CN")).toBe(
      "下次自动检查：10月4日08:00左右",
    );
    expect(zhCN.nextAutoCheck.soon).toBe("下次自动检查：很快");
  });
});
