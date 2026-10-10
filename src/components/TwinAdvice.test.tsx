import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";
import i18n from "../i18n";
import { twinsByArtifact } from "../lib/commands";
import type { CommandState, InstalledArtifact } from "../lib/types";
import { NO_FACTS } from "../lib/types";
import { artifactKeyId } from "../store/ui";
import { installedBy, notUsedWord, twinAdviceLines, twinUninstallLine, twinVerdict } from "./TwinAdvice";

const en = i18n.getFixedT("en");
const zh = i18n.getFixedT("zh-CN");

const LABELS: Record<string, string> = { "npm:/opt/homebrew": "npm", "standalone-codex": "Codex" };
const labelFor = (instanceId: string) => LABELS[instanceId] ?? instanceId;

function codexCopy(instanceId: string, name: string, version: string, state: CommandState | null): InstalledArtifact {
  return {
    key: { instance_id: instanceId, kind: instanceId.startsWith("npm") ? "Package" : "Binary", name },
    display_name: name,
    version,
    reason: "Requested",
    description: null,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
    facts: { ...NO_FACTS, family: "codex", commands: [{ name: "codex", state }] },
  };
}

const own = codexCopy("standalone-codex", "codex", "0.159.3", "Runs");
const npm = codexCopy("npm:/opt/homebrew", "@openai/codex", "0.155.1", {
  ShadowedBy: { by: own.key },
});
const twins = twinsByArtifact([own, npm]);
const twinsOf = (artifact: InstalledArtifact) => twins.get(artifactKeyId(artifact.key));

describe("which copy of a tool installed twice Terminal runs", () => {
  it("is this one, or the other copy, or nothing to say where the commands have no one verdict", () => {
    expect(twinVerdict(own, twinsOf(own))?.kind).toBe("runs");
    expect(twinVerdict(npm, twinsOf(npm))).toEqual({ kind: "unused", command: "codex", by: own });
    const unknown = codexCopy("npm:/opt/homebrew", "@openai/codex", "0.155.1", null);
    expect(twinVerdict(unknown, twinsByArtifact([own, unknown]).get(artifactKeyId(unknown.key)))).toBeNull();
    expect(twinVerdict(own, undefined)).toBeNull();
  });

  it("names a tool's own installer as such, a source by its name", () => {
    expect(installedBy(zh, "standalone-codex", labelFor)).toBe("Codex自带的安装程序");
    expect(installedBy(zh, "npm:/opt/homebrew", labelFor)).toBe("npm");
  });

  it("says in the details which copy runs, that Terminal does not use this one, and that it may go", () => {
    expect(twinAdviceLines(zh, npm, twinsOf(npm), labelFor, true)).toEqual([
      "在终端里输入“codex”，运行的是Codex自带的安装程序装的那一份，版本0.159.3；终端不会用到这一份。",
      "不需要的话，可以卸载这一份。",
    ]);
    // Not offered where it cannot be uninstalled here.
    expect(twinAdviceLines(en, npm, twinsOf(npm), labelFor, false)).toEqual([
      "Typing codex in Terminal runs the copy from Codex's own installer, version 0.159.3, so Terminal doesn't use this one.",
    ]);
    expect(twinAdviceLines(zh, own, twinsOf(own), labelFor, false)).toEqual([
      "在终端里输入“codex”，运行的是这一份；npm装的那一份，版本0.155.1，终端不会用到。",
    ]);
  });

  it("marks the Updates row of the copy Terminal does not run, and says updating it changes nothing typed", () => {
    expect(notUsedWord(en, own, twinsOf(own), labelFor, "Codex")).toBeUndefined();
    const word = notUsedWord(en, npm, twinsOf(npm), labelFor, "@openai/codex");
    expect(word?.label).toBe("Not used in Terminal");
    expect(word?.ariaLabel).toBe("Not used in Terminal: @openai/codex");
    const lines = [...render(<>{word?.detail}</>).container.querySelectorAll("[data-detail-line]")].map((p) => p.textContent);
    expect(lines).toEqual([
      "Typing codex in Terminal runs the copy from Codex's own installer, version 0.159.3, so Terminal doesn't use this one.",
      "Updating this copy doesn't change the one Terminal runs.",
    ]);
    // Six characters at most, as every status word.
    expect([...zh("clarity.notUsedWord")].length).toBeLessThanOrEqual(6);
  });

  it("tells the uninstall that the other copy stays, and that the command still works where it runs that one", () => {
    expect(twinUninstallLine(zh, npm, twinsOf(npm), labelFor)).toBe(
      "Codex自带的安装程序装的那一份不受影响，终端里的“codex”还能用。",
    );
    // The copy Terminal runs: the other stays, and nothing is promised of the command.
    expect(twinUninstallLine(en, own, twinsOf(own), labelFor)).toBe("The copy from npm stays.");
    expect(twinUninstallLine(en, own, undefined, labelFor)).toBeNull();
  });
});
