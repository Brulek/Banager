import { describe, expect, it } from "vitest";
import { ADAPTER_LABEL_KEYS } from "../lib/sources";
import { GLYPH_INK_DARK, loadToolIcons, type ToolIconPack } from "../lib/toolIcons";
import { renderWithProviders } from "../test/setup";
import { SOURCE_AVATAR_CLASSES, SourceAvatar } from "./SourceAvatar";

/**
 * A pack of this test's own (`renderWithProviders` otherwise hands the
 * avatars none): a glyph for Homebrew, a black one for Cargo, a raster
 * for Grok Build, nothing for the rest.
 */
const PACK: ToolIconPack = {
  version: 1,
  generated: "2026-09-28",
  glyphs: {
    "si-homebrew": { path: "M4 4h16v16H4z", hex: "FBB040", title: "Homebrew" },
    "si-rust": { path: "M4 4h16v16H4z", hex: "000000", title: "Rust" },
  },
  rasters: { "gh-xai-org": { file: "gh-xai-org.webp", title: "xai-org" } },
  tools: {},
  sources: { brew: "si-homebrew", "standalone-grok": "gh-xai-org", cargo: "si-rust" },
};
const toolIcons = loadToolIcons(PACK, new Map([["gh-xai-org.webp", "/assets/gh-xai-org.webp"]]));

describe("SourceAvatar", () => {
  it("has a colour for every source Banager knows", () => {
    // A source added to ADAPTER_LABEL_KEYS without a colour would get the
    // grey meant for a source this build does not know.
    expect(Object.keys(SOURCE_AVATAR_CLASSES).sort()).toEqual(
      Object.keys(ADAPTER_LABEL_KEYS).sort(),
    );
  });

  it("shows the source's logo from the pack, a glyph on its brand's colour, hidden from screen readers", () => {
    const { container } = renderWithProviders(<SourceAvatar adapterId="brew" label="Homebrew" size="md" />, {
      toolIcons,
    });
    const avatar = container.firstElementChild as HTMLElement;
    expect(avatar).toHaveAttribute("data-logo", "glyph");
    expect(avatar).toHaveAttribute("aria-hidden", "true");
    expect(avatar).toHaveStyle({ backgroundColor: "#FBB040" });
    // Near-black on Homebrew's amber, where white would not read.
    expect(avatar.querySelector("svg")).toHaveAttribute("fill", GLYPH_INK_DARK);
    expect(avatar.querySelector("path")).toHaveAttribute("d", "M4 4h16v16H4z");
    // In place of the initial, on the square the initial has at that size.
    expect(avatar.textContent).toBe("");
    expect(avatar.className).not.toContain("bg-source-homebrew");
    expect(avatar.className).toContain("h-8");
    expect(avatar.className).toContain("rounded-[7px]");
  });

  it("edges a glyph's square with 12% white in dark mode, and with nothing in light mode", () => {
    const { container } = renderWithProviders(<SourceAvatar adapterId="brew" label="Homebrew" size="xs" />, {
      toolIcons,
    });
    const classes = (container.firstElementChild as HTMLElement).className.split(" ");
    expect(classes).toEqual(expect.arrayContaining(["dark:inset-ring", "dark:inset-ring-white/12"]));
    // No ring or border outside the dark variant.
    expect(classes.filter((c) => /^(inset-ring|ring|border)/.test(c))).toEqual([]);
  });

  it("edges a black glyph's square with 20% white in dark mode, where 12% would be the window's own grey", () => {
    const { container } = renderWithProviders(<SourceAvatar adapterId="cargo" label="Cargo" size="xs" />, {
      toolIcons,
    });
    const classes = (container.firstElementChild as HTMLElement).className.split(" ");
    expect(classes).toEqual(expect.arrayContaining(["dark:inset-ring", "dark:inset-ring-white/20"]));
    expect(classes).not.toContain("dark:inset-ring-white/12");
    expect(classes.filter((c) => /^(inset-ring|ring|border)/.test(c))).toEqual([]);
  });

  it("draws a raster logo on white under a half-point edge, so a black one shows in dark mode", () => {
    const { container } = renderWithProviders(<SourceAvatar adapterId="standalone-grok" label="Grok Build" size="xs" />, {
      toolIcons,
    });
    const avatar = container.firstElementChild as HTMLElement;
    expect(avatar.tagName).toBe("IMG");
    expect(avatar).toHaveAttribute("data-logo", "raster");
    expect(avatar).toHaveAttribute("src", "/assets/gh-xai-org.webp");
    expect(avatar).toHaveAttribute("alt", "");
    expect(avatar).toHaveAttribute("aria-hidden", "true");
    expect(avatar.className.split(" ")).toEqual(
      expect.arrayContaining(["bg-white", "outline-[0.5px]", "outline-black/12", "object-contain", "h-4", "rounded-[4px]"]),
    );
    // In dark mode the edge is light, so one that fills its square with
    // black -- Grok's -- keeps an outline on the dark window.
    expect(avatar.className.split(" ")).toEqual(
      expect.arrayContaining(["dark:outline-1", "dark:-outline-offset-1", "dark:outline-white/20"]),
    );
  });

  it("shows the first letter of the name, hidden from screen readers, for a source the pack has no logo for", () => {
    const { container } = renderWithProviders(<SourceAvatar adapterId="brew" label="Homebrew" />);
    const avatar = container.firstElementChild;
    expect(avatar).toHaveTextContent("H");
    expect(avatar).toHaveAttribute("aria-hidden", "true");
    expect(avatar?.className).toContain("bg-source-homebrew");
    expect(avatar).not.toHaveAttribute("data-logo");
  });

  it("falls back to grey for a source it has no colour for", () => {
    const { container } = renderWithProviders(<SourceAvatar adapterId="toString" label="mystery" />, { toolIcons });
    expect(container.firstElementChild).toHaveTextContent("M");
    expect(container.firstElementChild?.className).toContain("bg-neutral-avatar");
  });
});
