import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

// What Increase Contrast (System Settings > Accessibility > Display)
// changes, read off index.css: the accent a filled button and the
// selection are drawn in, the focus ring, the secondary text -- and that
// nothing of it reaches the default appearance, which keeps Apple's own
// values.

const CSS = readFileSync(path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../index.css"), "utf-8")
  .replace(/\/\*[\s\S]*?\*\//g, "")
  .replace(/\s+/g, " ");

/** The text of the top-level block that starts with `head` (an at-rule's prelude), braces balanced. */
function block(head: string): string {
  const start = CSS.indexOf(`${head} {`);
  if (start === -1) throw new Error(`index.css has no ${head}`);
  let depth = 0;
  for (let at = CSS.indexOf("{", start); at < CSS.length; at += 1) {
    if (CSS[at] === "{") depth += 1;
    if (CSS[at] === "}") depth -= 1;
    if (depth === 0) return CSS.slice(start, at + 1);
  }
  throw new Error(`${head} never closes`);
}

describe("Increase contrast", () => {
  it("darkens the light appearance's accent, the default blue and the user's own alike, and leaves the dark one's", () => {
    const light = block("@media (prefers-contrast: more) and (prefers-color-scheme: light)");
    expect(light).toContain(":root { --color-accent: #0060df; }");
    expect(light).toContain("@supports (color: AccentColor) { :root { --color-accent: color-mix(in srgb, AccentColor 85%, black); } }");
    const dark = block("@media (prefers-contrast: more) and (prefers-color-scheme: dark)");
    expect(dark).not.toContain("--color-accent");
  });

  it("comes after the user's accent, so that it wins over it", () => {
    const accent = CSS.indexOf("@supports (color: AccentColor) { :root { --color-accent: AccentColor; } }");
    expect(accent).toBeGreaterThan(-1);
    for (const head of [
      "@media (prefers-contrast: more)",
      "@media (prefers-contrast: more) and (prefers-color-scheme: light)",
      "@media (prefers-contrast: more) and (prefers-color-scheme: dark)",
    ]) {
      expect(CSS.indexOf(`${head} {`)).toBeGreaterThan(accent);
    }
  });

  it("draws the focus ring solid in either appearance, and the secondary text at 70%", () => {
    const both = block("@media (prefers-contrast: more)");
    expect(both).toContain("--color-focus: rgb(0 103 244);");
    expect(both).toContain("--color-muted: rgb(0 0 0 / 0.7);");
    const dark = block("@media (prefers-contrast: more) and (prefers-color-scheme: dark)");
    expect(dark).toContain("--color-focus: rgb(26 169 255);");
    expect(dark).toContain("--color-muted: rgb(255 255 255 / 0.7);");
    // Solid: no alpha left in either.
    for (const css of [both, dark]) expect(css).not.toMatch(/--color-focus: rgb\([^)]*\//);
  });

  it("leaves the default appearance Apple's own: the accent #007AFF and the focus ring at half strength", () => {
    expect(CSS).toContain("--color-accent: #007aff;");
    expect(CSS).toContain("--color-focus: rgb(0 103 244 / 0.5);");
    expect(CSS).toContain("--color-focus: rgb(26 169 255 / 0.5);");
    // Nothing outside a prefers-contrast block sets the darker accent.
    const outside = CSS.replace(block("@media (prefers-contrast: more) and (prefers-color-scheme: light)"), "");
    expect(outside).not.toContain("#0060df");
  });

  it("draws a selected row's grey button's words on its white dark enough to read: the accent darkened by 15%", () => {
    // The default blue on white is 4:1; 15% darker, 5.3:1, over the 4.5:1
    // a 13 point word needs -- in either appearance, and darker still
    // under Increase Contrast, which darkens the accent it is mixed from.
    // The rule's declarations, in any order or spacing.
    const rule = CSS.match(
      /\[data-list\]:focus-within \[data-tool-row\]\[data-selected\] button\.bg-fill:enabled ?\{([^}]*)\}/,
    );
    expect(rule).not.toBeNull();
    const body = rule?.[1] ?? "";
    expect(body).toMatch(/background-color: ?#fff ?;/);
    expect(body).toMatch(/(^|;) ?color: ?color-mix\(in srgb, ?var\(--color-accent\) 85%, ?black\) ?(;|$)/);
  });
});
