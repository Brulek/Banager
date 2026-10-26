import { describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { mockCatalog } from "./mockCatalog";
import { createMockBackend } from "./mockBackend";
import { DEFAULT_SCENARIO } from "./scenario";
import type { CatalogEntry, InstalledArtifact, ManagerInstance } from "../lib/types";
import { NO_FACTS } from "../lib/types";

describe("the browser preview's catalog", () => {
  const instance = (adapter_id: string, id: string, over: Partial<ManagerInstance> = {}): ManagerInstance => ({
    id,
    adapter_id,
    exe_path: "/opt/homebrew/bin/brew",
    prefix: "/opt/homebrew",
    scope: "User",
    version: "7.0.10",
    answered_at: null,
    unverified_version: null,
    read_only_reason: null,
    status: { unavailable: null, notes: [] },
    ...over,
  });
  const artifact = (instance_id: string, kind: InstalledArtifact["key"]["kind"], name: string, family: string | null = null): InstalledArtifact => ({
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
  });

  it("has the entries of catalog.rs, in its order, installed by the same names", () => {
    const rust = readFileSync(
      path.join(path.dirname(fileURLToPath(import.meta.url)), "../../crates/banager-core/src/catalog.rs"),
      "utf8",
    );
    const table = rust.slice(rust.indexOf("pub const CATALOG"), rust.indexOf("/// What the Discover page says of an entry"));
    const ids = [...table.matchAll(/id: "([^"]+)"/g)].map((m) => m[1]);
    const names = [...table.matchAll(/name: "([^"]+)"/g)].map((m) => m[1]);
    const preview = mockCatalog([], []);
    expect(preview.map((entry) => entry.id)).toEqual(ids);
    expect(preview.flatMap((entry) => (entry.install === "HomebrewItself" ? [] : [entry.install.Homebrew.name]))).toEqual(names);
  });

  it("works each state out as catalog.rs does", () => {
    const brew = "brew:/opt/homebrew";
    const states = (instances: ManagerInstance[], artifacts: InstalledArtifact[]) =>
      Object.fromEntries(mockCatalog(instances, artifacts).map((entry) => [entry.id, entry.state]));
    expect(states([], [])).toMatchObject({ homebrew: "NotInstalled", codex: "NeedsHomebrew", ffmpeg: "NeedsHomebrew" });
    expect(
      states(
        [instance("brew", brew), instance("npm", "npm:/opt/homebrew")],
        [artifact("standalone-claude", "Tool", "claude", "claude-code"), artifact(brew, "Formula", "ffmpeg@7"), artifact(brew, "Formula", "nodenv")],
      ),
    ).toMatchObject({
      homebrew: "Installed",
      "claude-code": "Installed",
      codex: "NotInstalled",
      node: "Installed",
      ffmpeg: "Installed",
      uv: "NotInstalled",
    });
    expect(states([instance("brew", brew, { read_only_reason: "PrefixNotWritable" })], [])).toMatchObject({
      homebrew: "Installed",
      codex: "NeedsHomebrew",
    });
  });

  it("says what docs/ui-preview.md says of the full and the empty pretend Mac", async () => {
    // Through the mock backend, whose snapshot has each AI tool's family.
    vi.useFakeTimers();
    try {
      const states = async (state: "full" | "empty") => {
        const backend = createMockBackend({ ...DEFAULT_SCENARIO, state });
        const refreshed = backend.invoke("refresh");
        await vi.runOnlyPendingTimersAsync();
        await refreshed;
        const entries = (await backend.invoke("get_catalog")) as CatalogEntry[];
        return entries.map((entry) => [entry.id, entry.state]);
      };
      expect((await states("full")).every(([, state]) => state === "Installed")).toBe(true);
      expect(await states("empty")).toEqual([
        ["homebrew", "NotInstalled"],
        ...["claude-code", "codex", "gemini-cli", "ollama", "node", "uv", "ffmpeg"].map((id) => [id, "NeedsHomebrew"]),
      ]);
    } finally {
      vi.useRealTimers();
    }
  });
});
