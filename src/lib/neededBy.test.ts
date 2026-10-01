import { describe, expect, it } from "vitest";
import i18n from "../i18n";
import { countsAsTool, neededBy, neededByItem, neededByReason, neededBySentence, type NeededBy } from "./neededBy";
import type { InstalledArtifact, Warning } from "./types";

const zh = i18n.getFixedT("zh-CN");
const en = i18n.getFixedT("en");

const NPM: NeededBy = { instance_id: "npm:/opt/homebrew", program: true, tools: 4 };
const PIPX: NeededBy = { instance_id: "pipx", program: false, tools: 2 };
const OLLAMA: NeededBy = { instance_id: "ollama:http://127.0.0.1:11434", program: true, tools: 1 };
const PIP: NeededBy = { instance_id: "pip:/opt/homebrew/bin/python3.14", program: true, tools: 5 };
const PIPX_ITSELF: NeededBy = { instance_id: "pipx", program: true, tools: 3 };
/** pip is view-only in Banager. */
const notPip = (instanceId: string) => !instanceId.startsWith("pip:");

/** The sidebar's names for these sources. */
function sourceOf(instanceId: string): string {
  return (
    {
      "npm:/opt/homebrew": "npm",
      pipx: "pipx",
      "ollama:http://127.0.0.1:11434": "Ollama",
      "pip:/opt/homebrew/bin/python3.14": "pip",
    }[instanceId] ?? instanceId
  );
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
    expect(neededByItem(zh, NPM, "npm")).toBe("npm及其4个工具");
    expect(neededByItem(zh, PIPX, "pipx")).toBe("pipx装的2个工具");
    expect(neededByItem(zh, OLLAMA, "Ollama")).toBe("Ollama及其1个模型");
    expect(neededByItem(en, NPM, "npm")).toBe("npm with its 4 tools");
    expect(neededByItem(en, PIPX, "pipx")).toBe("2 tools installed with pipx");
    expect(neededByItem(en, { ...OLLAMA, tools: 3 }, "Ollama")).toBe("Ollama with its 3 models");
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
      "还有软件要用到它：npm及其4个工具。要卸载它，请先卸载npm装的4个工具。",
    );
    expect(neededByReason(zh, [PIPX], ["pipx"], sourceOf)).toBe(
      "还有软件要用到它：pipx和pipx装的2个工具。要卸载它，请先卸载pipx和pipx装的2个工具。",
    );
    expect(neededByReason(en, [NPM], [], sourceOf)).toBe(
      "Still used by npm with its 4 tools. To uninstall it, first uninstall the 4 tools installed with npm.",
    );
  });
});

describe("what the sentences add after what to uninstall first", () => {
  it("says a view-only source's tools are uninstalled in Terminal", () => {
    expect(neededBySentence(zh, "python@3.14", [PIP], sourceOf, false, notPip)).toBe(
      "要卸载“python@3.14”，请先卸载pip装的5个工具。pip装的工具无法在这里卸载，要在终端里卸载。",
    );
    expect(neededBySentence(en, "python@3.14", [PIP], sourceOf, false, notPip)).toBe(
      "Uninstall the 5 tools installed with pip first to remove python@3.14. Tools installed with pip can't be uninstalled here. Uninstall them in Terminal.",
    );
    expect(neededByReason(zh, [PIP], [], sourceOf, notPip)).toBe(
      "还有软件要用到它：pip及其5个工具。要卸载它，请先卸载pip装的5个工具。pip装的工具无法在这里卸载，要在终端里卸载。",
    );
  });

  it("says pipx, uv and Cargo themselves only update and uninstall their tools", () => {
    expect(neededBySentence(zh, "pipx", [PIPX_ITSELF], sourceOf, false, notPip)).toBe(
      "要卸载“pipx”，请先卸载pipx装的3个工具。pipx装的工具要靠它更新和卸载。",
    );
    // Some of pipx's tools on a Python: their environments run on it.
    expect(neededBySentence(zh, "python@3.13", [PIPX], sourceOf, false, notPip)).toBe(
      "要卸载“python@3.13”，请先卸载pipx装的2个工具。",
    );
  });

  it("says a batch uninstalls those tools first, when every one of them goes", () => {
    expect(neededByReason(zh, [NPM], [], sourceOf, notPip, true)).toBe(
      "还有软件要用到它：npm及其4个工具。要卸载它，请先卸载npm装的4个工具。这些工具这次会卸载，之后可以再卸载它。",
    );
    expect(neededByReason(en, [NPM], [], sourceOf, notPip, true)).toBe(
      "Still used by npm with its 4 tools. To uninstall it, first uninstall the 4 tools installed with npm. Those tools are uninstalled in this batch. Uninstall it afterward.",
    );
  });
});

describe("countsAsTool", () => {
  const tool = (name: string, reason: InstalledArtifact["reason"] = "Requested") =>
    ({ key: { instance_id: "x", kind: "NpmGlobal", name }, reason }) as unknown as InstalledArtifact;
  it("leaves out what a source's program comes with, and what pip installed for another package", () => {
    expect(countsAsTool("npm", tool("npm"))).toBe(false);
    expect(countsAsTool("npm", tool("corepack"))).toBe(false);
    expect(countsAsTool("npm", tool("prettier"))).toBe(true);
    expect(countsAsTool("pip", tool("Setup_Tools".replace("_T", "t")))).toBe(false);
    expect(countsAsTool("pip", tool("requests", "Dependency"))).toBe(false);
    expect(countsAsTool("pip", tool("requests"))).toBe(true);
  });
});
