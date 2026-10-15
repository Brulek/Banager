import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
// The whole front end lives under `src`, so scanning from its root (rather
// than a hard-coded list of subdirectories) means a newly added folder —
// `src/lib`, `src/store`, or anything created later — is covered without
// this file changing.
const SRC_ROOT = path.resolve(__dirname, "..");

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
// not a letter. The minimum is two letters (`[A-Za-z]` plus `{1,}`), so a
// short word like `OK` is still caught rather than slipping through under
// a four-character floor.
const SUSPICIOUS_JSX_TEXT = />\s*[A-Za-z][A-Za-z0-9 ,.'!?:;()-]{1,}\s*</g;

// User-visible copy also hides in attribute strings; every one of these in
// this codebase is written as `aria-label={t("…")}`, so a quoted literal is
// a mistake.
const SUSPICIOUS_ATTRIBUTE = /(aria-label|placeholder|title|alt)="[A-Za-z][^"]{2,}"/g;

// Non-language tokens the two regexes above would legitimately flag:
// symbols, code samples and product/brand names (e.g. a package manager
// name) that must NOT be routed through i18n because they are not
// language, they are literal identifiers that read the same in every
// locale. Each entry is the exact matched substring (including the
// surrounding `>`/`<` for JSX text, or the whole `attr="value"` for an
// attribute) so the allowlist can't accidentally hide a real hit that
// merely contains the same word. Empty for now: scanning the whole of
// `src` at the two-letter floor found no untranslated user-visible text
// and no non-language token either — add an entry here only when a
// genuine non-language literal is found, never to silence a real hit.
const ALLOWED_LITERALS = new Set<string>([]);

const files = collectTsxFiles(SRC_ROOT);

describe("no literal user-visible strings in JSX", () => {
  it.each(files)("has no literal JSX text in %s", (file) => {
    const source = readFileSync(file, "utf-8");
    const jsxText = (source.match(SUSPICIOUS_JSX_TEXT) ?? []).filter(
      (m) => m.slice(1, -1).trim().length > 0,
    );
    const attributes = source.match(SUSPICIOUS_ATTRIBUTE) ?? [];
    const real = [...jsxText, ...attributes].filter((m) => !ALLOWED_LITERALS.has(m));
    expect(real, `${file} has literal text: ${real.join(" | ")}`).toEqual([]);
  });

  // App.tsx and main.tsx are the only non-test .tsx files at the src root,
  // outside components/ and pages/. collectTsxFiles recurses into
  // subfolders (components/ui is reached from components/), so a walk
  // started at those two folders misses exactly these two root files;
  // finding both proves the walk starts at the root.
  it("scans the two non-test .tsx files at the src root, App.tsx and main.tsx", () => {
    for (const name of ["App.tsx", "main.tsx"]) {
      expect(files, `${name} missing from the scanned list`).toContain(path.join(SRC_ROOT, name));
    }
  });

  it("catches a two-letter JSX literal like <p>OK</p>", () => {
    const hits = "<p>OK</p>".match(SUSPICIOUS_JSX_TEXT) ?? [];
    expect(hits).toContain(">OK<");
  });
});
