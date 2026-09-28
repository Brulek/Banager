import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

// Where a web page's habits would show through the window: the page's
// stylesheet. Which text selects is each component's to say.

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

describe("the page", () => {
  it("selects no text but what is marked `select-text`, and a text field's", () => {
    expect(rule("body")).toMatchObject({ "-webkit-user-select": "none", "user-select": "none" });
    expect(rule("input, textarea")).toEqual({ "-webkit-user-select": "text", "user-select": "text" });
  });

  it("highlights selected text in a tint of the accent", () => {
    expect(rule("::selection")).toEqual({ "background-color": "var(--color-selection)" });
    expect(CSS).toContain("--color-selection: color-mix(in srgb, var(--color-accent) 30%, transparent);");
    // Dark mode's, of the lighter accent it gives text (`--color-accent-text`).
    expect(CSS).toContain("--color-selection: color-mix(in srgb, var(--color-accent-text) 40%, transparent);");
  });
});
