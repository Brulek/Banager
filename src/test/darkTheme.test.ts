import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { compile } from "tailwindcss";
import { beforeAll, describe, expect, it } from "vitest";

// What dark mode draws, read off the stylesheet as Tailwind builds it: a
// value index.css sets for dark mode is worth nothing if the class that
// should use it was built with the light one written into it.

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

/** index.css as Tailwind builds it for `candidates`, comments dropped and whitespace run together. */
async function build(candidates: string[]): Promise<string> {
  const compiler = await compile(readFileSync(path.join(ROOT, "src/index.css"), "utf-8"), {
    base: path.join(ROOT, "src"),
    async loadStylesheet(id, base) {
      const file = id === "tailwindcss" ? path.join(ROOT, "node_modules/tailwindcss/index.css") : path.resolve(base, id);
      return { path: file, base: path.dirname(file), content: readFileSync(file, "utf-8") };
    },
  });
  return compiler
    .build(candidates)
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/\s+/g, " ");
}

/** The declarations of the first rule for exactly `selector` in `css`, as property to value. */
function rule(css: string, selector: string): Record<string, string> {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const match = new RegExp(`(?:^|[{};])\\s*${escaped}\\s*\\{([^{}]*)\\}`).exec(css);
  if (match === null) throw new Error(`no rule for ${selector}`);
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

/** A variable's value in the theme Tailwind emits: light mode's. */
function lightTheme(css: string, name: string): string {
  const start = css.indexOf("@layer theme { :root, :host {");
  if (start === -1) throw new Error("no theme");
  const theme = css.slice(start, css.indexOf("@layer base", start));
  const match = new RegExp(`[{;] ${name}: ([^;{}]+);`).exec(theme);
  if (match === null) throw new Error(`the theme has no ${name}`);
  return match[1].trim();
}

/** The declarations index.css gives `:root` in dark mode. */
function darkRoot(css: string): Record<string, string> {
  const start = css.indexOf("@media (prefers-color-scheme: dark) { :root {");
  if (start === -1) throw new Error("no dark :root");
  return rule(css.slice(start + "@media (prefers-color-scheme: dark) {".length), ":root");
}

describe("the dark theme, as built", () => {
  let css = "";
  beforeAll(async () => {
    css = await build(["shadow-dialog", "shadow-menu", "bg-surface", "bg-popover", "bg-switch-off", "bg-switch-knob"]);
  });

  it("gives a dialog, a menu and a popover the shadow of the mode they are in, not the light one", () => {
    // The classes read the variables, so dark mode's apply: a light
    // hairline ring round the dialog, not a black one lost on #1E1E1E.
    expect(rule(css, ".shadow-dialog")).toEqual({ "box-shadow": "var(--shadow-dialog)" });
    expect(rule(css, ".shadow-menu")).toEqual({ "box-shadow": "var(--shadow-menu)" });
    expect(rule(css, ":root")["--shadow-dialog"]).toBe("0 0 0 0.5px rgb(0 0 0 / 0.2), 0 16px 48px rgb(0 0 0 / 0.25)");
    expect(rule(css, ":root")["--shadow-menu"]).toBe("0 0 0 0.5px rgb(0 0 0 / 0.15), 0 6px 20px rgb(0 0 0 / 0.15)");
    expect(darkRoot(css)["--shadow-dialog"]).toMatch(/^0 0 0 0\.5px rgb\(255 255 255 \/ 0\.15\), /);
    expect(darkRoot(css)["--shadow-menu"]).toMatch(/^0 0 0 0\.5px rgb\(255 255 255 \/ 0\.15\), /);
  });

  it("draws a dialog, a menu and a popover a step lighter than the sidebar", () => {
    expect(darkRoot(css)["--color-sidebar"]).toBe("#282828");
    expect(darkRoot(css)["--color-surface"]).toBe("#2c2c2c");
    expect(darkRoot(css)["--color-popover"]).toBe("#2c2c2c");
  });

  it("draws a switch's knob a light grey in the dark, not the page's brightest white, on a quieter track", () => {
    expect(lightTheme(css, "--color-switch-knob")).toBe("#ffffff");
    expect(lightTheme(css, "--color-switch-off")).toBe("rgb(0 0 0 / 0.11)");
    expect(darkRoot(css)["--color-switch-knob"]).toBe("#e2e2e2");
    expect(darkRoot(css)["--color-switch-off"]).toBe("rgb(255 255 255 / 0.1)");
    expect(rule(css, ".bg-switch-knob")).toEqual({ "background-color": "var(--color-switch-knob)" });
  });
});
