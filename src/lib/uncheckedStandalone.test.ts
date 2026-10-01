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
  it("is Codex's own install alone, the one recipe with Latest::Unchecked", () => {
    expect([...UNCHECKED_STANDALONE]).toEqual(["standalone-codex"]);
    expect(updatesUnchecked(instance("standalone-codex"))).toBe(true);
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
    expect(standaloneSummaryKey("standalone-codex")).toBe("standalone.codex.summary");
    expect(en.standalone.codex.summary).toBe("OpenAI's AI coding assistant");
    expect(zhCN.standalone.codex.summary).toBe("OpenAI的AI编程助手");
    const copy = uninstallBlockedCopy("NoSafeMethod", "standalone-codex");
    expect(copy.badge).toBe("installed.blocked.NoSafeMethod.badge");
    // Not "has no uninstall command": whether it has one was not looked into.
    expect(copy.description).toBe("standalone.codex.uninstallDescription");
    expect(zhCN.standalone.codex.uninstallDescription).not.toMatch(/没有卸载命令/);
    expect(copy.command(artifact("standalone-codex", true).key, instance("standalone-codex"))).toBe("");
  });

  it("keeps the row's words short and plain in Chinese", () => {
    for (const word of [zhCN.standalone.codex.updatesItself, zhCN.standalone.codex.notChecked]) {
      expect(word.length).toBeLessThanOrEqual(6);
      expect(word).not.toMatch(/[！!您]/);
    }
  });
});
