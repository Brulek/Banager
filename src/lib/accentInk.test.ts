import { afterEach, describe, expect, it, vi } from "vitest";
import {
  ACCENT_INK_ATTRIBUTE,
  KEEP_WHITE,
  MINIMUM,
  accentInk,
  contrastRatio,
  parseComputedColor,
  watchAccentInk,
  type Rgb,
} from "./accentInk";

/** `#RRGGBB` as channels. */
function hex(value: string): Rgb {
  return {
    r: Number.parseInt(value.slice(1, 3), 16),
    g: Number.parseInt(value.slice(3, 5), 16),
    b: Number.parseInt(value.slice(5, 7), 16),
  };
}

/** `color-mix(in srgb, accent 85%, black)`: what Increase Contrast draws the accent in (index.css). */
function darkened({ r, g, b }: Rgb): Rgb {
  return { r: r * 0.85, g: g * 0.85, b: b * 0.85 };
}

const WHITE: Rgb = { r: 255, g: 255, b: 255 };
const BLACK: Rgb = { r: 0, g: 0, b: 0 };

/**
 * macOS 27's accents, as AppKit draws them (`controlAccentColor` in sRGB,
 * read with each System Settings choice in the light and the dark
 * appearance): what WebKit's `AccentColor` is taken to be in the window --
 * not yet seen there (I21b in docs/superpowers/backlog.md).
 */
const ACCENTS: Record<string, { light: string; dark: string }> = {
  blue: { light: "#007AFF", dark: "#007AFF" },
  purple: { light: "#953D96", dark: "#A550A7" },
  pink: { light: "#F74F9E", dark: "#F74F9E" },
  red: { light: "#E0383E", dark: "#FF5257" },
  orange: { light: "#F7821B", dark: "#F7821B" },
  yellow: { light: "#FFC726", dark: "#FFC600" },
  green: { light: "#62BA46", dark: "#62BA46" },
  graphite: { light: "#989898", dark: "#8C8C8C" },
};

const inkColour = (ink: "light" | "dark") => (ink === "light" ? WHITE : BLACK);

describe("the words on the accent (decision I21b)", () => {
  it("measures contrast as WCAG does", () => {
    expect(contrastRatio(WHITE, BLACK)).toBeCloseTo(21, 5);
    expect(contrastRatio(WHITE, hex("#007AFF"))).toBeCloseTo(4.02, 2);
    expect(contrastRatio(hex("#007AFF"), WHITE)).toBeCloseTo(4.02, 2);
  });

  it("keeps the Mac's white words on its own blue, at about 4:1, as Finder draws them (I21a)", () => {
    expect(accentInk(hex("#007AFF"), false)).toBe("light");
    // An older macOS's dark blue, about 3.6:1.
    expect(accentInk(hex("#0A84FF"), false)).toBe("light");
    expect(KEEP_WHITE).toBeLessThanOrEqual(contrastRatio(WHITE, hex("#0A84FF")));
  });

  it("puts black words on yellow, green and orange, and on any accent white reads worse on than on blue, at 4.5:1 or more", () => {
    for (const name of ["yellow", "green", "orange", "graphite", "pink"]) {
      for (const appearance of ["light", "dark"] as const) {
        const accent = hex(ACCENTS[name][appearance]);
        expect(accentInk(accent, false), `${name} ${appearance}`).toBe("dark");
        expect(contrastRatio(BLACK, accent), `${name} ${appearance}`).toBeGreaterThanOrEqual(MINIMUM);
      }
    }
    // Red in the dark appearance is lighter: white reads at about 3.2:1 there.
    expect(accentInk(hex(ACCENTS.red.dark), false)).toBe("dark");
  });

  it("keeps white where it reads at least as well as on the Mac's blue: purple, and red in the light appearance", () => {
    for (const accent of [ACCENTS.purple.light, ACCENTS.purple.dark, ACCENTS.red.light]) {
      expect(accentInk(hex(accent), false), accent).toBe("light");
      expect(contrastRatio(WHITE, hex(accent))).toBeGreaterThanOrEqual(KEEP_WHITE);
    }
  });

  it("reads at 4.5:1 or more on every accent Increase Contrast darkens, in either appearance", () => {
    for (const [name, both] of Object.entries(ACCENTS)) {
      for (const appearance of ["light", "dark"] as const) {
        const accent = darkened(hex(both[appearance]));
        const ink = accentInk(accent, true);
        expect(contrastRatio(inkColour(ink), accent), `${name} ${appearance}`).toBeGreaterThanOrEqual(MINIMUM);
      }
    }
    // The blue keeps its white words.
    expect(accentInk(darkened(hex("#007AFF")), true)).toBe("light");
  });

  it("always has one of the two at 4.5:1 or more, whatever the accent", () => {
    for (let level = 0; level <= 255; level += 5) {
      for (const accent of [
        { r: level, g: level, b: level },
        { r: 255, g: level, b: 0 },
        { r: 0, g: level, b: 255 },
        { r: level, g: 255, b: level },
      ]) {
        const ink = accentInk(accent, true);
        expect(contrastRatio(inkColour(ink), accent)).toBeGreaterThanOrEqual(MINIMUM);
      }
    }
  });
});

