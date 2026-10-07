import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { IssuedPlan, OpRequest, Snapshot } from "../lib/types";
import { createMockBackend } from "./mockBackend";
import { mockRelinkLines } from "./mockKegLinks";
import { DEFAULT_SCENARIO } from "./scenario";

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

/** The preview's backend, refreshed, and its request to update the Homebrew formula `name`. */
async function backendWith(name: string) {
  const backend = createMockBackend(DEFAULT_SCENARIO);
  await answer<Snapshot>(backend.invoke("refresh"));
  const request: OpRequest = { kind: "Upgrade", instance_id: "brew:/opt/homebrew", artifact_kind: "Formula", name };
  return { backend, request };
}

/** The preview's plan of the update of the Homebrew formula `name`. */
async function updateOf(name: string): Promise<IssuedPlan> {
  const { backend, request } = await backendWith(name);
  return answer<IssuedPlan>(backend.invoke("plan_operation", { request }));
}

describe("the preview's keg-only formulae linked by hand (y1-keg)", () => {
  it("links node@22 back after its update, before its cleanup, and says so first", async () => {
    const issued = await updateOf("node@22");
    expect(issued.plan.action).toEqual({
      CommandThen: {
        program: "/opt/homebrew/bin/brew",
        args: ["upgrade", "--formula", "node@22"],
        env: expect.any(Array),
        then: [
          ["link", "--formula", "--force", "node@22"],
          ["cleanup", "node@22"],
        ],
      },
    });
    expect(issued.plan.warnings[0]).toEqual({
      HomebrewRelinksAfterUpdate: { name: "node@22", commands: ["corepack", "node", "npm", "npx"] },
    });
    // After it, the log says Homebrew linked it back itself, as it does
    // for a `brew link` it recorded.
    expect(mockRelinkLines(issued.plan)).toEqual([{ note: { StillLinkedAfterUpdate: { name: "node@22" } } }]);
  });

  it("leaves every other formula's update as it was", async () => {
    const issued = await updateOf("git");
    expect("CommandThen" in issued.plan.action && issued.plan.action.CommandThen.then).toEqual([["cleanup", "git"]]);
    expect(mockRelinkLines(issued.plan)).toEqual([]);
  });

  it("offers no update of openssl@3, whose bin/openssl another program holds", async () => {
    const { backend, request } = await backendWith("openssl@3");
    // Its refusal awaited before the clock moves, so it is never unhandled.
    const refused = expect(backend.invoke("plan_operation", { request })).rejects.toMatch(/"reason":"LinkTaken"/);
    await vi.runOnlyPendingTimersAsync();
    await refused;
  });
});
