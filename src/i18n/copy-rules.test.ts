import { describe, expect, it } from "vitest";
import en from "./en.json";
import zhCN from "./zh-CN.json";
import zhHant from "./zh-Hant.json";
import zhHantDescriptions from "../assets/tool-descriptions/zh-Hant.json";

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
describe.each([["zh-CN", zhCN], ["zh-Hant", zhHant]])("the copy rules, for %s and English", (_name, chinese) => {
  it("never hedges with 多半 or 也可能, and never says PATH, pin or 实例 in Chinese", () => {
    const offenders = entries(chinese).filter(([, text]) => /多半|也可能|PATH|pin|实例|實例/.test(text));
    expect(offenders).toEqual([]);
  });

  it("keeps every word a row's status column shows to six Chinese characters at most", () => {
    // A row says its state in one word (spec §3.4): the Updates and
    // Installed pages' status words (`StatusChip`), a model's skipped
    // version, and the Unknown page's broken link. A version or a name in a
    // placeholder does not count.
    const statusKeys = entries(chinese)
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
    const strings = new Map(entries(chinese));
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
    const offenders = [...entries(chinese), ...entries(en)].filter(([, text]) =>
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
describe.each([["zh-CN", zhCN], ["zh-Hant", zhHant]])("the polish-3 copy rules, in %s", (name, chinese) => {
  it("puts no space between Chinese and a Latin letter, a digit or an interpolation", () => {
    // text-autospace draws the gap (1/8 em, as AppKit does); a typed space
    // on top of it is twice the gap. Spaces between two Latin words, or a
    // number and a Latin unit, are not Chinese ones and stay.
    const hanThenLatin = /\p{Script=Han} [A-Za-z0-9{]/u;
    const latinThenHan = /[A-Za-z0-9}] \p{Script=Han}/u;
    expect(keysWhere(chinese, (text) => hanThenLatin.test(text) || latinThenHan.test(text))).toEqual([]);
  });

  it("uses the locale’s quotes and trails off with … only", () => {
    const foreignQuotes = name === "zh-Hant" ? /[“”]/ : /[「」]/;
    expect(keysWhere(chinese, (text) => foreignQuotes.test(text))).toEqual([]);
    expect(keysWhere(chinese, (text) => text.includes('"'))).toEqual([]);
    expect(keysWhere(chinese, (text) => text.includes("...") || text.includes("……"))).toEqual([]);
  });

  it.each(["您", "我们", "我們", "！", "请注意", "請注意", "需要留意", "加载中", "載入中", "其它"])("never says %s", (word) => {
    expect(keysWhere(chinese, (text) => text.includes(word))).toEqual([]);
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
    const naming = keysWhere(chinese, (text) => text.includes("Banager"));
    expect(naming.length).toBeLessThanOrEqual(6);
    expect(naming.filter((key) => !allowed.includes(key))).toEqual([]);
  });

  it("says 没有改动 only of an operation that had started", () => {
    // A refusal stops before anything runs; 无法… already says nothing was done.
    expect(keysWhere(chinese, (text) => /没有改动|沒有改動/.test(text)).filter((key) => !key.startsWith("operations."))).toEqual(
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
      "families.updateTheseCount_one",
      "families.updateTheseCount_other",
      "families.showAll",
      "families.showAi",
      "twinsFilter.show",
      "families.showNotOnPath",
      "families.showNotOnPathCount",
      "families.showBrewRetired",
      "families.showBrewRetiredCount",
      "otherVersionsShow.show",
      "otherVersionsShow.showCount",
      "twinsFilterMore.showCount",
      "families.view_one",
      "families.view_other",
      "families.viewIn.installed",
      "families.viewIn.unknown",
      "families.viewIn.updates",
      "families.viewIn.settings",
      "sourceNotice.showTool",
      "sourceNotice.showCommand",
      "updates.update",
      "updates.confirmCount_one",
      "updates.confirmCount_other",
      "updates.retry",
      "updates.skipVersion",
      "updates.snooze",
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
      "setupCheck.open",
      "unknown.showInFinder",
      "unknown.copyPath",
      "unknown.scanAgain",
      "brewStatus.copyLink",
      "commands.copyPath",
      "pathLine.copy",
      "keepsData.copyPath",
      "welcome.start",
      "shortcuts.title",
      "failureSteps.viewLogs_one",
      "failureSteps.viewLogs_other",
      "failureSteps.previous",
      "failureSteps.next",
      "needsPassword.viewSteps",
      "history.more_one",
      "history.more_other",
      "faq.title",
      "noAnswer.fix",
      "noAnswer.sheet.confirm",
      "noAnswer.op.cancel",
      "sourceDiagnostic.label",
      "sourceDiagnostic.copy",
      "failureRecovery.details",
    ];
    // Words that stay lower case inside a title (Cancel the Rest, Show in
    // Finder), never as its first or last word.
    const minor = new Set(["a", "an", "the", "and", "but", "or", "for", "nor", "in", "on", "at", "to", "of", "by", "as"]);
    const titleCase = (text: string): boolean => {
      const words = text
        .replace(/…$/, "")
        .split(" ")
        // A placeholder, or a name in quotes, has no case of its own.
        .filter((word) => !/^[({“]/.test(word));
      return words.every(
        // A number has no case: Remind Me in 30 Days.
        (word, index) => /^[A-Z0-9]/.test(word) || (index > 0 && index < words.length - 1 && minor.has(word)),
      );
    };
    const strings = new Map(entries(en));
    for (const key of buttons) {
      const text = strings.get(key);
      expect(text, key).toBeTypeOf("string");
      expect(titleCase(text as string), `${key}: ${text}`).toBe(true);
    }
  });

  it("never names a button a bare Show or View, with or without …: it says what it shows, or where (walk-3 W3-5)", () => {
    // Allowed: the 「显示」 popup's own label, which names a menu whose
    // choices say what it shows ("Show: AI Tools"), and is no button.
    const allowed = ["families.showLabel"];
    const bare = keysWhere(en, (text) => /^\s*(Show|View)\b[\s….]*$/i.test(text)).filter(
      (key) => !allowed.includes(key),
    );
    expect(bare).toEqual([]);
  });

  it("writes a disclosure's count in sentence case: it says how many more, it is not a command", () => {
    // 「还有N条提示」 and 「另有N个无法在这里更新」 are the words of a
    // disclosure line, as Cork's "There are 6 additional packages…".
    const strings = new Map(entries(en));
    expect(strings.get("sourceNotice.more_one")).toBe("{{count}} more note");
    expect(strings.get("sourceNotice.more_other")).toBe("{{count}} more notes");
    expect(strings.get("updates.cantUpdateHere")).toBe("{{number}} more can't be updated here");
  });
});

/**
 * A tool's own error words, kept where its log is not -- a source's that
 * did not answer (`SourceDiagnostic`), an operation's whose log is gone
 * (`MissingFailureLog`) -- fold out under one name in every language: not
 * 「启动诊断」, which reads as a button that starts a diagnosis and was
 * kept for any failed check, not only at startup (r21 C4).
 */
describe("the kept error words' one name", () => {
  it.each([["en", en], ["zh-CN", zhCN], ["zh-Hant", zhHant]])("in %s", (_name, locale) => {
    const strings = new Map(entries(locale));
    expect(strings.get("sourceDiagnostic.label")).toBe(strings.get("failureRecovery.details"));
    expect(strings.get("sourceDiagnostic.next")).toContain(strings.get("sourceDiagnostic.copy"));
    expect(strings.get("sourceDiagnostic.text")).not.toMatch(/Startup|启动|啟動/);
  });
});

/**
 * Traditional Chinese as Taiwan and macOS write it, not Simplified Chinese
 * converted character by character (walk-5): the words macOS's own zh_TW
 * strings use -- 拷貝, 略過, 一般, 檔案夾, 命令列, 選單列, 核心延伸功能 --
 * and Taiwan's 列 for a list's row, 透過, 錯誤訊息 and 主要版本.
 */
describe("Taiwan's words, in Traditional Chinese", () => {
  it.each([
    ["複製", "拷貝"],
    ["跳過", "略過"],
    ["暫勿", "暫時無法"],
    ["通用", "一般"],
    ["資料夾", "檔案夾"],
    ["指令列", "命令列"],
    ["錯誤資訊", "錯誤訊息"],
    ["大版本", "主要版本"],
    ["核心擴充", "核心延伸功能"],
    ["跟隨系統", "系統預設值"],
    ["能否找到", "是否找得到"],
  ])("says %s nowhere in the interface, %s instead", (word) => {
    expect(keysWhere(zhHant, (text) => text.includes(word))).toEqual([]);
  });

  /**
   * The keys of `locale` that say `pattern` anywhere but in the phrases
   * `allowed` names for that key.
   */
  const sayingOutside = (locale: unknown, pattern: RegExp, allowed: Record<string, string[]>) =>
    entries(locale)
      .filter(([key, text]) => {
        const rest = (allowed[key] ?? []).reduce((left, phrase) => left.split(phrase).join(""), text);
        return pattern.test(rest);
      })
      .map(([key]) => key);

  // 一行 as a line of a Terminal settings file, phrase by phrase; any
  // other 這一行 / 那一行 / 每一行 is a list's row, 列 (walk-5 W5-3).
  const linesOfAFile: Record<string, string[]> = {
    "warnings.editsShellConfig": ["加入的那一行"],
    "warnings.leavesShellConfigLineDetail": ["刪除那一行"],
    "warnings.leavesShellConfigLineMaybeDetail": ["檢查這一行"],
    "unreadInProtectedPlace.shellConfigUnreadDetail": ["Cargo的那一行"],
    "faq.questions.changesMac.answer": ["加入的那一行"],
    // The line for a shell startup file the details give to copy (U15 a).
    "pathLine.sentence": ["把這一行加到"],
    "pathLine.copy": ["拷貝這一行"],
    "pathLine.copyLabel": ["拷貝這一行"],
  };
  const rowsAs行 = (locale: unknown) => sayingOutside(locale, /[這那每]一行/, linesOfAFile);

  it("calls a list's row 列, and keeps 行 for a line of a file", () => {
    expect(rowsAs行(zhHant)).toEqual([]);
    // The FAQ's 「有一行寫著」 is a line, and its 「每一列都標出了原因」 a row,
    // in the one answer: the second turned back into 行 is caught (review
    // H5-A1).
    const turnedBack = structuredClone(zhHant);
    const answer = turnedBack.faq.questions.cantUpdate;
    expect(answer.answer).toContain("有一行寫著");
    answer.answer = answer.answer.replace("每一列", "每一行");
    expect(rowsAs行(turnedBack)).toEqual(["faq.questions.cantUpdate.answer"]);
  });

  it("says 透過 for ‘via’, keeping 通過 for passing a check", () => {
    const passing = { "brewStatus.reason.fails_gatekeeper_check": ["通過macOS的安全性檢查"] };
    expect(sayingOutside(zhHant, /通過/, passing)).toEqual([]);
  });

  it.each([
    ["選單欄", "選單列"],
    ["工具欄", "工具列"],
    ["占用", "佔用"],
    ["郵箱", "郵件帳號"],
    ["匹配", "比對"],
    ["線纜", "纜線"],
    ["資料夾", "檔案夾"],
    ["核心擴充", "核心延伸功能"],
    ["觸控欄", "觸控列"],
    ["語音識別", "語音辨識"],
    ["字元識別", "字元辨識"],
    ["結對", "配對"],
    ["劉海", "瀏海"],
    ["幀", "影格"],
    ["轉發", "轉送"],
    ["類函式庫", "類別函式庫"],
  ])("says %s in no tool description, %s instead", (word) => {
    const lines = Object.entries(zhHantDescriptions as Record<string, string>);
    expect(lines.filter(([, line]) => line.includes(word)).map(([key]) => key)).toEqual([]);
  });
});
