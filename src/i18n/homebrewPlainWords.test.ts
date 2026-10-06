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
    expect(zhHant.sourceNotice.indexMayBeStale.title).toBe("無法連上Homebrew，用它安裝的工具的更新這次沒能查全");
    expect(en.sourceNotice.indexMayBeStale.title).toBe(
      "Couldn't reach Homebrew, so updates for its tools weren't fully checked",
    );
  });

  it("says a download still going is Homebrew checking for new versions", () => {
    expect(zhCN.sourceNotice.indexUpdating.title).toBe("Homebrew正在联网查找新版本");
    expect(zhHant.sourceNotice.indexUpdating.title).toBe("Homebrew正在連線查找新版本");
    expect(en.sourceNotice.indexUpdating.title).toBe("Homebrew is checking online for new versions");
  });
});
