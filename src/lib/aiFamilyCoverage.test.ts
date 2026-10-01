import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import table from "../../crates/banager-core/data/ai-tools.json";
import en from "../i18n/en.json";
import zhCN from "../i18n/zh-CN.json";
import { standaloneSummaryKey } from "./sources";
import { resolveToolIcon, toolIconKey } from "./toolIcons";
import type { ArtifactKey } from "./types";

/**
 * Every member of the AI-tool families (crates/banager-core/data/
 * ai-tools.json) -- what the 「AI工具」 filter shows -- has a logo of its
 * own or its maker's, and a line saying what it is in both languages, so
 * that no row under the filter reads "npm package" or shows only its
 * source's logo. A family added to the table without them fails here.
 */

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const TABLES = path.resolve(__dirname, "../assets/tool-descriptions");
const lines = (file: string) =>
  new Map(Object.entries(JSON.parse(readFileSync(path.join(TABLES, file), "utf-8")) as Record<string, string>));
const CHINESE = lines("zh-CN.json");
const ENGLISH = lines("en.json");

type Source = "npm" | "formula" | "cask" | "pypi" | "standalone";

interface Member {
  family: string;
  source: Source;
  name: string;
}

const MEMBERS: Member[] = table.families.flatMap((family) =>
  family.members.map((member) => ({ family: family.id, source: member.source as Source, name: member.name })),
);

/** A tool of `member`'s, as a snapshot lists it: its key and its source's adapter id. */
function toolOf({ source, name }: Member): [ArtifactKey, string] {
  switch (source) {
    case "npm":
      return [{ instance_id: "npm:/opt/homebrew", kind: "Package", name }, "npm"];
    case "formula":
      return [{ instance_id: "brew:/opt/homebrew", kind: "Formula", name }, "brew"];
    case "cask":
      return [{ instance_id: "brew:/opt/homebrew", kind: "Cask", name }, "brew"];
    case "pypi":
      return [{ instance_id: "pipx", kind: "Tool", name }, "pipx"];
    case "standalone":
      return [{ instance_id: `standalone-${name}`, kind: "Binary", name }, `standalone-${name}`];
  }
}

/** The i18n string at a dotted key, or undefined. */
function message(messages: unknown, dotted: string): unknown {
  return dotted.split(".").reduce<unknown>((node, part) => {
    if (typeof node !== "object" || node === null || !Object.prototype.hasOwnProperty.call(node, part)) {
      return undefined;
    }
    return (node as Record<string, unknown>)[part];
  }, messages);
}

const label = (member: Member) => `${member.family}: ${member.source} ${member.name}`;

describe("the AI-tool families' members", () => {
  it("are listed, every source among them", () => {
    expect(MEMBERS.length).toBeGreaterThan(30);
    expect(new Set(MEMBERS.map((member) => member.source))).toEqual(
      new Set(["npm", "formula", "cask", "pypi", "standalone"]),
    );
  });

  it("each have a logo of their own or their maker's, not only their source's", () => {
    const missing = MEMBERS.filter((member) => resolveToolIcon(...toolOf(member)) === null).map(label);
    expect(missing, `no logo: ${missing.join(", ")}`).toEqual([]);
  });

  it("each have a line in Chinese", () => {
    // A tool with its own installer says what it is in the locale files
    // (its summary); every other member, in the Chinese table.
    const missing = MEMBERS.filter((member) => {
      const [key, adapterId] = toolOf(member);
      const summary = standaloneSummaryKey(adapterId);
      if (member.source === "standalone") {
        return summary === null || typeof message(zhCN, summary) !== "string";
      }
      const toolKey = toolIconKey(key, adapterId);
      return toolKey === null || !CHINESE.has(toolKey);
    }).map(label);
    expect(missing, `no Chinese line: ${missing.join(", ")}`).toEqual([]);
  });

  it("each have a line in English", () => {
    // Homebrew gives its own words in English for a formula and a cask,
    // and the English table holds none for them (toolDescriptions.test):
    // npm and PyPI give Banager none, so their members need the table's.
    const missing = MEMBERS.filter((member) => {
      const [key, adapterId] = toolOf(member);
      if (member.source === "standalone") {
        const summary = standaloneSummaryKey(adapterId);
        return summary === null || typeof message(en, summary) !== "string";
      }
      if (member.source === "formula" || member.source === "cask") return false;
      const toolKey = toolIconKey(key, adapterId);
      return toolKey === null || !ENGLISH.has(toolKey);
    }).map(label);
    expect(missing, `no English line: ${missing.join(", ")}`).toEqual([]);
  });

  it("are found under the keys the tables and the pack use, a PyPI name as PEP 503 normalises it", () => {
    const kimi = MEMBERS.find((member) => member.source === "pypi" && member.name === "kimi-cli");
    expect(kimi).toBeDefined();
    expect(toolIconKey(...toolOf(kimi as Member))).toBe("pypi:kimi-cli");
    expect(CHINESE.get("pypi:kimi-cli")).toContain("Kimi Code");
    expect(ENGLISH.get("pypi:kimi-cli")).toContain("Kimi Code");
  });
});
