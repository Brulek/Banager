import { describe, expect, it } from "vitest";
import i18n from "../i18n";
import { neededBy, neededByItem, neededByReason, neededBySentence, type NeededBy } from "./neededBy";
import type { Warning } from "./types";

const zh = i18n.getFixedT("zh-CN");
const en = i18n.getFixedT("en");

const NPM: NeededBy = { instance_id: "npm:/opt/homebrew", program: true, tools: 4 };
const PIPX: NeededBy = { instance_id: "pipx", program: false, tools: 2 };
const OLLAMA: NeededBy = { instance_id: "ollama:http://127.0.0.1:11434", program: true, tools: 1 };

/** The sidebar's names for these sources. */
function sourceOf(instanceId: string): string {
  return { "npm:/opt/homebrew": "npm", pipx: "pipx", "ollama:http://127.0.0.1:11434": "Ollama" }[instanceId] ?? instanceId;
}

describe("neededBy", () => {
  it("picks the sources out of a plan's warnings, in their order, and nothing else", () => {
    const warnings: Warning[] = [
      { UninstallScope: { what: "HomebrewFormulaOnly" } },
      { NeededBySource: NPM },
      "DependentsUnknown",
      { WouldBreak: { names: ["pipx"] } },
      { NeededBySource: PIPX },
    ];
    expect(neededBy(warnings)).toEqual([NPM, PIPX]);
    expect(neededBy([])).toEqual([]);
  });
});

describe("neededByItem", () => {
  it("names the source with every tool, or the tools whose environment it is", () => {
    expect(neededByItem(zh, NPM, "npm")).toBe("npm和它的4个工具");
    expect(neededByItem(zh, PIPX, "pipx")).toBe("pipx装的2个工具");
    expect(neededByItem(zh, OLLAMA, "Ollama")).toBe("Ollama和它的1个模型");
    expect(neededByItem(en, NPM, "npm")).toBe("npm and its 4 tools");
    expect(neededByItem(en, PIPX, "pipx")).toBe("2 tools installed with pipx");
    expect(neededByItem(en, { ...OLLAMA, tools: 3 }, "Ollama")).toBe("Ollama and its 3 models");
    expect(neededByItem(en, { ...PIPX, tools: 1 }, "uv")).toBe("1 tool installed with uv");
  });
});

describe("neededBySentence", () => {
  it("says to uninstall the tools first, not the source whose program goes with them", () => {
    expect(neededBySentence(zh, "node@22", [NPM], sourceOf, false)).toBe("要卸载“node@22”，请先卸载npm装的4个工具。");
    expect(neededBySentence(en, "node@22", [NPM], sourceOf, false)).toBe(
      "Uninstall the 4 tools installed with npm first to remove node@22.",
    );
    expect(neededBySentence(zh, "ollama", [OLLAMA], sourceOf, false)).toBe("要卸载“ollama”，请先卸载Ollama的1个模型。");
    expect(neededBySentence(en, "ollama", [OLLAMA], sourceOf, false)).toBe("Uninstall Ollama's model first to remove ollama.");
  });

  it("names Homebrew's dependents above it as Homebrew software, and joins several sources", () => {
    expect(neededBySentence(zh, "python@3.13", [PIPX, { ...NPM, tools: 1 }], sourceOf, true)).toBe(
      "要卸载“python@3.13”，请先卸载上面的Homebrew软件、pipx装的2个工具和npm装的1个工具。",
    );
    expect(neededBySentence(en, "python@3.13", [PIPX], sourceOf, true)).toBe(
      "Uninstall the Homebrew software above and the 2 tools installed with pipx first to remove python@3.13.",
    );
    expect(neededBySentence(zh, "python@3.13", [PIPX, OLLAMA], sourceOf, false)).toBe(
      "要卸载“python@3.13”，请先卸载pipx装的2个工具和Ollama的1个模型。",
    );
  });
});

describe("neededByReason", () => {
  it("says what still needs the package, Homebrew's dependents first, and what to uninstall before it", () => {
    expect(neededByReason(zh, [NPM], [], sourceOf)).toBe(
      "还有软件要用到它：npm和它的4个工具。要卸载它，请先卸载npm装的4个工具。",
    );
    expect(neededByReason(zh, [PIPX], ["pipx"], sourceOf)).toBe(
      "还有软件要用到它：pipx和pipx装的2个工具。要卸载它，请先卸载pipx和pipx装的2个工具。",
    );
    expect(neededByReason(en, [NPM], [], sourceOf)).toBe(
      "Still used by npm and its 4 tools. To uninstall it, first uninstall the 4 tools installed with npm.",
    );
  });
});
