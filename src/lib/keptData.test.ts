import { describe, expect, it } from "vitest";
import table from "../../crates/banager-core/data/ai-tools.json";
import { shownBy } from "./families";
import { familyStaysAfter } from "./keptData";
import type { InstalledArtifact } from "./types";
import { NO_FACTS } from "./types";

function artifact(instance_id: string, kind: InstalledArtifact["key"]["kind"], name: string, family: string | null): InstalledArtifact {
  return {
    key: { instance_id, kind, name },
    display_name: name,
    version: "1.0",
    reason: "Requested",
    description: null,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
    facts: { ...NO_FACTS, family },
  };
}

const ownCodex = artifact("standalone-codex", "Binary", "codex", "codex");
const npmCodex = artifact("npm:/opt/homebrew", "Package", "@openai/codex", "codex");
const npmClaude = artifact("npm:/opt/homebrew", "Package", "@anthropic-ai/claude-code", "claude-code");
const ollama = artifact("brew:/opt/homebrew", "Formula", "ollama", "ollama");
const ollamaApp = artifact("brew:/opt/homebrew", "Cask", "ollama-app", "ollama");
const wget = artifact("brew:/opt/homebrew", "Formula", "wget", null);
const jq = artifact("brew:/opt/homebrew", "Formula", "jq", null);

describe("familyStaysAfter: whether a tool of the family of one that goes stays installed (U15 e)", () => {
  it("is so for another copy of the tool, either way round", () => {
    expect(familyStaysAfter([npmCodex], [ownCodex, npmCodex, wget])).toBe(true);
    expect(familyStaysAfter([ownCodex], [ownCodex, npmCodex, wget])).toBe(true);
  });

  it("is so for another tool of the family that is no copy: Ollama's app beside Homebrew's ollama", () => {
    expect(familyStaysAfter([ollama], [ollama, ollamaApp])).toBe(true);
  });

  it("is not where every tool of the family goes together", () => {
    expect(familyStaysAfter([npmCodex, ownCodex], [ownCodex, npmCodex, wget])).toBe(false);
  });

  it("is not for another family's tool, nor for tools no family has", () => {
    expect(familyStaysAfter([npmCodex], [npmCodex, npmClaude, wget, jq])).toBe(false);
    expect(familyStaysAfter([wget], [wget, jq])).toBe(false);
  });
});

/**
 * R88-1: a tool's family as `families.rs` gives it, read from the table
 * itself (crates/banager-core/data/ai-tools.json) by the same exact match --
 * npm's package name, Homebrew's formula key, a tap's with its full name --
 * so no family is written here by hand.
 */
function tableFamily(adapter: "npm" | "brew", kind: InstalledArtifact["key"]["kind"], name: string): string | null {
  const source = adapter === "npm" ? "npm" : kind === "Formula" ? "formula" : "cask";
  const found = table.families.find((family) =>
    family.members.some((member) => member.source === source && member.name === name),
  );
  return found?.id ?? null;
}

function listed(adapter: "npm" | "brew", kind: InstalledArtifact["key"]["kind"], name: string): InstalledArtifact {
  const instance = adapter === "npm" ? "npm:/opt/homebrew" : "brew:/opt/homebrew";
  return artifact(instance, kind, name, tableFamily(adapter, kind, name));
}

describe("familyStaysAfter with the vendors' own taps (R88-1)", () => {
  const taps = {
    opencode: listed("brew", "Formula", "anomalyco/tap/opencode"),
    opencodeV2: listed("brew", "Formula", "anomalyco/tap/opencode-v2"),
    crush: listed("brew", "Formula", "charmbracelet/tap/crush"),
  };
  const npm = {
    opencodeAi: listed("npm", "Package", "opencode-ai"),
    opencodeCli: listed("npm", "Package", "@opencode/cli"),
    crush: listed("npm", "Package", "@charmland/crush"),
  };
  const others = [listed("brew", "Formula", "someone/tap/opencode"), listed("brew", "Formula", "someone/tap/crush")];

  it("lists each under AI Tools, and another tap's formula of the same short name under none", () => {
    for (const tool of [...Object.values(taps), ...Object.values(npm)]) {
      expect(shownBy("ai", tool), tool.key.name).toBe(true);
    }
    for (const tool of others) expect(shownBy("ai", tool), tool.key.name).toBe(false);
  });

  it("says the family stays when the npm copy goes and the tap's stays, and not when both go", () => {
    for (const [going, staying] of [
      [npm.opencodeAi, taps.opencode],
      [npm.opencodeAi, taps.opencodeV2],
      [npm.opencodeCli, taps.opencode],
      [npm.opencodeCli, taps.opencodeV2],
      [npm.crush, taps.crush],
    ] as const) {
      expect(familyStaysAfter([going], [going, staying, wget]), going.key.name).toBe(true);
      expect(familyStaysAfter([staying], [going, staying, wget]), staying.key.name).toBe(true);
      expect(familyStaysAfter([going, staying], [going, staying, wget])).toBe(false);
    }
    // Another tap's formula of that short name is no copy that stays.
    expect(familyStaysAfter([npm.crush], [npm.crush, ...others])).toBe(false);
    expect(familyStaysAfter([npm.opencodeCli], [npm.opencodeCli, ...others])).toBe(false);
  });
});
