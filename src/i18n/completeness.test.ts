import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import en from "./en.json";
import zhCN from "./zh-CN.json";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

const PLURAL_SUFFIXES = ["_zero", "_one", "_two", "_few", "_many", "_other"];

function stripPluralSuffix(key: string): string {
  const suffix = PLURAL_SUFFIXES.find((s) => key.endsWith(s));
  return suffix ? key.slice(0, -suffix.length) : key;
}

function flattenKeys(value: unknown, prefix = ""): string[] {
  if (typeof value !== "object" || value === null) {
    return [prefix];
  }
  return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) =>
    flattenKeys(child, prefix ? `${prefix}.${key}` : key),
  );
}

function normalizedKeySet(resource: unknown): Set<string> {
  return new Set(flattenKeys(resource).map(stripPluralSuffix));
}

describe("i18n key parity", () => {
  it("has the same set of keys in en.json and zh-CN.json", () => {
    const enKeys = normalizedKeySet(en);
    const zhKeys = normalizedKeySet(zhCN);

    const missingInZh = [...enKeys].filter((k) => !zhKeys.has(k)).sort();
    const missingInEn = [...zhKeys].filter((k) => !enKeys.has(k)).sort();

    expect(missingInZh, `zh-CN.json is missing: ${missingInZh.join(", ")}`).toEqual([]);
    expect(missingInEn, `en.json is missing: ${missingInEn.join(", ")}`).toEqual([]);
  });
});

// Every `.ts`/`.tsx` that ships, joined into one haystack. Tests are left
// out on purpose: a key that only a test still looks up is exactly the
// orphan this is hunting for.
const SRC_DIR = path.resolve(__dirname, "..");

function collectSourceFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((entry) => {
    const full = path.join(dir, entry);
    if (statSync(full).isDirectory()) return collectSourceFiles(full);
    if (!/\.tsx?$/.test(full)) return [];
    if (/\.test\.tsx?$/.test(full)) return [];
    return [full];
  });
}

const source = collectSourceFiles(SRC_DIR)
  .map((file) => readFileSync(file, "utf-8"))
  .join("\n");

/**
 * Whether anything that ships still looks this key up.
 *
 * Most call sites spell the key out, so the first check finds them. Two
 * shapes compose one at runtime and both are in use here:
 *
 * - a static head and an interpolated tail -- `t(`nav.${p}`)`,
 *   `t(`operations.kind.${current.kind}`)`. The head plus `.${` is enough
 *   to claim every key under it, which is as precise as this can be: the
 *   tail is a Rust variant name that no amount of grepping will enumerate.
 * - an interpolated head and a static tail -- `t(`${prefix}.description`)`
 *   over a lookup table (`READ_ONLY_NOTICE_KEYS`). Both halves have to be
 *   present: the head as a whole string literal, the tail spelled out
 *   after an interpolation.
 */
function isReferenced(key: string): boolean {
  if (source.includes(key)) return true;
  const segments = key.split(".");
  for (let i = 1; i < segments.length; i += 1) {
    const head = segments.slice(0, i).join(".");
    const tail = segments.slice(i).join(".");
    if (source.includes(`${head}.\${`)) return true;
    if (source.includes(`"${head}"`) && source.includes(`}.${tail}`)) return true;
  }
  return false;
}

describe("i18n keys are reachable", () => {
  /**
   * This project's recurring failure mode is a field, variant, badge or
   * key that is defined, mirrored and never rendered. The parity test
   * above cannot catch one: a key nothing reads stays green as long as
   * both locales carry it. Five review rounds found five of these by
   * hand; this finds the next one.
   */
  it("has no key in en.json that nothing renders", () => {
    const orphans = [...normalizedKeySet(en)].filter((key) => !isReferenced(key)).sort();

    expect(orphans, `nothing looks these up: ${orphans.join(", ")}`).toEqual([]);
  });
});

function flattenEntries(value: unknown, prefix = ""): [string, string][] {
  if (typeof value === "string") {
    return [[prefix, value]];
  }
  if (typeof value !== "object" || value === null) {
    return [];
  }
  return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) =>
    flattenEntries(child, prefix ? `${prefix}.${key}` : key),
  );
}

const CJK = "\\u4e00-\\u9fff";
// A half-width comma, colon or parenthesis with CJK text pressed against
// both sides is the most recognisable machine-translation tell in
// Simplified Chinese -- mainland punctuation (GB/T 15834) uses the
// full-width forms ， ： （ ） there instead. This does not flag ASCII next
// to an interpolation, a Latin word or a domain name (crates.io,
// nodejs.org): those are legitimate half-width uses this file also
// contains, and only a CJK character on *both* sides marks Chinese prose.
const ASCII_PUNCT_BETWEEN_CJK = new RegExp(`[${CJK}][,:()][${CJK}]`);

describe("zh-CN uses full-width punctuation in Chinese prose", () => {
  /**
   * The Chinese translation drifted back toward untouched machine
   * translation over several review rounds: half-width ， ： （ ） creeping
   * in between Chinese characters instead of the full-width forms the rest
   * of the file already uses. This guards the fix so it can't come back
   * silently string by string.
   */
  it("has no half-width , : ( ) sitting between two CJK characters", () => {
    const offenders = flattenEntries(zhCN)
      .filter(([, value]) => ASCII_PUNCT_BETWEEN_CJK.test(value))
      .map(([key, value]) => `${key}: ${value}`);

    expect(offenders, `half-width punctuation inside Chinese prose:\n${offenders.join("\n")}`).toEqual([]);
  });
});
