import { describe, expect, it, vi } from "vitest";
import { createMockBackend } from "./mockBackend";
import { DEFAULT_SCENARIO } from "./scenario";
import type { Found, IssuedPlan } from "../lib/types";

describe("the browser preview's search of Homebrew", () => {
  it("searches as the real one does, and installs a result only by its id", async () => {
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
      // No Homebrew before the first check.
      await refusal(backend.invoke("search_homebrew", { query: "jq" }), JSON.stringify({ kind: "catalog_state", state: "NeedsHomebrew" }));
      await settle(backend.invoke("refresh"));
      const found = await settle<Found[]>(backend.invoke("search_homebrew", { query: "jq" }));
      expect(found.map((f) => f.name)).toEqual(["jq", "jq-lsp", "jquake", "gojq", "jaq"]);
      expect(found.find((f) => f.name === "jquake")?.kind).toBe("Cask");
      for (const query of ["--macports", "a/b", " "]) {
        await refusal(backend.invoke("search_homebrew", { query }), JSON.stringify({ kind: "invalid_query" }));
      }
      const codex = await settle<Found[]>(backend.invoke("search_homebrew", { query: "codex" }));
      expect(codex[0]).toMatchObject({ name: "codex", kind: "Cask", catalog_id: "codex", installed: false });
      const issued = await settle<IssuedPlan>(backend.invoke("plan_install_found", { foundId: found[1].id }));
      expect(issued.plan.request).toEqual({
        kind: "Install",
        instance_id: "brew:/opt/homebrew",
        artifact_kind: "Formula",
        name: "jq-lsp",
      });
      await refusal(backend.invoke("plan_install_found", { foundId: "jq-lsp" }), JSON.stringify({ kind: "search_expired" }));
    } finally {
      vi.useRealTimers();
    }
  });
});
