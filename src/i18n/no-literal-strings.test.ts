import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const SCAN_DIRS = ["../components", "../pages"].map((p) => path.resolve(__dirname, p));

function collectTsxFiles(dir: string): string[] {
  const entries = readdirSync(dir);
  return entries.flatMap((entry) => {
    const full = path.join(dir, entry);
    const stat = statSync(full);
    if (stat.isDirectory()) return collectTsxFiles(full);
    if (full.endsWith(".tsx") && !full.endsWith(".test.tsx")) return [full];
    return [];
  });
}

// Matches a JSX text child sitting directly between two tags, e.g.
// `<p>Nothing installed yet</p>`, including the wrapped form prettier
// produces for long copy (`>\n  Nothing installed yet\n</p>`) — hence `\s*`,
// not `[ \t]*`, at both ends. A child that is itself an expression
// (`<p>{t("x")}</p>`) never matches, because the `>` is followed by `{`,
// not a letter.
const SUSPICIOUS_JSX_TEXT = />\s*[A-Za-z][A-Za-z0-9 ,.'!?:;()-]{3,}\s*</g;

// User-visible copy also hides in attribute strings; every one of these in
// this codebase is written as `aria-label={t("…")}`, so a quoted literal is
// a mistake.
const SUSPICIOUS_ATTRIBUTE = /(aria-label|placeholder|title|alt)="[A-Za-z][^"]{2,}"/g;

const files = SCAN_DIRS.flatMap((dir) => collectTsxFiles(dir));

describe("no literal user-visible strings in JSX", () => {
  it.each(files)("has no literal JSX text in %s", (file) => {
    const source = readFileSync(file, "utf-8");
    const jsxText = (source.match(SUSPICIOUS_JSX_TEXT) ?? []).filter(
      (m) => m.slice(1, -1).trim().length > 0,
    );
    const attributes = source.match(SUSPICIOUS_ATTRIBUTE) ?? [];
    const real = [...jsxText, ...attributes];
    expect(real, `${file} has literal text: ${real.join(" | ")}`).toEqual([]);
  });
});
