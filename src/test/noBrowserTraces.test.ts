import { readFileSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import tauriConfig from "../../src-tauri/tauri.conf.json";
import capability from "../../src-tauri/capabilities/default.json";

// Where a web page's habits would show through the window: the page's
// stylesheet, what the components ask of Tailwind, and the web view's
// configuration. Which right-click keeps a menu is src/lib/contextMenu.ts's
// to say, and which text selects is each component's.

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

/** index.css with its comments dropped and its whitespace run together. */
const CSS = readFileSync(path.join(ROOT, "src/index.css"), "utf-8")
  .replace(/\/\*[\s\S]*?\*\//g, "")
  .replace(/\s+/g, " ");

/**
 * The declarations of index.css's rule for exactly `selector` -- `html,
 * body` is a rule of its own, not `body`'s -- as property to value.
 */
function rule(selector: string): Record<string, string> {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = new RegExp(`(?:^|[{};])\\s*${escaped}\\s*\\{([^{}]*)\\}`).exec(CSS);
  if (match === null) throw new Error(`index.css has no rule for ${selector}`);
  return Object.fromEntries(
    match[1]
      .split(";")
      .map((declaration) => declaration.trim())
      .filter((declaration) => declaration !== "")
      .map((declaration) => {
        const colon = declaration.indexOf(":");
        return [declaration.slice(0, colon).trim(), declaration.slice(colon + 1).trim()];
      }),
  );
}

/** Every component and module under src, tests left out. */
function sources(dir = path.join(ROOT, "src")): string[] {
  return readdirSync(dir).flatMap((entry) => {
    const full = path.join(dir, entry);
    if (statSync(full).isDirectory()) return sources(full);
    return /\.tsx?$/.test(full) && !/\.test\.tsx?$/.test(full) ? [full] : [];
  });
}

describe("the page", () => {
  it("selects no text but what is marked `select-text`, and a text field's", () => {
    expect(rule("body")).toMatchObject({ "-webkit-user-select": "none", "user-select": "none" });
    expect(rule("input, textarea")).toEqual({ "-webkit-user-select": "text", "user-select": "text" });
  });

  it("highlights selected text as a Mac does", () => {
    expect(rule("::selection")).toEqual({ "background-color": "var(--color-selection)" });
    // selectedTextBackgroundColor, light and dark.
    expect(CSS).toContain("--color-selection: #b3d7ff;");
    expect(CSS).toContain("--color-selection: #3f638b;");
  });

  it("breaks a paragraph's lines so that its last is not left with a word or two", () => {
    expect(rule("body")).toMatchObject({ "text-wrap": "pretty" });
  });

  it("lets no image or link be dragged out of the window", () => {
    expect(rule("img, a")).toEqual({ "-webkit-user-drag": "none" });
  });

  it("does not bounce as a whole when a scroll runs past its edge", () => {
    expect(rule("html, body")).toEqual({ "overscroll-behavior": "none" });
  });

  it("gives a button the arrow, not the hand", () => {
    // Tailwind v4's preflight leaves a button the browser's own cursor, the
    // arrow; v3's gave it the hand, which a Mac app's buttons never show.
    const preflight = readFileSync(path.join(ROOT, "node_modules/tailwindcss/preflight.css"), "utf-8");
    expect(preflight).not.toMatch(/cursor:\s*pointer/);
    expect(CSS).not.toMatch(/cursor:\s*pointer/);
    // Links in text could keep the hand; the app has none.
    const pointing = sources().filter((file) => readFileSync(file, "utf-8").includes("cursor-pointer"));
    expect(pointing.map((file) => path.relative(ROOT, file))).toEqual([]);
  });

  it("rings the focus for the keyboard only", () => {
    // `focus-visible:`, never `focus:`, for a ring or an outline: a click
    // draws neither.
    const ringing = sources().filter((file) =>
      /(^|[\s"'`])focus:(ring|outline(?!-none))/m.test(readFileSync(file, "utf-8")),
    );
    expect(ringing.map((file) => path.relative(ROOT, file))).toEqual([]);
    expect(CSS).not.toMatch(/:focus\b(?!-)/);
  });
});

describe("hover and focus", () => {
  /** Each source file that holds `pattern`, with how many times. */
  function holding(pattern: RegExp): Record<string, number> {
    const found: Record<string, number> = {};
    for (const file of sources()) {
      const count = (readFileSync(file, "utf-8").match(pattern) ?? []).length;
      if (count > 0) found[path.relative(ROOT, file)] = count;
    }
    return found;
  }

  it("draws one focus ring for everything, the page's: 3px in the focus colour, around the edge", () => {
    expect(rule(":focus-visible")).toEqual({ outline: "3px solid var(--color-focus)", "outline-offset": "0" });
    // No component draws a ring of its own for the focus.
    expect(holding(/focus-visible:ring|ring-offset/g)).toEqual({});
  });

  it("fades no colour in and out, as a web page's buttons do", () => {
    expect(holding(/transition-colors/g)).toEqual({});
  });

  it("underlines no link, under the pointer or not", () => {
    expect(holding(/hover:underline/g)).toEqual({});
  });

  it("lights up nothing under the pointer but a toolbar's icon button and a menu's items", () => {
    // Rows, the sidebar, links and push buttons stay as they are; the
    // icon buttons (`ICON_BUTTON`, and `SMALL_ICON_BUTTON` for the
    // operation bar) take the quietest fill.
    const fills = holding(/hover:bg-/g);
    expect(Object.values(fills).reduce((total, count) => total + count, 0)).toBeLessThanOrEqual(4);
    expect(Object.keys(fills)).toEqual(["src/components/ui/controls.ts"]);
  });

  it("paints no button red: a destructive action the user chose is the default button", () => {
    expect(holding(/\bbg-danger\b/g)).toEqual({});
  });
});

describe("the web view", () => {
  it("has no browser zoom on ⌘+ and ⌘−", () => {
    // Off unless a window turns it on; on, it would also need the page to
    // be allowed to zoom its web view, which it is not.
    for (const window of tauriConfig.app.windows) {
      expect((window as { zoomHotkeysEnabled?: boolean }).zoomHotkeysEnabled ?? false).toBe(false);
    }
    expect(capability.permissions).not.toContain("core:webview:allow-set-webview-zoom");
  });
});
