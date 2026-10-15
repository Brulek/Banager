import { describe, expect, it } from "vitest";
import { accentInk, parseComputedColor } from "../lib/accentInk";
import { MOCK_ACCENTS, mockAccentStyle } from "./mockAccent";

describe("?accent=", () => {
  it("stands in for AccentColor, in either appearance and darkened under Increase Contrast", () => {
    const { css, problem } = mockAccentStyle("?accent=Yellow&page=updates");
    expect(problem).toBeNull();
    expect(css).toContain(":root:root { --color-accent: #ffc726; }");
    expect(css).toContain("@media (prefers-color-scheme: dark) { :root:root { --color-accent: #ffc600; } }");
    expect(css).toContain("@media (prefers-contrast: more) { :root:root { --color-accent: color-mix(in srgb, #ffc726 85%, black); } }");
  });

  it("does nothing without it, and says so of a name it does not know", () => {
    expect(mockAccentStyle("?page=updates")).toEqual({ css: null, problem: null });
    expect(mockAccentStyle("?accent=teal")).toEqual({
      css: null,
      problem: "?accent=teal is not one of: blue, purple, pink, red, orange, yellow, green, graphite",
    });
  });

  it("has light accents for black words and dark ones for white, to look at both", () => {
    const ink = (hex: string) => accentInk(parseComputedColor(`rgb(${[1, 3, 5].map((at) => Number.parseInt(hex.slice(at, at + 2), 16)).join(", ")})`)!, false);
    expect(ink(MOCK_ACCENTS.blue.light)).toBe("light");
    expect(ink(MOCK_ACCENTS.purple.light)).toBe("light");
    expect(ink(MOCK_ACCENTS.yellow.light)).toBe("dark");
    expect(ink(MOCK_ACCENTS.green.dark)).toBe("dark");
  });
});
