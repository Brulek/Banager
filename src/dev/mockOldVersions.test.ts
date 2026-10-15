import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { IssuedPlan, OpRequest, OpSummary, OperationEvent, Snapshot, UiEvent } from "../lib/types";
import { createMockBackend } from "./mockBackend";
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

/** The preview's plan of `kind` for the Homebrew formula `name`, and the backend it came from. */
async function planOf(kind: OpRequest["kind"], name: string) {
  const backend = createMockBackend(DEFAULT_SCENARIO);
  await answer<Snapshot>(backend.invoke("refresh"));
  const request: OpRequest = { kind, instance_id: "brew:/opt/homebrew", artifact_kind: "Formula", name };
  const issued = await answer<IssuedPlan>(backend.invoke("plan_operation", { request }));
  return { backend, issued };
}

describe("the preview's old versions of a Homebrew formula (U9)", () => {
  it("follows an update with brew cleanup, naming every version installed now", async () => {
    const { issued } = await planOf("Upgrade", "git");
    expect(issued.plan.action).toEqual({
      CommandThen: {
        program: "/opt/homebrew/bin/brew",
        args: ["upgrade", "--formula", "git"],
        env: expect.any(Array),
        then: [["cleanup", "git"]],
      },
    });
    expect(issued.plan.warnings[0]).toEqual({ HomebrewCleansUpOldVersions: { versions: ["2.54.0", "2.55.0"] } });
  });

  it("uninstalls every version of a formula with more than one, and only then passes --force", async () => {
    const two = await planOf("Uninstall", "git");
    expect("Command" in two.issued.plan.action && two.issued.plan.action.Command.args).toEqual([
      "uninstall",
      "--formula",
      "--force",
      "git",
    ]);
    expect(two.issued.plan.warnings).toContainEqual({
      HomebrewRemovesEveryVersion: { versions: ["2.54.0", "2.55.0"] },
    });
    const one = await planOf("Uninstall", "jq");
    expect("Command" in one.issued.plan.action && one.issued.plan.action.Command.args).toEqual([
      "uninstall",
      "--formula",
      "jq",
    ]);
  });

  it("says in the log where the cleanup starts, and the next refresh lists no other version", async () => {
    const { backend, issued } = await planOf("Upgrade", "git");
    const events: UiEvent[] = [];
    await backend.invoke("subscribe_events", { channel: { onmessage: (event: UiEvent) => events.push(event) } });
    const opId = await answer<number>(backend.invoke("submit_operation", { planId: issued.id }));
    await vi.runAllTimersAsync();
    const own = events.flatMap((event) => ("Operation" in event ? [event.Operation as OperationEvent] : []));
    expect(own).toContainEqual({ Note: { op_id: opId, note: { CleaningUpOldVersions: { name: "git" } } } });
    const done = (await backend.invoke("list_operations")) as OpSummary[];
    expect(done[0]).toMatchObject({ id: opId, outcome: "Succeeded" });
    const after = await answer<Snapshot>(backend.invoke("refresh"));
    const git = after.artifacts.find((a) => a.key.name === "git" && a.key.kind === "Formula");
    expect(git?.facts.homebrew?.other_versions).toEqual([]);
  });
});
