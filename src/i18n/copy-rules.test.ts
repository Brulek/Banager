import { describe, expect, it } from "vitest";
import en from "./en.json";
import zhCN from "./zh-CN.json";

/** Every string in a locale file, with its key. */
function entries(value: unknown, prefix = ""): [string, string][] {
  if (typeof value === "string") return [[prefix, value]];
  if (typeof value !== "object" || value === null) return [];
  return Object.entries(value as Record<string, unknown>).flatMap(([key, child]) =>
    entries(child, prefix ? `${prefix}.${key}` : key),
  );
}

/**
 * The copy table's rules that a string can be checked against on its own
 * (docs/superpowers/2026-09-27-ui-redesign.md, 原则 3): no hedging, no
 * internal words, no explanation in brackets. The rest -- one line in a
 * row, at most two sentences behind an ⓘ, and above all that a shorter
 * sentence stays true -- is the table's, string by string.
 */
describe("the copy rules, over every string in both languages", () => {
  it("never hedges with 多半 or 也可能, and never says PATH, pin or 实例 in Chinese", () => {
    const offenders = entries(zhCN).filter(([, text]) => /多半|也可能|PATH|pin|实例/.test(text));
    expect(offenders).toEqual([]);
  });

  it("keeps brackets for a count and nothing else", () => {
    // 「更新所选（3）」 is a count; 「程序（链接）」 and 「（pin）」 were asides.
    const count = /[（(]\{\{number\}\}[）)]/g;
    const offenders = [...entries(zhCN), ...entries(en)].filter(([, text]) => /[（(]/.test(text.replace(count, "")));
    expect(offenders).toEqual([]);
  });

  it("says nothing about PATH or instances in English either", () => {
    const offenders = entries(en).filter(([, text]) => /\bPATH\b|\binstances?\b/.test(text));
    expect(offenders).toEqual([]);
  });
});
