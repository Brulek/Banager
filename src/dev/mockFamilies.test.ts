import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Snapshot } from "../lib/types";
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
});
