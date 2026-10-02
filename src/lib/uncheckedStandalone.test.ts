import { describe, expect, it } from "vitest";
import en from "../i18n/en.json";
import zhCN from "../i18n/zh-CN.json";
import { ADAPTER_LABEL_KEYS, standaloneSummaryKey, uninstallBlockedCopy } from "./sources";
import type { InstalledArtifact, ManagerInstance } from "./types";
import { NO_FACTS } from "./types";
import { UNCHECKED_STANDALONE, uncheckedUpdatesOf, updatesUnchecked } from "./uncheckedStandalone";

function instance(adapterId: string): ManagerInstance {
  return {
    id: adapterId,
    adapter_id: adapterId,
    exe_path: "/Users/you/.local/bin/x",
    prefix: "/Users/you/x",
    scope: "User",
    version: "1.0",
    status: { unavailable: null, notes: [] },
    unverified_version: null,
    read_only_reason: null,
  };
}

function artifact(adapterId: string, autoUpdates: boolean): InstalledArtifact {
  return {
    key: { instance_id: adapterId, kind: "Binary", name: "x" },
    display_name: "X",
    version: "1.0",
    reason: "Requested",
    description: null,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: null,
    auto_updates: autoUpdates,
    uninstall_blocked: null,
    facts: NO_FACTS,
  };
}

describe("tools whose updates Banager does not check", () => {
  it("is Codex's and opencode's own installs, the recipes with Latest::Unchecked", () => {
    expect([...UNCHECKED_STANDALONE]).toEqual(["standalone-codex", "standalone-opencode"]);
    expect(updatesUnchecked(instance("standalone-codex"))).toBe(true);
    expect(updatesUnchecked(instance("standalone-opencode"))).toBe(true);
    // Claude Code updates itself too, but its updates are checked.
    for (const id of ["standalone-claude", "standalone-agy", "standalone-grok", "standalone-rustup", "npm", "brew"]) {
      expect(updatesUnchecked(instance(id)), id).toBe(false);
    }
  });

  it("says it updates itself only when the install follows the latest release", () => {
    const codex = instance("standalone-codex");
    expect(uncheckedUpdatesOf(artifact("standalone-codex", true), codex)).toBe("updatesItself");
    expect(uncheckedUpdatesOf(artifact("standalone-codex", false), codex)).toBe("notChecked");
    expect(uncheckedUpdatesOf(artifact("standalone-claude", true), instance("standalone-claude"))).toBeNull();
  });

  it("gives Codex its name, its line and its own words for why it cannot be uninstalled here", () => {
    expect(ADAPTER_LABEL_KEYS["standalone-codex"]).toBe("adapters.standalone-codex");
    expect(en.adapters["standalone-codex"]).toBe("Codex");
    expect(zhCN.adapters["standalone-codex"]).toBe("Codex");
    expect(standaloneSummaryKey("standalone-codex")).toBe("codexStandalone.summary");
    expect(en.codexStandalone.summary).toBe("OpenAI's AI coding assistant");
    expect(zhCN.codexStandalone.summary).toBe("OpenAI的AI编程助手");
    const copy = uninstallBlockedCopy("NoSafeMethod", "standalone-codex");
    expect(copy.badge).toBe("clarity.listedOnly");
    // Not "has no uninstall command": whether it has one was not looked into.
    expect(copy.description).toBe("codexStandalone.uninstallDescription");
    expect(zhCN.codexStandalone.uninstallDescription).not.toMatch(/没有卸载命令/);
    // Nobody checked that Codex publishes uninstall instructions.
    expect(zhCN.codexStandalone.uninstallDescription).not.toMatch(/官方说明/);
    expect(en.codexStandalone.uninstallDescription).not.toMatch(/official/i);
    // Nor a command to type: none was looked into either.
    for (const text of [zhCN.codexStandalone.uninstallDescription, en.codexStandalone.uninstallDescription]) {
      expect(text).not.toMatch(/终端|运行|rm |curl|Terminal|run /i);
    }
    // What it is and what this page does with it: its own script installed it, and it is only listed.
    expect(zhCN.codexStandalone.uninstallDescription).toBe("这份{{source}}是用它自己的安装脚本装的。这里只列出它，不能在这里卸载。");
    expect(copy.command(artifact("standalone-codex", true).key, instance("standalone-codex"))).toBe("");
  });

  it("says Codex's own script installed it at its latest release, not that someone set it so, nor when it updates", () => {
    // 「设为跟随最新版本」 read as a setting the user had made. The marker
    // shows the script installed the latest release; whether its updater
    // runs, and when, is not read, so the line says it *can* update itself.
    expect(zhCN.codexStandalone.updatesItselfDetail).toBe(
      "{{source}}是用它自己的安装脚本按最新版本装的，可以自己安装新版本。这里不检查也不安装它的更新。",
    );
    expect(zhCN.codexStandalone.updatesItselfDetail).not.toMatch(/设为|使用时|运行时/);
    expect(en.codexStandalone.updatesItselfDetail).not.toMatch(/is set to|when (you )?(use|run)/i);
  });

  it("keeps the row's words short and plain in Chinese", () => {
    for (const word of [zhCN.codexStandalone.updatesItself, zhCN.codexStandalone.notChecked]) {
      expect(word.length).toBeLessThanOrEqual(6);
      expect(word).not.toMatch(/[！!您]/);
    }
  });

  it("gives opencode its name, its line, why its version is unknown, and its own words for no uninstall", () => {
    expect(ADAPTER_LABEL_KEYS["standalone-opencode"]).toBe("adapters.standalone-opencode");
    expect(en.adapters["standalone-opencode"]).toBe("opencode");
    expect(zhCN.adapters["standalone-opencode"]).toBe("opencode");
    expect(standaloneSummaryKey("standalone-opencode")).toBe("standalone.summary.standalone-opencode");
    expect(zhCN.standalone.summary["standalone-opencode"]).toBe("开源的AI编程助手");
    // Its documentation says it downloads updates itself by default.
    expect(uncheckedUpdatesOf(artifact("standalone-opencode", true), instance("standalone-opencode"))).toBe(
      "updatesItself",
    );
    expect(zhCN.standalone.opencode.updatesItselfDetail).toContain("默认");
    expect(zhCN.standalone.opencode.versionNotRead).toContain("不知道它的版本");
    const copy = uninstallBlockedCopy("NoSafeMethod", "standalone-opencode");
    expect(copy.description).toBe("standalone.opencode.uninstallDescription");
    expect(zhCN.standalone.opencode.uninstallDescription).not.toMatch(/没有卸载命令|官方说明/);
    expect(copy.command(artifact("standalone-opencode", true).key, instance("standalone-opencode"))).toBe("");
  });
});
