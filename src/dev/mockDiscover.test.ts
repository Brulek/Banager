import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Snapshot } from "../lib/types";
import { hasCommandNotOnPath, isBrewRetired } from "../lib/families";
import { createMockBackend } from "./mockBackend";
import { DEFAULT_SCENARIO } from "./scenario";

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("the preview's discovery choices", () => {
  it("has a tool Terminal can't find and two Homebrew disabled or deprecated, out of the box", async () => {
    const backend = createMockBackend(DEFAULT_SCENARIO);
    const call = backend.invoke("refresh");
    await vi.runOnlyPendingTimersAsync();
    const snapshot = (await call) as Snapshot;
    // Grok Build, while ~/.grok/bin is off the search path (its source's
    // `NotOnPath` note), so 「终端里找不到（1）」 -- but no line of its own
    // over the list: Grok Build's notice already says it (`discoverCovered`).
    expect(snapshot.artifacts.filter((a) => hasCommandNotOnPath(a)).map((a) => a.key.name)).toEqual(["grok"]);
    // youtube-dl, deprecated, and QuickJot, disabled.
    expect(snapshot.artifacts.filter((a) => isBrewRetired(a)).map((a) => a.key.name)).toEqual([
      "youtube-dl",
      "quickjot",
    ]);
  });
});
