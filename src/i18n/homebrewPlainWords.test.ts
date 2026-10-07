import { describe, expect, it } from "vitest";
import en from "./en.json";
import zhCN from "./zh-CN.json";
import zhHant from "./zh-Hant.json";

/** Every string in a locale file, with its key. */
function entries(value: unknown, prefix = ""): [string, string][] {
  if (typeof value === "string") return [[prefix, value]];
  if (typeof value !== "object" || value === null) return [];
  return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) =>
    entries(child, prefix ? `${prefix}.${key}` : key),
  );
}

/**
 * The author's decision I16 (r6): 「无法更新Homebrew软件清单」 was not
 * words a person who does not use Terminal knows, and it did not say which
 * tools it was about. Homebrew's catalogue -- what `brew update` downloads
 * -- is said as what it is for: Homebrew checking online for new versions.
 */
describe("Homebrew's catalogue, in plain words", () => {
  it.each([
    ["zh-CN", zhCN],
    ["zh-Hant", zhHant],
    ["en", en],
  ])("never calls it a software list in %s", (_name, locale) => {
    const offenders = entries(locale).filter(([, text]) =>
      /软件清单|軟體清單|软件列表|軟體列表|旧清单|舊清單|清单更新|清單更新|software list|list of software|old list/i.test(text),
    );
    expect(offenders).toEqual([]);
  });

  it("says that Homebrew could not be reached, and what that leaves out", () => {
    expect(zhCN.sourceNotice.indexMayBeStale.title).toBe("无法连上Homebrew，用它安装的工具的更新这次没能查全");
    expect(zhHant.sourceNotice.indexMayBeStale.title).toBe("無法連上Homebrew，用它安裝的工具的更新這次未能完整檢查");
    expect(en.sourceNotice.indexMayBeStale.title).toBe(
      "Couldn't reach Homebrew, so updates for its tools weren't fully checked",
    );
  });

  it("says a download still going is Homebrew checking for new versions", () => {
    expect(zhCN.sourceNotice.indexUpdating.title).toBe("Homebrew正在联网查找新版本");
    expect(zhHant.sourceNotice.indexUpdating.title).toBe("Homebrew正在連線檢查新版本");
    expect(en.sourceNotice.indexUpdating.title).toBe("Homebrew is checking online for new versions");
  });

  it("says it in Taiwan's words in zh-Hant: 檢查／尋找 and 未能, not the mainland's 查找 and 沒能", () => {
    // The file's own way: Terminal 「尋找」 a command (`notOnPathMore`).
    const offenders = entries(zhHant).filter(([, text]) => /查找|沒能/.test(text));
    expect(offenders).toEqual([]);
    expect(zhHant.sourceNotice.indexUpdating.description).toBe("「更新」頁顯示的是上次的結果。檢查完成後會自動重新檢查。");
    expect(zhHant.operations.status.waitingForBrewUpdate).toBe("正在等待Homebrew檢查新版本…");
  });
});

/**
 * r24 W9: `brew link` had two Chinese names -- 「接在终端里 / 接回 / 接上」
 * over an update's command that reads `brew link`, 「链接」 on the notice,
 * the Fix sheet, its button and the operation bar -- and a person could
 * not tell they were the same thing, the one that matters, since the Fix
 * sheet's 「链接」 is the way out. One name: 链接 / 連結, as English says
 * "link" in all of them.
 */
describe("Homebrew's link, by one name", () => {
  const ofTheLink = (locale: unknown) =>
    entries(locale).filter(
      ([key]) =>
        key.startsWith("kegLinks.") ||
        /(^|\.)notLinked$/.test(key) ||
        key === "operations.outcome.BanagerFailed.LinkTakenDetail" ||
        key.startsWith("noAnswer.sheet.") ||
        key.startsWith("noAnswer.op."),
    );

  it.each([
    ["zh-CN", zhCN, "链接"],
    ["zh-Hant", zhHant, "連結"],
  ])("says 链接 / 連結 for it throughout %s", (_name, locale, word) => {
    const sentences = ofTheLink(locale);
    expect(sentences.length).toBeGreaterThanOrEqual(20);
    // 接 alone, with 链接 / 連結 taken out: 接在、接回、接上、接到、接不回;
    // and 断开 / 斷開 for unlinking, which is 解除链接 / 解除連結.
    const otherName = /接[在回上到不]|断开|斷開/;
    expect(sentences.filter(([, text]) => otherName.test(text.replaceAll(word, "")))).toEqual([]);
    // The ones that are about the link say it by that name.
    for (const key of ["kegLinks.relinks", "kegLinks.blockedBadge", "kegLinks.logNoLongerLinkedLead"]) {
      expect(sentences.find(([each]) => each === key)?.[1]).toContain(word);
    }
    expect(sentences.find(([each]) => each === "failureMore.cause.notLinked")?.[1]).toContain(word);
  });

  it("says what the update's sheet says in the words of the Fix sheet's button", () => {
    expect(zhCN.kegLinks.relinks).toBe(
      "{{name}}已链接到终端。更新会先解除链接，再由Homebrew重新链接；更新后会检查，没有链接上就再链接一次。",
    );
    expect(zhCN.noAnswer.sheet.confirm).toBe("链接");
    expect(zhHant.kegLinks.relinks).toBe(
      "{{name}}已連結到終端機。更新會先解除連結，再由Homebrew重新連結；更新後會檢查，沒有連結上就再連結一次。",
    );
    expect(zhHant.noAnswer.sheet.confirm).toBe("連結");
    // Unlinking is 解除, never 取消: 「取消链接」 is the button that cancels a link.
    expect(zhCN.noAnswer.op.cancel).toBe("取消链接");
    expect(zhCN.kegLinks.relinks).not.toContain("取消链接");
    expect(zhHant.kegLinks.relinks).not.toContain("取消連結");
  });
});
