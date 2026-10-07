import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { twinsByArtifact, twinVerdict } from "../lib/commands";
import type { ArtifactKey, CommandFact, InstalledArtifact, Snapshot } from "../lib/types";
import { NO_FACTS } from "../lib/types";
import { artifactKeyId } from "../store/ui";
import { createMockBackend } from "./mockBackend";
import { mockFamilyOf } from "./mockFamilies";
import { DEFAULT_SCENARIO } from "./scenario";

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("the preview's AI tool families", () => {
  it("tags AI tools from several sources in the pretend Mac, and nothing else", async () => {
    const backend = createMockBackend(DEFAULT_SCENARIO);
    const call = backend.invoke("refresh");
    await vi.runOnlyPendingTimersAsync();
    const snapshot = (await call) as Snapshot;
    const adapterOf = new Map(snapshot.instances.map((i) => [i.id, i.adapter_id]));
    const tagged = snapshot.artifacts
      .filter((a) => a.facts.family !== null)
      .map((a) => [adapterOf.get(a.key.instance_id), a.key.name, a.facts.family]);
    expect(tagged).toEqual([
      ["brew", "ollama", "ollama"],
      ["standalone-agy", "agy", "antigravity-cli"],
      ["standalone-claude", "claude", "claude-code"],
      ["standalone-grok", "grok", "grok-build"],
      ["uv", "mistral-vibe", "mistral-vibe"],
      ["npm", "@openai/codex", "codex"],
      ["npm", "opencode-ai", "opencode"],
      ["brew", "gemini-cli", "gemini-cli"],
      ["pipx", "aider-chat", "aider"],
      ["standalone-codex", "codex", "codex"],
    ]);
    // Three of them have an update the Updates page can offer.
    const updatable = snapshot.updates.filter((u) => ["@openai/codex", "gemini-cli", "aider-chat"].includes(u.key.name));
    expect(updatable).toHaveLength(3);
  });

  it("matches as families.rs does: by source kind, PyPI names as PEP 503 normalises them", () => {
    const key = (kind: "Formula" | "Cask" | "Package" | "Tool" | "Binary" | "Model", name: string) => ({
      instance_id: "x",
      kind,
      name,
    });
    expect(mockFamilyOf("npm", key("Package", "@openai/codex"))).toBe("codex");
    expect(mockFamilyOf("npm", key("Package", "codex"))).toBeNull();
    expect(mockFamilyOf("brew", key("Cask", "codex"))).toBe("codex");
    expect(mockFamilyOf("brew", key("Formula", "codex"))).toBeNull();
    expect(mockFamilyOf("brew", key("Formula", "goose"))).toBeNull();
    expect(mockFamilyOf("brew", key("Cask", "ollama-app"))).toBe("ollama");
    expect(mockFamilyOf("uv", key("Tool", "Aider_Chat"))).toBe("aider");
    expect(mockFamilyOf("standalone-claude", key("Binary", "claude"))).toBe("claude-code");
    expect(mockFamilyOf("standalone-rustup", key("Binary", "rustup"))).toBeNull();
    expect(mockFamilyOf("standalone-codex", key("Binary", "codex"))).toBe("codex");
    expect(mockFamilyOf("ollama", key("Model", "llama3.2:3b"))).toBeNull();
  });

  it("puts each Homebrew package review r36 V4 found missing in its tool's family, so a second copy is one", () => {
    // The same table as families.rs: each of the eight is its tool.
    for (const [kind, name, family] of [
      ["Cask", "antigravity-cli", "antigravity-cli"],
      ["Cask", "claude-code@latest", "claude-code"],
      ["Cask", "copilot-cli@prerelease", "copilot-cli"],
      ["Cask", "droid", "droid"],
      ["Cask", "ollama-binary", "ollama"],
      ["Formula", "kimi-code", "kimi-code"],
      ["Formula", "mistral-vibe", "mistral-vibe"],
      ["Formula", "openclaw-cli", "openclaw"],
    ] as const) {
      expect(mockFamilyOf("brew", { instance_id: "brew:/opt/homebrew", kind, name }), name).toBe(family);
    }
    // Antigravity CLI's own install and Homebrew's cask both put `agy` on
    // the Mac, Homebrew's first on PATH: two copies of one tool, and
    // typing `agy` runs the cask's -- not "another program with this name".
    const artifact = (key: ArtifactKey, adapterId: string, commands: CommandFact[]): InstalledArtifact => ({
      key,
      display_name: key.name,
      version: "1.0",
      reason: "Requested",
      description: null,
      homepage: null,
      size_bytes: null,
      installed_at: null,
      path: null,
      auto_updates: false,
      uninstall_blocked: null,
      facts: { ...NO_FACTS, family: mockFamilyOf(adapterId, key), commands },
    });
    const caskKey: ArtifactKey = { instance_id: "brew:/opt/homebrew", kind: "Cask", name: "antigravity-cli" };
    const cask = artifact(caskKey, "brew", [{ name: "agy", state: "Runs" }]);
    const own = artifact({ instance_id: "standalone-agy", kind: "Binary", name: "agy" }, "standalone-agy", [
      { name: "agy", state: { ShadowedBy: { by: caskKey } } },
    ]);
    const twins = twinsByArtifact([cask, own]);
    expect(twinVerdict(own, twins.get(artifactKeyId(own.key)))).toEqual({ kind: "unused", command: "agy", by: cask });
    expect(twinVerdict(cask, twins.get(artifactKeyId(cask.key)))?.kind).toBe("runs");
  });
});
