import { afterEach, describe, expect, it } from "vitest";
import i18n from "../i18n";
import { revealFailureKey } from "./revealFailure";

afterEach(async () => {
  await i18n.changeLanguage("en");
});

describe("revealFailureKey", () => {
  it("says to scan again only for a program that changed since the scan", () => {
    // The envelopes `reveal_in_finder` sends (src-tauri/src/reveal.rs).
    expect(revealFailureKey(new Error('{"kind":"changed_since_scan"}'))).toBe("unknownReveal.changedSinceScan");
    expect(revealFailureKey(new Error('{"kind":"not_revealable"}'))).toBe("unknown.showInFinderFailed");
    expect(revealFailureKey(new Error('{"kind":"reveal_failed","detail":"the path is not UTF-8"}'))).toBe(
      "unknown.showInFinderFailed",
    );
    // Not an envelope at all.
    expect(revealFailureKey(new Error("No such file or directory (os error 2)"))).toBe("unknown.showInFinderFailed");
    expect(revealFailureKey(new Error('"changed_since_scan"'))).toBe("unknown.showInFinderFailed");
    expect(revealFailureKey(new Error('{"kind":7}'))).toBe("unknown.showInFinderFailed");
    expect(revealFailureKey(null)).toBe("unknown.showInFinderFailed");
  });

  it("says it in both languages", async () => {
    expect(i18n.t(revealFailureKey(new Error('{"kind":"changed_since_scan"}')))).toBe(
      "It changed after the last scan. Scan again.",
    );
    await i18n.changeLanguage("zh-CN");
    expect(i18n.t(revealFailureKey(new Error('{"kind":"changed_since_scan"}')))).toBe(
      "它在上次扫描后有变动。请重新扫描。",
    );
  });
});
