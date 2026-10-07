import { describe, expect, it } from "vitest";
import i18n from "../i18n";
import { linkFixesOf, noAnswerNotice, noAnswerRow, saidNoAnswer, NO_ANSWER_WORDS } from "./noAnswer";
import { sourceNoticesFor } from "./sources";
import { sourceStateWords } from "./diagnostics";
import type { LinkFix, ManagerInstance, NoAnswer } from "./types";

const NODE_22: LinkFix = {
  key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "node@22" },
  version: "22.23.3_1",
};
const NODE_20: LinkFix = {
  key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "node@20" },
  version: "20.19.5",
};

function npm(noAnswer: NoAnswer | null, over: Partial<ManagerInstance> = {}): ManagerInstance {
  return {
    id: "npm:/opt/homebrew",
    adapter_id: "npm",
    exe_path: "/opt/homebrew/bin/npm",
    prefix: "/opt/homebrew",
    scope: "User",
    version: null,
    answered_at: null,
    unverified_version: null,
    read_only_reason: null,
    status: { unavailable: "NotResponding", notes: [], no_answer: noAnswer },
    ...over,
  };
}

const missingNode = (fixes: LinkFix[] = []): NoAnswer => ({
  kind: "CouldNotStart",
  missing_program: "node",
  link_fixes: fixes,
});

/** What a notice says, title and sentence, in one of the three languages. */
function said(instance: ManagerInstance, language: "en" | "zh-CN" | "zh-Hant", count = 0) {
  const [notice] = sourceNoticesFor(instance, "npm", count);
  const t = i18n.getFixedT(language);
  return { title: t(notice.titleKey, notice.values), description: t(notice.descriptionKey, notice.values), notice };
}

