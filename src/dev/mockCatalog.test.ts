import { describe, expect, it, vi } from "vitest";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { mockCatalog } from "./mockCatalog";
import { createMockBackend } from "./mockBackend";
import { DEFAULT_SCENARIO } from "./scenario";
import type { CatalogEntry, InstalledArtifact, IssuedPlan, ManagerInstance, OpSummary } from "../lib/types";
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
    expect(
      preview.flatMap((entry) =>
        entry.install === "HomebrewItself"
          ? []
          : ["Homebrew" in entry.install ? entry.install.Homebrew.name : entry.install.OllamaModel.name],
      ),
    ).toEqual(names);
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
        ...["claude-code", "codex", "gemini-cli", "ollama"].map((id) => [id, "NeedsHomebrew"]),
        // The starter model needs Ollama, not Homebrew.
        ["model", "NeedsOllama"],
        ...["node", "uv", "ffmpeg"].map((id) => [id, "NeedsHomebrew"]),
      ]);
    } finally {
      vi.useRealTimers();
    }
  });

  it("installs an entry from Discover by its id, and the check after it finds the tool", async () => {
    // docs/ui-preview.md: `?state=nothing`, Homebrew with nothing installed.
    vi.useFakeTimers();
    try {
      const backend = createMockBackend({ ...DEFAULT_SCENARIO, state: "nothing" });
      const settle = async <T,>(call: Promise<unknown>): Promise<T> => {
        await vi.runOnlyPendingTimersAsync();
        return (await call) as T;
      };
      const refusal = async (call: Promise<unknown>, reason: string) => {
        const assertion = expect(call).rejects.toBe(reason);
        await vi.runOnlyPendingTimersAsync();
        await assertion;
      };
      await settle(backend.invoke("refresh"));
      const catalog = await settle<CatalogEntry[]>(backend.invoke("get_catalog"));
      expect(catalog.filter((entry) => entry.state === "NotInstalled" && entry.installs_here).map((entry) => entry.id)).toEqual([
        "claude-code",
        "codex",
        "gemini-cli",
        "ollama",
        "node",
        "uv",
        "ffmpeg",
      ]);
      // Not offered from here, and no entry at all.
      for (const catalogId of ["homebrew", "jq", "gemma3:4b"]) {
        await refusal(backend.invoke("plan_install", { catalogId }), JSON.stringify({ kind: "refused" }));
      }
      const issued = await settle<IssuedPlan>(backend.invoke("plan_install", { catalogId: "gemini-cli" }));
      expect(issued.plan.request).toEqual({
        kind: "Install",
        instance_id: "brew:/opt/homebrew",
        artifact_kind: "Formula",
        name: "gemini-cli",
      });
      expect(issued.plan.action).toMatchObject({ Command: { args: ["install", "--formula", "gemini-cli"] } });
      const opId = (await backend.invoke("submit_operation", { planId: issued.id })) as number;
      for (let turn = 0; turn < 50; turn += 1) {
        const operations = await settle<OpSummary[]>(backend.invoke("list_operations"));
        if (operations.find((op) => op.id === opId)?.status === "Done") break;
      }
      const done = (await backend.invoke("list_operations")) as OpSummary[];
      expect(done.find((op) => op.id === opId)).toMatchObject({ status: "Done", outcome: "Succeeded" });
      await settle(backend.invoke("refresh"));
      const after = await settle<CatalogEntry[]>(backend.invoke("get_catalog"));
      expect(after.find((entry) => entry.id === "gemini-cli")?.state).toBe("Installed");
      // Installed now: refused in the row's terms. And the window still may
      // not plan an install by a package's name.
      await refusal(
        backend.invoke("plan_install", { catalogId: "gemini-cli" }),
        JSON.stringify({ kind: "catalog_state", state: "Installed" }),
      );
      await refusal(
        backend.invoke("plan_operation", { request: { ...issued.plan.request, name: "node" } }),
        JSON.stringify({ kind: "refused" }),
      );
    } finally {
      vi.useRealTimers();
    }
  });
});
