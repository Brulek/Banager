import { describe, expect, it } from "vitest";
import { BUTTON, ICON_BUTTON, LINK, SMALL_ICON_BUTTON, type ButtonKind, type ButtonSize } from "./controls";

/** A class list's classes. */
function classes(className: string): string[] {
  return className.split(/\s+/).filter((name) => name !== "");
}

/** The ones with no state prefix: how it looks at rest. */
function atRest(className: string): string[] {
  return classes(className).filter((name) => !name.includes(":"));
}

const SIZES: ButtonSize[] = ["small", "regular", "large"];
const KINDS: ButtonKind[] = ["grey", "default"];

describe("BUTTON", () => {
  it("comes in macOS's three heights: small 20, regular 24, large 28", () => {
    for (const kind of KINDS) {
      expect(atRest(BUTTON.small[kind])).toEqual(expect.arrayContaining(["h-5", "px-2", "text-small"]));
      expect(atRest(BUTTON.regular[kind])).toEqual(expect.arrayContaining(["h-6", "px-3", "text-body", "min-w-14"]));
      expect(atRest(BUTTON.large[kind])).toEqual(expect.arrayContaining(["h-7", "px-4", "text-body", "min-w-20"]));
    }
  });

  it("rounds small and regular to the control radius, and a large one fully", () => {
    for (const kind of KINDS) {
      expect(atRest(BUTTON.small[kind])).toContain("rounded-control");
      expect(atRest(BUTTON.regular[kind])).toContain("rounded-control");
      expect(atRest(BUTTON.large[kind])).toContain("rounded-full");
    }
  });

  it("draws grey as the fill with the label colour, darker while pressed", () => {
    for (const size of SIZES) {
      const grey = BUTTON[size].grey;
      expect(atRest(grey)).toEqual(expect.arrayContaining(["bg-fill", "text-foreground"]));
      expect(classes(grey)).toContain("enabled:active:bg-fill-pressed");
    }
  });

  it("draws default as the accent with white words in the regular weight, darker while pressed", () => {
    for (const size of SIZES) {
      const primary = BUTTON[size].default;
      expect(atRest(primary)).toEqual(expect.arrayContaining(["bg-accent", "text-accent-foreground", "font-normal"]));
      expect(atRest(primary)).not.toContain("font-semibold");
      expect(classes(primary)).toContain("enabled:active:bg-accent-pressed");
    }
  });

  it("changes nothing under the pointer, and has no ring of its own", () => {
    for (const size of SIZES) {
      for (const kind of KINDS) {
        expect(BUTTON[size][kind]).not.toMatch(/(^|\s)[\w-]*hover:/);
        expect(BUTTON[size][kind]).not.toMatch(/ring|outline|transition/);
      }
    }
  });

  it("turns off as the grey button with tertiary words, never as a faded accent", () => {
    for (const size of SIZES) {
      for (const kind of KINDS) {
        expect(classes(BUTTON[size][kind])).toContain("disabled:text-tertiary");
        expect(BUTTON[size][kind]).not.toMatch(/opacity/);
      }
      expect(classes(BUTTON[size].default)).toContain("disabled:bg-fill");
    }
  });

  it("is never red", () => {
    for (const size of SIZES) {
      for (const kind of KINDS) expect(BUTTON[size][kind]).not.toMatch(/danger/);
    }
  });
});

describe("ICON_BUTTON", () => {
  it("is 28 by 28 with a 16 glyph in the muted colour and no fill at rest", () => {
    const rest = atRest(ICON_BUTTON);
    expect(rest).toEqual(expect.arrayContaining(["h-7", "w-7", "rounded-control", "text-muted"]));
    expect(rest.filter((name) => name.startsWith("bg-"))).toEqual([]);
    expect(classes(ICON_BUTTON)).toContain("[&>svg]:size-4");
  });

  it("takes the quietest fill under the pointer, and the grey button's while pressed", () => {
    expect(classes(ICON_BUTTON)).toEqual(
      expect.arrayContaining(["enabled:hover:bg-fill-subtle", "enabled:active:bg-fill", "disabled:text-tertiary"]),
    );
  });
});

describe("SMALL_ICON_BUTTON", () => {
  it("is ICON_BUTTON at 20 by 20 for a status bar, the same glyph, colour and fills", () => {
    const rest = atRest(SMALL_ICON_BUTTON);
    expect(rest).toEqual(expect.arrayContaining(["h-5", "w-5", "rounded-control", "text-muted"]));
    expect(rest.filter((name) => name.startsWith("bg-"))).toEqual([]);
    expect(classes(SMALL_ICON_BUTTON)).toEqual(
      expect.arrayContaining(["[&>svg]:size-4", "enabled:hover:bg-fill-subtle", "enabled:active:bg-fill"]),
    );
  });
});

describe("LINK", () => {
  it("is the accent as text, in the regular weight and its line's size, with no underline", () => {
    expect(atRest(LINK)).toEqual(expect.arrayContaining(["text-accent-text", "font-normal"]));
    expect(LINK).not.toMatch(/underline|text-(small|body|name|title|section)|hover:/);
  });
});
