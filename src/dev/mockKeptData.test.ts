import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { IssuedPlan, OpRequest, Snapshot, Warning } from "../lib/types";
import { createMockBackend } from "./mockBackend";
import { DEFAULT_SCENARIO, type Scenario } from "./scenario";

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

async function answer<T>(call: Promise<unknown>): Promise<T> {
  await vi.runOnlyPendingTimersAsync();
  await vi.runOnlyPendingTimersAsync();
  return (await call) as T;
}

/** The preview's uninstall plan of the row named `name` of `kind`. */
async function uninstallPlanOf(kind: string, name: string, state: Scenario["state"] = "full"): Promise<Warning[]> {
  const backend = createMockBackend({ ...DEFAULT_SCENARIO, state });
  const snapshot = await answer<Snapshot>(backend.invoke("refresh"));
  const row = snapshot.artifacts.find((a) => a.key.kind === kind && a.key.name === name);
  if (row === undefined) throw new Error(`no ${kind} ${name} in the preview`);
  const request: OpRequest = {
    kind: "Uninstall",
    instance_id: row.key.instance_id,
    artifact_kind: row.key.kind,
    name: row.key.name,
  };
  const issued = await answer<IssuedPlan>(backend.invoke("plan_operation", { request }));
  return issued.plan.warnings;
}

function keptPaths(warnings: Warning[]): string[] {
  return warnings.flatMap((w) => (typeof w !== "string" && "KeepsData" in w ? [w.KeepsData.path] : []));
}

describe("the preview's uninstall previews name what stays", () => {
  it("for Claude Code from npm (`?state=many`): ~/.claude and ~/.claude.json, measured", async () => {
    const warnings = await uninstallPlanOf("Package", "@anthropic-ai/claude-code", "many");
    expect(keptPaths(warnings)).toEqual(["~/.claude", "~/.claude.json"]);
    const sizes = warnings.flatMap((w) => (typeof w !== "string" && "KeepsData" in w ? [w.KeepsData.size] : []));
    expect(sizes.every((size) => size !== null)).toBe(true);
  });

  it("for Codex from npm, and Gemini CLI from Homebrew with its path alone", async () => {
    const codex = await uninstallPlanOf("Package", "@openai/codex");
    expect(keptPaths(codex)).toEqual(["~/.codex"]);
    // The pretend Mac has Codex's own install too: its folder is left out of the size.
    expect(codex.filter((w) => typeof w !== "string" && "KeepsData" in w)).toEqual([
      {
        KeepsData: {
          path: "~/.codex",
          what: "ToolData",
          size: { bytes: 38_400_000, partial: false, at_least: false },
          left_out: ["~/.codex/packages/standalone"],
        },
      },
    ]);
    const gemini = await uninstallPlanOf("Formula", "gemini-cli");
    expect(gemini.filter((w) => typeof w !== "string" && "KeepsData" in w)).toEqual([
      { KeepsData: { path: "~/.gemini", what: "ToolData", size: null, left_out: [] } },
    ]);
  });

  it("for Homebrew's ollama: the models folder", async () => {
    const warnings = await uninstallPlanOf("Formula", "ollama");
    expect(keptPaths(warnings)).toEqual(["~/.ollama/models"]);
  });

  it("not again for the standalone Claude Code, whose own list keeps them", async () => {
    const warnings = await uninstallPlanOf("Binary", "claude");
    expect(keptPaths(warnings)).toEqual([]);
  });

  it("for nothing else", async () => {
    expect(keptPaths(await uninstallPlanOf("Formula", "jq"))).toEqual([]);
  });
});
