import { describe, expect, it } from "vitest";
import en from "./en.json";
import zhCN from "./zh-CN.json";

/** Every string in a locale file, with its key. */
function entries(value: unknown, prefix = ""): [string, string][] {
  if (typeof value === "string") return [[prefix, value]];
  if (typeof value !== "object" || value === null) return [];
  return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) =>
    entries(child, prefix ? `${prefix}.${key}` : key),
  );
}

/** The keys of the strings in `locale` that `test` holds for. */
function keysWhere(locale: unknown, test: (text: string) => boolean): string[] {
  return entries(locale)
    .filter(([, text]) => test(text))
    .map(([key]) => key);
}

/**
 * The copy table's rules that a string can be checked against on its own
 * (docs/superpowers/2026-09-27-ui-redesign.md, 原则 3, still in force under
 * docs/superpowers/2026-09-29-aesthetics-spec.md, 4.1): no hedging, no
 * internal words, no explanation in brackets. The rest -- one line in a
 * row, at most two sentences behind an ⓘ, and above all that a shorter
 * sentence stays true -- is the table's, string by string.
 */
describe("the copy rules, over every string in both languages", () => {
  it("never hedges with 多半 or 也可能, and never says PATH, pin or 实例 in Chinese", () => {
    const offenders = entries(zhCN).filter(([, text]) => /多半|也可能|PATH|pin|实例/.test(text));
    expect(offenders).toEqual([]);
  });

  it("keeps every word a row's status column shows to six Chinese characters at most", () => {
    // A row says its state in one word (spec §3.4): the Updates and
    // Installed pages' status words (`StatusChip`), a model's skipped
    // version, and the Unknown page's broken link. A version or a name in a
    // placeholder does not count.
    const statusKeys = entries(zhCN)
      .map(([key]) => key)
      .filter(
        (key) =>
          /^updates\.blocked\.\w+\.badge$/.test(key) ||
          /^installed\.blocked\.\w+\.badge$/.test(key) ||
          /^unknown\.kind\.\w+$/.test(key) ||
          [
            "updates.readOnly",
            "updates.cannotCheck",
            "updates.selfUpdating",
            "updates.sourceUnavailable",
            "installed.updateIgnored",
            "installed.updateSkipped",
            "installed.updateSkippedNewBuild",
            "installed.uninstallHold.label",
            "majorVersion.tag",
          ].includes(key),
      );
    expect(statusKeys.length).toBeGreaterThanOrEqual(13);
    const strings = new Map(entries(zhCN));
    const tooLong = statusKeys.filter((key) => {
      const text = (strings.get(key) ?? "").replace(/\{\{\w+\}\}/g, "");
      return (text.match(/\p{Script=Han}/gu) ?? []).length > 6;
    });
    expect(tooLong).toEqual([]);
  });

  it("keeps brackets for a count, a shortcut or where a source is, and nothing else", () => {
    // 「更新所选（3）」 is a count, and 「重新检查（⌘R）」 names the keys
    // that do it, as the toolbar's tooltip says them; 「Homebrew（Intel）」
    // is a name, not copy -- which of two sources of one kind a row in the
    // sidebar is (`common.sourceWithPlace`, spec R8). 「程序（链接）」 and
    // 「（pin）」 were asides.
    const count = /[（(]\{\{number\}\}[）)]/g;
    const shortcut = /[（(]⌘[A-Z,]+[）)]/g;
    const place = /^\{\{source\}\} ?[（(]\{\{place\}\}[）)]$/g;
    const offenders = [...entries(zhCN), ...entries(en)].filter(([, text]) =>
      /[（(]/.test(text.replace(count, "").replace(shortcut, "").replace(place, "")),
    );
    expect(offenders).toEqual([]);
  });

  it("says nothing about PATH or instances in English either", () => {
    const offenders = entries(en).filter(([, text]) => /\bPATH\b|\binstances?\b/.test(text));
    expect(offenders).toEqual([]);
  });
});

/**
 * The polish-3 rules (docs/superpowers/2026-09-29-aesthetics-spec.md, 4.1
 * and 4.3): Chinese written the way macOS's own strings are, and an app
 * that is not the subject of its own sentences.
 */
describe("the polish-3 copy rules, in Chinese", () => {
  it("puts no space between Chinese and a Latin letter, a digit or an interpolation", () => {
    // text-autospace draws the gap (1/8 em, as AppKit does); a typed space
    // on top of it is twice the gap. Spaces between two Latin words, or a
    // number and a Latin unit, are not Chinese ones and stay.
    const hanThenLatin = /\p{Script=Han} [A-Za-z0-9{]/u;
    const latinThenHan = /[A-Za-z0-9}] \p{Script=Han}/u;
    expect(keysWhere(zhCN, (text) => hanThenLatin.test(text) || latinThenHan.test(text))).toEqual([]);
  });

  it("quotes with “ ” and trails off with … only", () => {
    expect(keysWhere(zhCN, (text) => /[「」]/.test(text))).toEqual([]);
    expect(keysWhere(zhCN, (text) => text.includes('"'))).toEqual([]);
    expect(keysWhere(zhCN, (text) => text.includes("...") || text.includes("……"))).toEqual([]);
  });

  it.each(["您", "我们", "！", "请注意", "需要留意", "加载中", "其它"])("never says %s", (word) => {
    expect(keysWhere(zhCN, (text) => text.includes(word))).toEqual([]);
  });

  it("names Banager only where it is the one doing the work, the one to quit, or the name itself", () => {
    // The app's name, what the daily check does, where to allow its
    // notifications, not to quit it mid-operation, to reopen it when it
    // can't load, and whose built-in logos these are. Everywhere else the sentence has no subject, or
    // says 无法…, the way macOS's own strings do.
    const allowed = [
      "app.name",
      "operations.noCancelHint",
      "emptyStates.loadFailed.nextStep",
      "settings.autoCheck.description",
      "settings.notifyUpdates.refused",
      "settings.iconCredits.simpleIcons",
    ];
    const naming = keysWhere(zhCN, (text) => text.includes("Banager"));
    expect(naming.length).toBeLessThanOrEqual(6);
    expect(naming.filter((key) => !allowed.includes(key))).toEqual([]);
  });

  it("says 没有改动 only of an operation that had started", () => {
    // A refusal stops before anything runs; 无法… already says nothing was done.
    expect(keysWhere(zhCN, (text) => text.includes("没有改动")).filter((key) => !key.startsWith("operations."))).toEqual(
      [],
    );
  });
});

describe("the polish-3 copy rules, in English", () => {
  it("never starts a sentence with Banager, but for the app's name and what its daily check does", () => {
    const allowed = ["app.name", "settings.autoCheck.description"];
    const sentenceStart = /(^|[.!?]\s+)Banager\b/;
    expect(keysWhere(en, (text) => sentenceStart.test(text)).filter((key) => !allowed.includes(key))).toEqual([]);
  });

  it("writes every button and menu item in Title Case", () => {
    // The keys are named by hand: nothing in a key's name says it is a
    // button (`updates.update` is one, `updates.newVersion` is not), and
    // an aria label or a heading is sentence case. A new button's key
    // belongs in this list.
    const buttons = [
      "app.reload",
      "common.cancel",
      "common.close",
      "common.viewLog",
      "common.details",
      "common.copyCommand",
      "common.done",
      "operations.copyLog",
      "header.checkAgain",
      "overview.reviewUpdates",
      "overview.seeProgress",
      "installed.uninstall",
      "updates.selectAll",
      "updates.updateSelectedCount",
      "updates.updateAll",
      "families.updateTheseCount",
      "families.showAll",
      "families.showAi",
      "twinsFilter.show",
      "families.showNotOnPath",
      "families.showNotOnPathCount",
      "families.showBrewRetired",
      "families.showBrewRetiredCount",
      "updates.update",
      "updates.retry",
      "updates.skipVersion",
      "updates.neverRemind",
      "updates.justUpdated.clear",
      "updates.showReasons",
      "updates.showHidden",
      "commandPreview.show_one",
      "commandPreview.show_other",
      "sourceNotice.openOllama",
      "sourceNotice.showFewer",
      "operations.batch.cancelAll",
      "operations.batch.cancelRest",
      "operations.cancelKind.Install",
      "operations.cancelKind.Uninstall",
      "operations.cancelKind.Upgrade",
      "quit.keepWaiting",
      "quit.quitAnyway",
      "uninstall.confirm",
      "uninstall.confirmPermanent",
      "settings.skippedVersions.unskip",
      "settings.ignoredUpdates.unignore",
      "settings.iconCredits.open",
      "unknown.showInFinder",
      "unknown.copyPath",
      "unknown.scanAgain",
      "brewStatus.copyLink",
      "commands.copyPath",
      "keepsData.copyPath",
    ];
    // Words that stay lower case inside a title (Cancel the Rest, Show in
    // Finder), never as its first or last word.
    const minor = new Set(["a", "an", "the", "and", "but", "or", "for", "nor", "in", "on", "at", "to", "of", "by", "as"]);
    const titleCase = (text: string): boolean => {
      const words = text
        .replace(/…$/, "")
        .split(" ")
        .filter((word) => !/^[({]/.test(word));
      return words.every(
        (word, index) => /^[A-Z]/.test(word) || (index > 0 && index < words.length - 1 && minor.has(word)),
      );
    };
    const strings = new Map(entries(en));
    for (const key of buttons) {
      const text = strings.get(key);
      expect(text, key).toBeTypeOf("string");
      expect(titleCase(text as string), `${key}: ${text}`).toBe(true);
    }
  });

  it("writes a disclosure's count in sentence case: it says how many more, it is not a command", () => {
    // 「还有N个问题」 and 「另有N个无法在这里更新」 are the words of a
    // disclosure line, as Cork's "There are 6 additional packages…".
    const strings = new Map(entries(en));
    expect(strings.get("sourceNotice.more_one")).toBe("{{count}} more issue");
    expect(strings.get("sourceNotice.more_other")).toBe("{{count}} more issues");
    expect(strings.get("updates.cantUpdateHere")).toBe("{{number}} more can't be updated here");
  });
});
