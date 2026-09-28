import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { GLYPH_INK_DARK, GLYPH_INK_LIGHT, loadToolIcons, type ToolIconPack } from "../lib/toolIcons";
import type { ArtifactKey } from "../lib/types";
import { renderWithProviders } from "../test/setup";
import { ToolAvatar } from "./ToolAvatar";

/**
 * A pack of this test's own, so that nothing here depends on what the
 * built-in pack lists. Homebrew and Claude Code have a logo; npm and pipx
 * have none, so their badges and avatars are their initials.
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
  },
  rasters: { "gh-iterm2": { file: "gh-iterm2.webp", title: "iTerm2" } },
  tools: {
    "brew:jq": "si-jq",
    "cask:iterm2": "gh-iterm2",
    "npm:typescript": "si-typescript",
    // A tool with its own installer: its logo is its source's.
    "standalone:claude": "si-claude",
  },
  sources: { brew: "si-homebrew", "standalone-claude": "si-claude" },
};
const toolIcons = loadToolIcons(PACK, new Map([["gh-iterm2.webp", "/assets/gh-iterm2.webp"]]));

const brew = "brew:/opt/homebrew";
const jq: ArtifactKey = { instance_id: brew, kind: "Formula", name: "jq" };
const wget: ArtifactKey = { instance_id: brew, kind: "Formula", name: "wget" };
const iterm: ArtifactKey = { instance_id: brew, kind: "Cask", name: "iterm2" };
const typescript: ArtifactKey = { instance_id: "npm:/opt/homebrew", kind: "Package", name: "typescript" };
const cowsay: ArtifactKey = { instance_id: "pipx", kind: "Tool", name: "cowsay" };
const claude: ArtifactKey = { instance_id: "standalone-claude", kind: "Binary", name: "claude" };
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
    expect(logo?.className).toContain("rounded-[9px]");

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

  it("puts the source's initial on the corner of the tool's logo when the source has no logo", () => {
    const { avatar } = renderAvatar({ adapterId: "npm", sourceLabel: "npm", iconKey: typescript });

    expect(ownLogo(avatar())?.querySelector("path")).toHaveAttribute("d", "M2 2h20v20H2z");
    const badge = avatar().querySelector("[data-source-badge]");
    expect(badge).toHaveTextContent("N");
    expect(badge?.firstElementChild?.className).toContain("bg-source-npm");
    expect(badge?.firstElementChild?.className).toContain("h-3.5");
  });

  it("shows the source's logo, with nothing on its corner, for a tool the pack has no logo for", () => {
    const { avatar } = renderAvatar({ adapterId: "brew", sourceLabel: "Homebrew", iconKey: wget });

    expect(avatar()).toHaveAttribute("data-logo", "glyph");
    expect(avatar().querySelector("path")).toHaveAttribute("d", "M3 3h18v18H3z");
    expect(avatar().className).toContain("h-8");
    expect(avatar().querySelector("[data-source-badge]")).toBeNull();
  });

  it("shows the source's initial, with nothing on its corner, when neither the tool nor its source has a logo", () => {
    const { avatar } = renderAvatar({ adapterId: "pipx", sourceLabel: "pipx", iconKey: cowsay });

    expect(avatar()).toHaveTextContent("P");
    expect(avatar().className).toContain("bg-source-python");
    expect(avatar().querySelector("[data-logo]")).toBeNull();
    expect(avatar().querySelector("[data-source-badge]")).toBeNull();
  });

  it("shows a tool whose logo is its source's own as the source, with nothing on its corner", () => {
    const { avatar } = renderAvatar({ adapterId: "standalone-claude", sourceLabel: "Claude Code", iconKey: claude });

    expect(avatar()).toHaveAttribute("data-logo", "glyph");
    expect(avatar().querySelector("path")).toHaveAttribute("d", "M4 4h16v16H4z");
    expect(avatar().querySelector("[data-source-badge]")).toBeNull();
  });

  it("shows the source's avatar alone, and looks nothing up, when it is given no tool", () => {
    const { avatar } = renderAvatar({ adapterId: "brew", sourceLabel: "Homebrew" });

    expect(avatar()).toHaveAttribute("data-logo", "glyph");
    expect(avatar().querySelector("[data-source-badge]")).toBeNull();
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  it("draws a raster logo on a white square inside the border colour's hairline", async () => {
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
      expect.arrayContaining(["bg-white", "border", "border-border", "object-contain", "h-8", "rounded-[9px]"]),
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
    expect(ownLogo(avatar())?.className).toContain("rounded-[7px]");
    expect(avatar().querySelector("[data-source-badge] [data-logo]")?.className).toContain("h-3.5");
  });
});