describe("why a source did not answer", () => {
  it("says npm cannot run for want of node, not that it is not responding (finding 1, 2026-10-07)", () => {
    const { title, description, notice } = said(npm(missingNode()), "zh-CN");
    expect(title).toBe("npm无法运行");
    expect(description).toBe("找不到它需要的node，无法列出它安装的内容。");
    // No fix to offer: checking again after installing node is what helps.
    expect(notice.action).toEqual({ id: "checkAgain", labelKey: "header.checkAgain" });
    expect(said(npm(missingNode()), "en").title).toBe("npm can't run");
    expect(said(npm(missingNode()), "zh-Hant").title).toBe("npm無法執行");
    expect(said(npm(missingNode()), "zh-Hant").description).toBe("找不到它需要的node，無法列出它安裝的內容。");
  });

  it("offers Fix… with the newest formula that has the program", () => {
    const { description, notice } = said(npm(missingNode([NODE_22, NODE_20])), "zh-CN");
    expect(description).toBe(
      "找不到它需要的node。Homebrew安装的node@22中有它，但没有链接到终端能找到的地方。",
    );
    expect(notice.action).toEqual({ id: "linkFix", labelKey: "noAnswer.fix", instanceId: "npm:/opt/homebrew" });
    expect(i18n.getFixedT("en")("noAnswer.fix")).toBe("Fix…");
    expect(said(npm(missingNode([NODE_22])), "en").description).toBe(
      "It can't find the node it needs. node@22 from Homebrew has it, but it isn't linked where Terminal looks.",
    );
  });

  it("says over its rows that they are its last answer", () => {
    expect(said(npm(missingNode([NODE_22])), "zh-CN", 4).description).toBe(
      "找不到它需要的node。Homebrew安装的node@22中有它，但没有链接到终端能找到的地方。用npm安装的4个工具显示的是它上次响应时的结果。",
    );
    expect(said(npm(missingNode()), "en", 1).description).toBe(
      "It can't find the node it needs. One tool was installed with npm, and it's shown as it was when npm last responded.",
    );
  });

  it("says a program that could not start, and one that ran and failed, as that", () => {
    const couldNotStart = npm({ kind: "CouldNotStart", missing_program: null, link_fixes: [] });
    expect(said(couldNotStart, "zh-CN")).toMatchObject({
      title: "npm无法运行",
      description: "它的程序无法启动，无法列出它安装的内容。",
    });
    const failed = npm({ kind: "ExitedWithError", missing_program: null, link_fixes: [] });
    expect(said(failed, "zh-CN")).toMatchObject({
      title: "npm运行时出错",
      // Not "check again later": waiting does not fix a lasting error
      // (a malformed ~/.npmrc, a rustup with no default toolchain).
      description: "它运行时出错，无法列出它安装的内容。原因解决之前，重新检查也会出错。",
    });
    expect(said(failed, "zh-CN", 2).description).toBe(
      "它运行时出错。有2个工具是用npm安装的，显示的是它上次响应时的结果。原因解决之前，重新检查也会出错。",
    );
    expect(said(failed, "en").title).toBe("npm ran into an error");
    expect(said(failed, "en").description).toBe(
      "It ran into an error, so what it has installed can't be shown. Checking again won't help until the cause is fixed.",
    );
    expect(said(failed, "zh-Hant").description).toBe(
      "它執行時出錯，無法列出它安裝的內容。原因解決之前，重新檢查也會出錯。",
    );
  });

  it("keeps 'not responding' for one that ran out of time, or that says no reason", () => {
    for (const why of [{ kind: "TimedOut", missing_program: null, link_fixes: [] } as NoAnswer, null]) {
      const instance = npm(why);
      expect(saidNoAnswer(instance)).toBeNull();
      expect(noAnswerNotice(instance, "npm", 0)).toBeNull();
      expect(said(instance, "zh-CN").title).toBe("npm没有响应");
    }
    // An older payload with no `no_answer` at all.
    const older = npm(null);
    delete older.status.no_answer;
    expect(said(older, "zh-CN").title).toBe("npm没有响应");
  });

  it("says nothing more of Ollama, a tool with its own installer, or a source that answered", () => {
    const ollama = npm(missingNode(), { id: "ollama", adapter_id: "ollama" });
    const claude = npm(missingNode(), { id: "standalone-claude", adapter_id: "standalone-claude" });
    const answered = npm(missingNode([NODE_22]), {
      status: { unavailable: null, notes: [], no_answer: missingNode([NODE_22]) },
    });
    for (const instance of [ollama, claude, answered]) {
      expect(saidNoAnswer(instance)).toBeNull();
      expect(linkFixesOf(instance)).toEqual([]);
      expect(noAnswerRow(instance)).toBeNull();
    }
    expect(linkFixesOf(undefined)).toEqual([]);
  });

  it("says why on the source's rows and in its state word", () => {
    const row = noAnswerRow(npm(missingNode([NODE_22])));
    expect(row).not.toBeNull();
    expect(i18n.getFixedT("zh-CN")(row!.key, { ...row!.values, source: "npm" })).toBe(
      "npm无法运行：找不到它需要的node。",
    );
    const failed = noAnswerRow(npm({ kind: "ExitedWithError", missing_program: null, link_fixes: [] }));
    expect(i18n.getFixedT("en")(failed!.key, { ...failed!.values, source: "npm" })).toBe(
      "npm ran into an error. Checking again won't help until the cause is fixed.",
    );
    const t = i18n.getFixedT("zh-CN");
    expect(sourceStateWords(t, npm(missingNode()))).toEqual(["无法运行"]);
    expect(t(NO_ANSWER_WORDS.ExitedWithError)).toBe("运行时出错");
    expect(sourceStateWords(t, npm({ kind: "TimedOut", missing_program: null, link_fixes: [] }))).toEqual([
      "没有响应",
    ]);
  });

  it("lists the fixes newest first, as the core sent them", () => {
    expect(linkFixesOf(npm(missingNode([NODE_22, NODE_20])))).toEqual([NODE_22, NODE_20]);
    expect(linkFixesOf(npm({ kind: "ExitedWithError", missing_program: null, link_fixes: [NODE_22] }))).toEqual([]);
  });
});
