import { describe, expect, it } from "vitest";
import { AUTO_CHECK_CHOICES, autoCheckChoice, checkEvery, withAutoCheckChoice } from "./checkFrequency";
import type { Settings } from "./types";

const settings = (overrides: Partial<Settings> = {}): Settings => ({
  language: "System",
  show_technical_details: false,
  ignored_updates: [],
  skipped_versions: [],
  include_self_updating: false,
  auto_check: false,
  notify_updates: false,
  ...overrides,
});

describe("the automatic check's three choices", () => {
  it("lists Manually, Daily and Weekly, in that order", () => {
    expect(AUTO_CHECK_CHOICES).toEqual(["Off", "Day", "Week"]);
  });

  it("reads settings that do not say how often as daily, as Rust does", () => {
    expect(checkEvery(settings())).toBe("Day");
    expect(autoCheckChoice(settings({ auto_check: true }))).toBe("Day");
    expect(autoCheckChoice(settings({ auto_check: true, auto_check_every: "Week" }))).toBe("Week");
    expect(autoCheckChoice(settings({ auto_check: false, auto_check_every: "Week" }))).toBe("Off");
  });

  it("turns the check on at a frequency, and off with its notification", () => {
    const on = withAutoCheckChoice(settings({ notify_updates: false }), "Week");
    expect(on).toEqual(settings({ auto_check: true, auto_check_every: "Week" }));
    const notifying = settings({ auto_check: true, auto_check_every: "Week", notify_updates: true });
    expect(withAutoCheckChoice(notifying, "Day")).toEqual({ ...notifying, auto_check_every: "Day" });
    expect(withAutoCheckChoice(notifying, "Off")).toEqual({
      ...notifying,
      auto_check: false,
      notify_updates: false,
    });
  });
});
