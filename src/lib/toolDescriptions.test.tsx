import { afterEach, describe, expect, it, vi } from "vitest";
import { act, render, waitFor } from "@testing-library/react";
import { readFileSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import i18n from "../i18n";
import { renderWithProviders } from "../test/setup";
import { toolIconKey } from "./toolIcons";
import { lazyDescriptionTable, useTranslatedDescription, type TranslatedDescription } from "./toolDescriptions";
import type { ArtifactKey, ArtifactKind } from "./types";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const SRC = path.resolve(__dirname, "..");
const TABLE_PATH = path.join(SRC, "assets/tool-descriptions/zh-CN.json");

function key(instanceId: string, kind: ArtifactKind, name: string): ArtifactKey {
  return { instance_id: instanceId, kind, name };
}

const BREW = "brew:/opt/homebrew";

// The accessor the probe last rendered with: the one a row would call.
let lookup: TranslatedDescription = () => null;
function Probe() {
  lookup = useTranslatedDescription();
  return null;
}

async function switchTo(language: string) {
  await act(async () => {
    await i18n.changeLanguage(language);
  });
}

afterEach(async () => {
  await switchTo("en");
});

describe("the built-in Chinese table", () => {
  const text = readFileSync(TABLE_PATH, "utf-8");
  const table: unknown = JSON.parse(text);
  const lines = table as Record<string, string>;

  it("is one object of lines, by key", () => {
    expect(typeof table === "object" && table !== null && !Array.isArray(table)).toBe(true);
    const bad = Object.entries(lines).filter(
      ([, line]) => typeof line !== "string" || line === "" || line !== line.trim() || /[\n\r\t]/.test(line),
    );
    expect(bad, `lines that are not one trimmed line: ${JSON.stringify(bad)}`).toEqual([]);
  });

  it("lists each key once, in order", () => {
    // JSON.parse keeps the last of two lines under one key, silently: the
    // file's own keys, one per line as it is written, say whether it has.
    const written = [...text.matchAll(/^ {2}("(?:[^"\\]|\\.)*"):/gm)].map((match) => JSON.parse(match[1]) as string);
    expect(written).toEqual(Object.keys(lines));
    expect(written).toEqual([...written].sort());
  });

  it("lists every line under a key toolIconKey gives a tool of its source", () => {
    // Each key back to a tool it is the key of: a key toolIconKey never
    // gives -- `pypi:Foo_Bar`, `brew:python@3.13`, a prefix of a source the
    // table was not made for -- would be a line no row ever shows.
    const toolsFor = (toolKey: string): Array<[ArtifactKey, string]> => {
      const colon = toolKey.indexOf(":");
      const name = toolKey.slice(colon + 1);
      switch (toolKey.slice(0, colon)) {
        case "brew":
          return [[key(BREW, "Formula", name), "brew"]];
        case "cask":
          return [[key(BREW, "Cask", name), "brew"]];
        case "npm":
          return [[key("npm:/opt/homebrew", "Package", name), "npm"]];
        case "pypi":
          return [
            [key("pip:/opt/homebrew/bin/python3", "Package", name), "pip"],
            [key("pipx", "Tool", name), "pipx"],
            [key("uv", "Tool", name), "uv"],
          ];
        case "cargo":
          return [[key("cargo:/Users/you/.cargo", "Binary", name), "cargo"]];
        default:
          return [];
      }
    };
    const unreachable = Object.keys(lines).filter((toolKey) => {
      const tools = toolsFor(toolKey);
      return tools.length === 0 || tools.some((tool) => toolIconKey(...tool) !== toolKey);
    });
    expect(unreachable, `no tool is keyed: ${unreachable.join(", ")}`).toEqual([]);
  });

  it("fits in 400 KB", () => {
    // 1000-based, as Finder counts, like the logo pack's budget.
    expect(statSync(TABLE_PATH).size).toBeLessThan(400_000);
  });

  it("is read by a dynamic import in one module, so a build puts it in a chunk of its own", () => {
    const modules = (dir: string): string[] =>
      readdirSync(dir).flatMap((entry) => {
        const full = path.join(dir, entry);
        if (statSync(full).isDirectory()) return modules(full);
        return /\.tsx?$/.test(full) && !/\.test\.tsx?$/.test(full) ? [full] : [];
      });
    const readers = modules(SRC).filter((file) => readFileSync(file, "utf-8").includes("tool-descriptions/"));
    expect(readers.map((file) => path.relative(SRC, file))).toEqual([path.join("lib", "toolDescriptions.ts")]);
    // Its one mention outside a comment: `import(...)`, never `import ... from`.
    const code = (readFileSync(readers[0], "utf-8").match(/[^\n]*tool-descriptions\/[^\n]*/g) ?? []).filter(
      (line) => !/^\s*(\*|\/\/)/.test(line),
    );
    expect(code).toHaveLength(1);
    expect(code[0]).toMatch(/(^|[^.\w])import\("\.\.\/assets\/tool-descriptions\/zh-CN\.json"\)/);
  });

  it("gives a window in Chinese the file's lines", async () => {
    // No provider: the built-in table, as the app reads it. A formula's
    // line and a cask's, the first of each in the file.
    render(<Probe />);
    const first = (prefix: string): [string, string] => {
      const entry = Object.entries(lines).find(([toolKey]) => toolKey.startsWith(prefix));
      if (entry === undefined) throw new Error(`no ${prefix} line`);
      return [entry[0].slice(prefix.length), entry[1]];
    };
    const [formula, formulaLine] = first("brew:");
    const [cask, caskLine] = first("cask:");
    expect(lookup(key(BREW, "Formula", formula), "brew")).toBeNull();

    await switchTo("zh-CN");
    await waitFor(() => expect(lookup(key(BREW, "Formula", formula), "brew")).toBe(formulaLine));
    expect(lookup(key(BREW, "Cask", cask), "brew")).toBe(caskLine);
  });
});

describe("useTranslatedDescription", () => {
  const LINES: Record<string, string> = {
    "brew:python": "解释型编程语言",
    "brew:someone/tap/git": "某人的 git",
    "brew:git": "分布式版本控制系统",
    "cask:firefox@developer-edition": "开发者版火狐",
    "npm:@openai/codex": "OpenAI 的编程助手",
    "pypi:charset-normalizer": "字符编码探测库",
    "cargo:tokei": "代码行数统计工具",
  };
  const table = () => lazyDescriptionTable(async () => LINES);

  it("finds a tool's line under its logo's key: a formula without its @version, a tapped one by its tap", async () => {
    renderWithProviders(<Probe />, { toolDescriptions: table() });
    await switchTo("zh-CN");

    await waitFor(() => expect(lookup(key(BREW, "Formula", "git"), "brew")).toBe("分布式版本控制系统"));
    expect(lookup(key(BREW, "Formula", "python@3.13"), "brew")).toBe("解释型编程语言");
    expect(lookup(key(BREW, "Formula", "someone/tap/git"), "brew")).toBe("某人的 git");
    expect(lookup(key(BREW, "Formula", "someone/tap/wget"), "brew")).toBeNull();
    // A cask's token whole, `@` and all.
    expect(lookup(key(BREW, "Cask", "firefox@developer-edition"), "brew")).toBe("开发者版火狐");
    expect(lookup(key(BREW, "Cask", "firefox"), "brew")).toBeNull();
    expect(lookup(key("npm:/opt/homebrew", "Package", "@openai/codex"), "npm")).toBe("OpenAI 的编程助手");
    expect(lookup(key("cargo:/Users/you/.cargo", "Binary", "tokei"), "cargo")).toBe("代码行数统计工具");
  });

  it("finds a Python package under its PEP 503 name, from pip, pipx and uv alike", async () => {
    renderWithProviders(<Probe />, { toolDescriptions: table() });
    await switchTo("zh-CN");

    await waitFor(() => expect(lookup(key("pipx", "Tool", "charset-normalizer"), "pipx")).toBe("字符编码探测库"));
    expect(lookup(key("pip:/usr/bin/python3", "Package", "Charset_Normalizer"), "pip")).toBe("字符编码探测库");
    expect(lookup(key("uv", "Tool", "charset.normalizer"), "uv")).toBe("字符编码探测库");
  });

  it("has no line for a tool the table does not list, one with no key, or a name on Object's prototype", async () => {
    renderWithProviders(<Probe />, { toolDescriptions: table() });
    await switchTo("zh-CN");

    await waitFor(() => expect(lookup(key(BREW, "Formula", "git"), "brew")).not.toBeNull());
    expect(lookup(key(BREW, "Formula", "wget"), "brew")).toBeNull();
    expect(lookup(key("ollama:http://127.0.0.1:11434", "Model", "llama3.2:3b"), "ollama")).toBeNull();
    expect(lookup(key("gem", "Package", "rails"), "gem")).toBeNull();
    expect(lookup(key(BREW, "Formula", "toString"), "brew")).toBeNull();
    expect(lookup(key("npm:/opt/homebrew", "Package", "constructor"), "npm")).toBeNull();
  });

  it("reads the table only once the window is in Chinese, once, and gives no line in English", async () => {
    const read = vi.fn(async () => LINES);
    renderWithProviders(<Probe />, { toolDescriptions: lazyDescriptionTable(read) });
    const git = key(BREW, "Formula", "git");

    // Rendered, and every effect run: nothing read in English.
    await act(async () => {});
    expect(read).not.toHaveBeenCalled();
    expect(lookup(git, "brew")).toBeNull();

    await switchTo("zh-CN");
    await waitFor(() => expect(lookup(git, "brew")).toBe("分布式版本控制系统"));
    expect(read).toHaveBeenCalledTimes(1);

    await switchTo("en");
    expect(lookup(git, "brew")).toBeNull();

    // Back in Chinese: the lines already read, at once, and not read again.
    await switchTo("zh-CN");
    expect(lookup(git, "brew")).toBe("分布式版本控制系统");
    expect(read).toHaveBeenCalledTimes(1);
  });

  it("gives no line while the table is on its way, nor after it failed to arrive", async () => {
    let fail: (reason: unknown) => void = () => {};
    const read = vi.fn(
      () =>
        new Promise<Record<string, string>>((_resolve, reject) => {
          fail = reject;
        }),
    );
    const logged = vi.spyOn(console, "error").mockImplementation(() => {});
    try {
      renderWithProviders(<Probe />, { toolDescriptions: lazyDescriptionTable(read) });
      await switchTo("zh-CN");
      expect(read).toHaveBeenCalledTimes(1);
      expect(lookup(key(BREW, "Formula", "git"), "brew")).toBeNull();

      await act(async () => {
        fail(new Error("no chunk"));
      });
      expect(lookup(key(BREW, "Formula", "git"), "brew")).toBeNull();
      expect(logged).toHaveBeenCalled();
    } finally {
      logged.mockRestore();
    }
  });
});