describe("parseComputedColor", () => {
  it("reads the forms a computed colour takes", () => {
    expect(parseComputedColor("rgb(0, 122, 255)")).toEqual({ r: 0, g: 122, b: 255 });
    expect(parseComputedColor("rgba(255, 198, 0, 1)")).toEqual({ r: 255, g: 198, b: 0 });
    expect(parseComputedColor("rgb(0 122 255)")).toEqual({ r: 0, g: 122, b: 255 });
    expect(parseComputedColor("rgb(0 122 255 / 0.5)")).toEqual({ r: 0, g: 122, b: 255 });
    // color-mix()'s, as WebKit and Chrome give it.
    const mixed = parseComputedColor("color(srgb 0 0.408 0.85)");
    expect(mixed?.r).toBeCloseTo(0, 5);
    expect(mixed?.g).toBeCloseTo(104.04, 2);
    expect(mixed?.b).toBeCloseTo(216.75, 2);
    expect(parseComputedColor("color(srgb 1 0.5 0 / 0.9)")).toEqual({ r: 255, g: 127.5, b: 0 });
  });

  it("reads nothing it does not know", () => {
    for (const value of ["", "AccentColor", "transparent", "color(display-p3 1 0 0)", "hsl(0 100% 50%)"]) {
      expect(parseComputedColor(value), value).toBeNull();
    }
  });
});

describe("watchAccentInk", () => {
  afterEach(() => {
    document.documentElement.removeAttribute(ACCENT_INK_ATTRIBUTE);
    vi.restoreAllMocks();
  });

  it("marks the page for black words on a light accent, and again when the window comes back to the front", () => {
    let accent = "rgb(0, 122, 255)";
    const stop = watchAccentInk({ readAccent: () => accent, moreContrast: () => false });
    expect(document.documentElement.hasAttribute(ACCENT_INK_ATTRIBUTE)).toBe(false);

    // Yellow, picked in System Settings while Banager was in the back.
    accent = "rgb(255, 198, 0)";
    window.dispatchEvent(new Event("focus"));
    expect(document.documentElement.getAttribute(ACCENT_INK_ATTRIBUTE)).toBe("dark");

    accent = "rgb(149, 61, 150)";
    document.dispatchEvent(new Event("visibilitychange"));
    expect(document.documentElement.hasAttribute(ACCENT_INK_ATTRIBUTE)).toBe(false);

    stop();
    accent = "rgb(255, 198, 0)";
    window.dispatchEvent(new Event("focus"));
    expect(document.documentElement.hasAttribute(ACCENT_INK_ATTRIBUTE)).toBe(false);
  });

  it("asks for 4.5:1 under Increase Contrast, and leaves white words where it cannot read the accent", () => {
    // Pink, darkened: white reads at about 4.3:1, under 4.5 -- black.
    let more = false;
    const stop = watchAccentInk({ readAccent: () => "color(srgb 0.82 0.26 0.53)", moreContrast: () => more });
    expect(document.documentElement.hasAttribute(ACCENT_INK_ATTRIBUTE)).toBe(false);
    more = true;
    window.dispatchEvent(new Event("focus"));
    expect(document.documentElement.getAttribute(ACCENT_INK_ATTRIBUTE)).toBe("dark");
    stop();

    const unread = watchAccentInk({ readAccent: () => "", moreContrast: () => true });
    expect(document.documentElement.hasAttribute(ACCENT_INK_ATTRIBUTE)).toBe(false);
    unread();
  });
});
