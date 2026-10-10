import { describe, expect, it } from "vitest";
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
