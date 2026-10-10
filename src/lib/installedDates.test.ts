import { describe, expect, it } from "vitest";
import { compareByInstalledAt, installedDateCellOf, installedDateText } from "./installedDates";
import { NO_FACTS, type InstalledArtifact } from "./types";

/** Seconds since the epoch of a local calendar day, as `installed_at` is. */
const day = (year: number, month: number, date: number, hour = 12) =>
  Math.floor(new Date(year, month - 1, date, hour).getTime() / 1000);

// 「今天」 in these tests: 1 October 2026, local time.
const NOW = new Date(2026, 9, 1, 9).getTime();

function tool(name: string, installed_at: number | null): InstalledArtifact {
  return {
    key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name },
    display_name: name,
    version: "1.0",
    reason: "Requested",
    description: null,
    homepage: null,
    size_bytes: null,
    installed_at,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
    facts: NO_FACTS,
  };
}

describe("compareByInstalledAt", () => {
  const byName = (a: InstalledArtifact, b: InstalledArtifact) => a.display_name.localeCompare(b.display_name);
  const order = (tools: InstalledArtifact[]) =>
    [...tools].sort((a, b) => compareByInstalledAt(a, b) || byName(a, b)).map((t) => t.display_name);

  it("puts the newest first", () => {
    const tools = [tool("old", day(2025, 4, 19)), tool("new", day(2026, 9, 28)), tool("mid", day(2026, 1, 3))];
    expect(order(tools)).toEqual(["new", "mid", "old"]);
  });

  it("calls two tools installed at the same second even, for the name to decide", () => {
    const at = day(2026, 9, 28);
    expect(compareByInstalledAt(tool("b", at), tool("a", at))).toBe(0);
    expect(order([tool("b", at), tool("a", at)])).toEqual(["a", "b"]);
  });

  it("puts a tool with no date after every tool with one, those by name", () => {
    const tools = [tool("zeta", null), tool("old", day(2019, 2, 1)), tool("alpha", null), tool("new", day(2026, 9, 1))];
    expect(order(tools)).toEqual(["new", "old", "alpha", "zeta"]);
    expect(compareByInstalledAt(tool("x", null), tool("y", null))).toBe(0);
  });
});

describe("installedDateText", () => {
  it("leaves the year out this year, in Chinese and in English", () => {
    expect(installedDateText(day(2026, 9, 28), "zh-CN", NOW)).toBe("9月28日");
    expect(installedDateText(day(2026, 9, 28), "en", NOW)).toBe("Sep 28");
  });

  it("says the year for any other year", () => {
    expect(installedDateText(day(2025, 4, 19), "zh-CN", NOW)).toBe("2025年4月19日");
    expect(installedDateText(day(2025, 4, 19), "en", NOW)).toBe("Apr 19, 2025");
  });

  it("goes by the local calendar: late on 31 December is last year", () => {
    expect(installedDateText(day(2025, 12, 31, 23), "zh-CN", NOW)).toBe("2025年12月31日");
    expect(installedDateText(day(2026, 1, 1, 0), "zh-CN", NOW)).toBe("1月1日");
  });
});

describe("installedDateCellOf", () => {
  it("says the day, or 「—」, muted, where the source said none", () => {
    expect(installedDateCellOf(tool("jq", day(2026, 9, 28)), "zh-CN", NOW)).toEqual({ text: "9月28日", muted: false });
    expect(installedDateCellOf(tool("ruff", null), "en", NOW)).toEqual({ text: "—", muted: true });
  });
});
