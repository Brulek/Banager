import { describe, expect, it } from "vitest";
import { formattingLocaleOf, systemLanguage } from "./language";

// R87-1: the window's words come from three catalogs, but a date or a
// time in System is the Mac's own, region and clock included, wherever
// the Mac's language is the window's.
describe("formattingLocaleOf", () => {
  it("formats the Mac's way in System, and while the settings load, when the Mac's language is the window's", () => {
    for (const [mac, window] of [
      ["en-GB", "en"],
      ["en-AU", "en"],
      ["en-US", "en"],
      ["zh-Hant-TW", "zh-Hant"],
      ["zh-HK", "zh-Hant"],
      ["zh-Hans-CN", "zh-CN"],
    ] as const) {
      // The window's words still come from the catalog the Mac's language names.
      expect(systemLanguage(mac)).toBe(window);
      expect(formattingLocaleOf("System", window, mac), mac).toBeNull();
      expect(formattingLocaleOf(undefined, window, mac), mac).toBeNull();
    }
  });

  it("keeps the window's language where the Mac's is one Banager has no words in", () => {
    // A French Mac reads Banager in English: not French month names among English words.
    expect(formattingLocaleOf("System", "en", "fr-FR")).toBe("en");
    expect(formattingLocaleOf("System", "en", "ja-JP")).toBe("en");
  });

  it("formats in the language chosen in Settings, whatever the Mac's", () => {
    expect(formattingLocaleOf("En", "en", "en-GB")).toBe("en");
    expect(formattingLocaleOf("ZhCn", "zh-CN", "en-GB")).toBe("zh-CN");
    expect(formattingLocaleOf("ZhHant", "zh-Hant", "zh-Hant-TW")).toBe("zh-Hant");
  });
});
