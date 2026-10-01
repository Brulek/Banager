import { describe, expect, it } from "vitest";
import { LONGEST_WAIT_MS, nextSnoozeLookMs } from "./snoozeExpiry";
import type { ArtifactKey } from "./types";

const key = (name: string): ArtifactKey => ({ instance_id: "brew:/opt/homebrew", kind: "Formula", name });
const NOW = 1_790_000_000_000;

describe("nextSnoozeLookMs", () => {
  it("waits until the earliest running snooze runs out, at most an hour", () => {
    expect(nextSnoozeLookMs({ snoozed_updates: [] }, NOW)).toBeNull();
    expect(nextSnoozeLookMs({}, NOW)).toBeNull();
    expect(
      nextSnoozeLookMs(
        {
          snoozed_updates: [
            { key: key("wget"), until: NOW / 1000 + 600 },
            { key: key("jq"), until: NOW / 1000 + 120 },
          ],
        },
        NOW,
      ),
    ).toBe(120_000);
    expect(nextSnoozeLookMs({ snoozed_updates: [{ key: key("wget"), until: NOW / 1000 + 30 * 86_400 }] }, NOW)).toBe(
      LONGEST_WAIT_MS,
    );
  });

  it("leaves out a snooze that has already run out", () => {
    expect(nextSnoozeLookMs({ snoozed_updates: [{ key: key("wget"), until: NOW / 1000 - 1 }] }, NOW)).toBeNull();
  });
});
