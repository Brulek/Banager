import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { GLYPH_INK_DARK, GLYPH_INK_LIGHT, loadToolIcons, type ToolIconPack } from "../lib/toolIcons";
import type { ArtifactKey } from "../lib/types";
import { renderWithProviders } from "../test/setup";
import { ToolAvatar } from "./ToolAvatar";

/**
 * A pack of this test's own, so that nothing here depends on what the
 * built-in pack lists. Homebrew, Cargo (Rust's), rustup (Rust's too) and
 * Claude Code have a logo; npm and pipx have none, so their badges are
 * their initials.
 */
const PACK: ToolIconPack = {
  version: 1,
  generated: "2026-09-28",
  glyphs: {
    // A dark brand colour, and two light ones.
    "si-jq": { path: "M1 1h22v22H1z", hex: "181717", title: "jq" },
    "si-typescript": { path: "M2 2h20v20H2z", hex: "FFD43B", title: "TypeScript" },
    "si-homebrew": { path: "M3 3h18v18H3z", hex: "FBB040", title: "Homebrew" },
    "si-claude": { path: "M4 4h16v16H4z", hex: "D97757", title: "Claude" },
    // Rust's near-black, Cargo's and rustup's mark alike.
    "si-rust": { path: "M5 5h14v14H5z", hex: "000000", title: "Rust" },
  },
  rasters: { "gh-iterm2": { file: "gh-iterm2.webp", title: "iTerm2" } },
  tools: {
    "brew:jq": "si-jq",
    "cask:iterm2": "gh-iterm2",
    "npm:typescript": "si-typescript",
    // A tool with its own installer: its logo is its source's.
    "standalone:claude": "si-claude",
    "standalone:rustup": "si-rust",
  },
  sources: { brew: "si-homebrew", cargo: "si-rust", "standalone-claude": "si-claude", "standalone-rustup": "si-rust" },
};
const toolIcons = loadToolIcons(PACK, new Map([["gh-iterm2.webp", "/assets/gh-iterm2.webp"]]));

const brew = "brew:/opt/homebrew";
const jq: ArtifactKey = { instance_id: brew, kind: "Formula", name: "jq" };
const wget: ArtifactKey = { instance_id: brew, kind: "Formula", name: "wget" };
const iterm: ArtifactKey = { instance_id: brew, kind: "Cask", name: "iterm2" };
const typescript: ArtifactKey = { instance_id: "npm:/opt/homebrew", kind: "Package", name: "typescript" };
const cowsay: ArtifactKey = { instance_id: "pipx", kind: "Tool", name: "cowsay" };
const claude: ArtifactKey = { instance_id: "standalone-claude", kind: "Binary", name: "claude" };
const tokei: ArtifactKey = { instance_id: "cargo:/Users/you/.cargo", kind: "Binary", name: "tokei" };
const rustup: ArtifactKey = { instance_id: "standalone-rustup", kind: "Binary", name: "rustup" };
const leftPad: ArtifactKey = { instance_id: "npm:/opt/homebrew", kind: "Package", name: "left-pad" };
/** An Ollama model outside the pack's model families, and a cask the pack has no logo for. */
const llava: ArtifactKey = { instance_id: "ollama", kind: "Model", name: "llava:7b" };
const quickjot: ArtifactKey = { instance_id: brew, kind: "Cask", name: "quickjot" };
/** TerminalIcon's prompt and cursor (src/components/icons.tsx): the Other Programs page's mark. */
const PROMPT = "M5.5 8L9.5 12L5.5 16M12.5 16.5H18.5";
const APP_ICON = "data:image/png;base64,iVBORw0KGgo=";

const mockInvoke = vi.mocked(invoke);

beforeEach(() => {
  mockInvoke.mockReset();
  // No app icon for anything, unless a test says otherwise.
  mockInvoke.mockResolvedValue(null);
});

function renderAvatar(props: Parameters<typeof ToolAvatar>[0]) {
  const rendered = renderWithProviders(<ToolAvatar {...props} />, { toolIcons });
  /** The avatar: the one element drawn, hidden from a screen reader. */
  const avatar = () => rendered.container.firstElementChild as HTMLElement;
  return { ...rendered, avatar };
}

