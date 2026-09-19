import { describe, expect, it } from "vitest";
import en from "./en.json";
import zhCN from "./zh-CN.json";

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
