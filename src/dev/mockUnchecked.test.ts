import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Settings, Snapshot } from "../lib/types";
import { updatesSummary } from "../lib/updateState";
import { createMockBackend } from "./mockBackend";
import { DEFAULT_SCENARIO, parseScenario } from "./scenario";

/**
 * `?state=unchecked` (decision I22): the preview's Mac with nothing to
 * update and uv not answering, so the Overview names it --
 * 「uv这次没检查，其余都是最新的」 -- in place of the green check.
 */

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

const settings: Settings = {
  language: "System",
  show_technical_details: false,
  ignored_updates: [],
  skipped_versions: [],
  include_self_updating: false,
  auto_check: false,
  notify_updates: false,
};

describe("?state=unchecked", () => {
  it("is a state the preview reads", () => {
    expect(parseScenario("?state=unchecked")).toEqual({
      scenario: { ...DEFAULT_SCENARIO, state: "unchecked" },
      problems: [],
    });
  });

  it("lists no update, and leaves uv alone not answering", async () => {
    const backend = createMockBackend({ ...DEFAULT_SCENARIO, state: "unchecked" });
    const call = backend.invoke("refresh");
    await vi.runOnlyPendingTimersAsync();
    const snapshot = (await call) as Snapshot;
    expect(snapshot.updates).toEqual([]);
    expect(snapshot.errors).toEqual([]);
    expect(updatesSummary(snapshot, settings)).toMatchObject({
      kind: "nothingToUpdate",
      notChecked: { ids: [snapshot.instances.find((i) => i.adapter_id === "uv")!.id], partly: false, rest: true },
    });
  });
});