/** The neutral program tile in `avatar`, where the tool has no icon or logo of its own. */
function programTile(avatar: HTMLElement): HTMLElement | null {
  return avatar.querySelector<HTMLElement>("[data-program-tile]");
}

/** The tool's own logo in `avatar`: the one not on its corner. */
function ownLogo(avatar: HTMLElement): HTMLElement | null {
  const logos = [...avatar.querySelectorAll<HTMLElement>("[data-logo]")];
  return logos.find((logo) => logo.closest("[data-source-badge]") === null) ?? null;
}

describe("ToolAvatar", () => {
  it("shows the tool's logo from the pack, with its source's logo on its corner", () => {
    const { avatar } = renderAvatar({ adapterId: "brew", sourceLabel: "Homebrew", iconKey: jq });

    expect(avatar()).toHaveAttribute("aria-hidden", "true");
    const logo = ownLogo(avatar());
    expect(logo).toHaveAttribute("data-logo", "glyph");
    expect(logo?.querySelector("path")).toHaveAttribute("d", "M1 1h22v22H1z");
    // A row's 32px, on the corners a source's avatar has at that size.
    expect(logo?.className).toContain("h-8");
    expect(logo?.className).toContain("rounded-[7px]");

    const badge = avatar().querySelector("[data-source-badge]");
    const mark = badge?.querySelector("[data-logo]");
    expect(mark).toHaveAttribute("data-logo", "glyph");
    expect(mark?.querySelector("path")).toHaveAttribute("d", "M3 3h18v18H3z");
    // 14px, whatever the avatar's size.
    expect(mark?.className).toContain("h-3.5");
    expect(avatar().textContent).toBe("");
  });

  it("draws a glyph white on a dark brand colour and near-black on a light one, on a square of that colour", () => {
    const { avatar } = renderAvatar({ adapterId: "brew", sourceLabel: "Homebrew", iconKey: jq });
    const dark = ownLogo(avatar()) as HTMLElement;
    expect(dark).toHaveStyle({ backgroundColor: "#181717" });
    expect(dark.querySelector("svg")).toHaveAttribute("fill", GLYPH_INK_LIGHT);

    const light = avatar().querySelector("[data-source-badge] [data-logo]") as HTMLElement;
    expect(light).toHaveStyle({ backgroundColor: "#FBB040" });
    expect(light.querySelector("svg")).toHaveAttribute("fill", GLYPH_INK_DARK);
  });

  it("edges the tool's glyph and its 14px badge's in 12% white in dark mode", () => {
    // jq's near-black, which dark mode's content would otherwise swallow.
    const { avatar } = renderAvatar({ adapterId: "brew", sourceLabel: "Homebrew", iconKey: jq });
    const edge = ["dark:inset-ring", "dark:inset-ring-white/12"];
    expect(ownLogo(avatar())?.className.split(" ")).toEqual(expect.arrayContaining(edge));
    const badge = avatar().querySelector("[data-source-badge] [data-logo]");
    expect(badge?.className.split(" ")).toEqual(expect.arrayContaining(edge));
  });

  it("puts the source's initial on the corner of the tool's logo when the source has no logo", () => {
    const { avatar } = renderAvatar({ adapterId: "npm", sourceLabel: "npm", iconKey: typescript });

    expect(ownLogo(avatar())?.querySelector("path")).toHaveAttribute("d", "M2 2h20v20H2z");
    const badge = avatar().querySelector("[data-source-badge]");
    expect(badge).toHaveTextContent("N");
    expect(badge?.firstElementChild?.className).toContain("bg-source-npm");
    expect(badge?.firstElementChild?.className).toContain("h-3.5");
  });

  it("draws a neutral program tile, its source's logo on its corner, for a tool the pack has no logo for (I8)", () => {
    // Not the source's logo: a tool without a logo of its own does not
    // borrow its source's, which made it look like another tool.
    const { avatar } = renderAvatar({ adapterId: "brew", sourceLabel: "Homebrew", iconKey: wget });

    expect(avatar()).toHaveAttribute("aria-hidden", "true");
    const tile = programTile(avatar()) as HTMLElement;
    expect(tile).not.toBeNull();
    expect(tile.closest("[data-source-badge]")).toBeNull();
    // The Other Programs page's tile: a prompt, white on the neutral grey,
    // on a row's 32px and the corners a logo has there.
    expect(tile.className.split(" ")).toEqual(
      expect.arrayContaining(["bg-neutral-avatar", "text-white", "h-8", "w-8", "rounded-[7px]"]),
    );
    expect(tile.querySelector("svg path")).toHaveAttribute("d", PROMPT);
    expect(tile.querySelector("svg")).toHaveAttribute("width", "18");
    expect(ownLogo(avatar())).toBeNull();
    // The source on its corner, 14px, as on a tool's own logo.
    const mark = avatar().querySelector("[data-source-badge] [data-logo]");
    expect(mark?.querySelector("path")).toHaveAttribute("d", "M3 3h18v18H3z");
    expect(mark?.className).toContain("h-3.5");
    // No letter anywhere: the tile is no initial.
    expect(avatar().textContent).toBe("");
  });

  it("edges the tile in 12% white in dark mode, as a logo's square is, so the dark grey keeps its outline", () => {
    const { avatar } = renderAvatar({ adapterId: "brew", sourceLabel: "Homebrew", iconKey: wget });
    expect(programTile(avatar())?.className.split(" ")).toEqual(
      expect.arrayContaining(["dark:inset-ring", "dark:inset-ring-white/12"]),
    );
  });

  it("tells a tool without a logo apart from one whose logo is its source's: tokei is no rustup (I8)", () => {
    const tokeiAvatar = renderAvatar({ adapterId: "cargo", sourceLabel: "Cargo", iconKey: tokei }).avatar();
    const rustupAvatar = renderAvatar({ adapterId: "standalone-rustup", sourceLabel: "rustup", iconKey: rustup }).avatar();

    // rustup: Rust's logo, its own source's, alone.
    expect(rustupAvatar).toHaveAttribute("data-logo", "glyph");
    expect(rustupAvatar.querySelector("path")).toHaveAttribute("d", "M5 5h14v14H5z");
    expect(programTile(rustupAvatar)).toBeNull();
    // tokei: the tile, with Cargo's Rust mark on its corner only.
    expect(programTile(tokeiAvatar)?.querySelector("path")).toHaveAttribute("d", PROMPT);
    expect(ownLogo(tokeiAvatar)).toBeNull();
    expect(tokeiAvatar.querySelector("[data-source-badge] path")).toHaveAttribute("d", "M5 5h14v14H5z");
  });

  it("draws the tile, never the source's initial, when neither the tool nor its source has a logo", () => {
    const { avatar } = renderAvatar({ adapterId: "pipx", sourceLabel: "pipx", iconKey: cowsay });

    expect(programTile(avatar())).not.toBeNull();
    expect(programTile(avatar())?.className).not.toContain("bg-source-python");
    // The initial is the source's mark, on the corner, as on a logo.
    const badge = avatar().querySelector("[data-source-badge]");
    expect(badge).toHaveTextContent("P");
    expect(badge?.firstElementChild?.className).toContain("bg-source-python");
    expect(badge?.firstElementChild?.className).toContain("h-3.5");
  });

  it("sizes the tile as an icon of each size: 24 and 48 with the badge, 20 without", () => {
    const glyphOf = (avatar: HTMLElement) => programTile(avatar)?.querySelector("svg")?.getAttribute("width");
    const sm = renderAvatar({ adapterId: "brew", sourceLabel: "Homebrew", iconKey: wget, size: "sm" }).avatar();
    expect(programTile(sm)?.className.split(" ")).toEqual(expect.arrayContaining(["h-6", "w-6", "rounded-[5px]"]));
    expect(glyphOf(sm)).toBe("12");
    expect(sm.querySelector("[data-source-badge] [data-logo]")?.className).toContain("h-3.5");

    const lg = renderAvatar({ adapterId: "brew", sourceLabel: "Homebrew", iconKey: wget, size: "lg" }).avatar();
    expect(programTile(lg)?.className.split(" ")).toEqual(expect.arrayContaining(["h-12", "w-12", "rounded-[11px]"]));
    expect(glyphOf(lg)).toBe("27");
    expect(lg.querySelector("[data-source-badge] [data-logo]")?.className).toContain("h-4");

    const compact = renderAvatar({ adapterId: "brew", sourceLabel: "Homebrew", iconKey: wget, size: "compact" }).avatar();
    expect(programTile(compact)?.className.split(" ")).toEqual(expect.arrayContaining(["h-5", "w-5", "rounded-[4px]"]));
    expect(glyphOf(compact)).toBe("12");
    expect(compact.querySelector("[data-source-badge]")).toBeNull();
  });

  it("sets the prompt a little up and to the left, clear of the badge, as Terminal's icon has it, and centres it with none", () => {
    // At 32 a centred prompt's cursor ran under the 14 badge on the corner.
    const promptOf = (size: "sm" | "md" | "lg" | "compact") =>
      programTile(renderAvatar({ adapterId: "brew", sourceLabel: "Homebrew", iconKey: wget, size }).avatar())
        ?.querySelector("svg")
        ?.getAttribute("class")
        ?.split(" ") ?? [];
    expect(promptOf("md")).toEqual(expect.arrayContaining(["-translate-x-px", "-translate-y-0.5"]));
    expect(promptOf("sm")).toEqual(expect.arrayContaining(["-translate-x-px", "-translate-y-0.5"]));
    expect(promptOf("lg")).toEqual(expect.arrayContaining(["-translate-x-0.5", "-translate-y-0.75"]));
    expect(promptOf("compact").filter((c) => c.includes("translate"))).toEqual([]);
  });

  it("shows a tool whose logo is its source's own as the source, with nothing on its corner", () => {
    const { avatar } = renderAvatar({ adapterId: "standalone-claude", sourceLabel: "Claude Code", iconKey: claude });

    expect(avatar()).toHaveAttribute("data-logo", "glyph");
    expect(avatar().querySelector("path")).toHaveAttribute("d", "M4 4h16v16H4z");
    expect(avatar().querySelector("[data-source-badge]")).toBeNull();
  });

  it("draws the prompt on the tile of each kind that is a program: a formula, a package, a tool, a binary", () => {
    const keys: [string, string, ArtifactKey][] = [
      ["brew", "Homebrew", wget],
      ["npm", "npm", leftPad],
      ["pipx", "pipx", cowsay],
      ["cargo", "Cargo", tokei],
    ];
    for (const [adapterId, sourceLabel, iconKey] of keys) {
      const tile = programTile(renderAvatar({ adapterId, sourceLabel, iconKey }).avatar());
      expect(tile?.querySelector("svg path"), iconKey.kind).toHaveAttribute("d", PROMPT);
    }
  });

  it("draws an Ollama model the pack has no logo for as the tile with no prompt: a model is data, not a command-line program (I8, I10)", () => {
    const { avatar } = renderAvatar({ adapterId: "ollama", sourceLabel: "Ollama", iconKey: llava });

    const tile = programTile(avatar()) as HTMLElement;
    expect(tile).not.toBeNull();
    expect(tile.className.split(" ")).toEqual(
      expect.arrayContaining(["bg-neutral-avatar", "h-8", "w-8", "rounded-[7px]", "dark:inset-ring-white/12"]),
    );
    expect(tile.querySelector("svg")).toBeNull();
    // Nor Ollama's own logo, which would make it look like Ollama: its
    // initial on the corner here, the one place the source is drawn.
    expect(ownLogo(avatar())).toBeNull();
    expect(avatar().querySelector("[data-source-badge]")).toHaveTextContent("O");
    expect(avatar().textContent).toBe("O");
  });

  it("draws a cask with no app icon and no logo as the tile with no prompt: an app, a font or a plug-in is no command-line program", async () => {
    const { avatar, queryClient } = renderAvatar({ adapterId: "brew", sourceLabel: "Homebrew", iconKey: quickjot });

    // While its app icon is asked for, and once the answer is that it has none.
    expect(programTile(avatar())?.querySelector("svg")).toBeNull();
    await waitFor(() =>
      expect(queryClient.getQueryState(["artifactIcon", brew, "Cask", "quickjot"])?.status).toBe("success"),
    );
    const tile = programTile(avatar()) as HTMLElement;
    expect(tile).not.toBeNull();
    expect(tile.querySelector("svg")).toBeNull();
    expect(avatar().querySelector("[data-source-badge] [data-logo] path")).toHaveAttribute("d", "M3 3h18v18H3z");
  });

  it("draws the tile with its source's mark, and no prompt, and looks nothing up, when it is given no tool", () => {
    const { avatar } = renderAvatar({ adapterId: "brew", sourceLabel: "Homebrew" });

    // Nothing says it is a program.
    expect(programTile(avatar())).not.toBeNull();
    expect(programTile(avatar())?.querySelector("svg")).toBeNull();
    expect(avatar().querySelector("[data-source-badge] [data-logo]")).toHaveAttribute("data-logo", "glyph");
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  it("draws a raster logo on a white square under a half-point edge", async () => {
    const { avatar, queryClient } = renderAvatar({ adapterId: "brew", sourceLabel: "Homebrew", iconKey: iterm });

    // The cask has no app icon: the pack's logo stays.
    await waitFor(() =>
      expect(queryClient.getQueryState(["artifactIcon", brew, "Cask", "iterm2"])?.status).toBe("success"),
    );
    const logo = ownLogo(avatar()) as HTMLElement;
    expect(logo.tagName).toBe("IMG");
    expect(logo).toHaveAttribute("data-logo", "raster");
    expect(logo).toHaveAttribute("src", "/assets/gh-iterm2.webp");
    expect(logo).toHaveAttribute("alt", "");
    expect(logo.className.split(" ")).toEqual(
      expect.arrayContaining(["bg-white", "outline-[0.5px]", "outline-black/12", "object-contain", "h-8", "rounded-[7px]"]),
    );
    expect(avatar().querySelector("[data-source-badge] [data-logo]")).not.toBeNull();
  });

  it("shows a cask's app icon over its logo once the icon arrives, the source's logo still on its corner", async () => {
    let answer: (icon: string | null) => void = () => {};
    mockInvoke.mockImplementation(
      (cmd: string) =>
        new Promise((resolve) => {
          if (cmd === "artifact_icon") answer = resolve;
        }),
    );
    const { avatar } = renderAvatar({ adapterId: "brew", sourceLabel: "Homebrew", iconKey: iterm });

    // Asked, and not here yet: the pack's logo stands in.
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("artifact_icon", { key: iterm }));
    expect(ownLogo(avatar())).toHaveAttribute("data-logo", "raster");

    await act(async () => answer(APP_ICON));

    await waitFor(() => expect(avatar().querySelector("img[data-app-icon]")).not.toBeNull());
    const icon = avatar().querySelector("img[data-app-icon]") as HTMLElement;
    expect(icon).toHaveAttribute("src", APP_ICON);
    expect(icon).toHaveAttribute("alt", "");
    expect(ownLogo(avatar())).toBeNull();
    expect(avatar()).toHaveAttribute("aria-hidden", "true");
    expect(avatar().querySelector("[data-source-badge] path")).toHaveAttribute("d", "M3 3h18v18H3z");
  });

  it("draws a quiet line's 24px logo with the same 14px badge", () => {
    const { avatar } = renderAvatar({ adapterId: "brew", sourceLabel: "Homebrew", iconKey: jq, size: "sm" });

    expect(ownLogo(avatar())?.className).toContain("h-6");
    expect(ownLogo(avatar())?.className).toContain("rounded-[5px]");
    expect(avatar().querySelector("[data-source-badge] [data-logo]")?.className).toContain("h-3.5");
  });
});
