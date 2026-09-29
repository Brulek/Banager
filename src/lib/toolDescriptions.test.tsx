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
const TABLES = path.join(SRC, "assets/tool-descriptions");

/** A built-in table's file: where it is, its text, and its lines by key. */
function builtIn(file: string): { tablePath: string; text: string; lines: Record<string, string> } {
  const tablePath = path.join(TABLES, file);
  const text = readFileSync(tablePath, "utf-8");
  return { tablePath, text, lines: JSON.parse(text) as Record<string, string> };
}

function key(instanceId: string, kind: ArtifactKind, name: string): ArtifactKey {
  return { instance_id: instanceId, kind, name };
}

const BREW = "brew:/opt/homebrew";
const NPM = "npm:/opt/homebrew";
const CARGO = "cargo:/Users/you/.cargo";

/**
 * The tools a key is the key of, each with its source's adapter id: a
 * key toolIconKey never gives -- `pypi:Foo_Bar`, `brew:python@3.13`, a
 * prefix of a source no table is made for -- would be a line no row ever
 * shows.
 */
function toolsFor(toolKey: string): Array<[ArtifactKey, string]> {
  const colon = toolKey.indexOf(":");
  const name = toolKey.slice(colon + 1);
  switch (toolKey.slice(0, colon)) {
    case "brew":
      return [[key(BREW, "Formula", name), "brew"]];
    case "cask":
      return [[key(BREW, "Cask", name), "brew"]];
    case "npm":
      return [[key(NPM, "Package", name), "npm"]];
    case "pypi":
      return [
        [key("pip:/opt/homebrew/bin/python3", "Package", name), "pip"],
        [key("pipx", "Tool", name), "pipx"],
        [key("uv", "Tool", name), "uv"],
      ];
    case "cargo":
      return [[key(CARGO, "Binary", name), "cargo"]];
    default:
      return [];
  }
}

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

describe.each([
  // Homebrew's formulae and casks, and npm, PyPI and crates.io packages.
  { file: "zh-CN.json", prefixes: ["brew", "cask", "npm", "pypi", "cargo"] },
  // Only the packages whose sources give Canager no description: npm's,
  // PyPI's (pip, pipx and uv) and crates.io's. Homebrew gives its own
  // words in English.
  { file: "en.json", prefixes: ["npm", "pypi", "cargo"] },
])("the built-in table $file", ({ file, prefixes }) => {
  const { tablePath, text, lines } = builtIn(file);
  const table: unknown = JSON.parse(text);

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

  it(`lists every line under a key toolIconKey gives a tool of its source, for ${prefixes.join(", ")} only`, () => {
    const unreachable = Object.keys(lines).filter((toolKey) => {
      const tools = toolsFor(toolKey);
      return (
        !prefixes.includes(toolKey.slice(0, toolKey.indexOf(":"))) ||
        tools.length === 0 ||
        tools.some((tool) => toolIconKey(...tool) !== toolKey)
      );
    });
    expect(unreachable, `no tool is keyed: ${unreachable.join(", ")}`).toEqual([]);
  });

  it("puts no space where Chinese meets a Latin letter or digit", () => {
    // The window spaces them itself (`text-autospace`, or `autospace`
    // where the web view cannot), as it does the copy's.
    const spaced = Object.entries(lines).filter(([, line]) =>
      /\p{Script=Han} +[A-Za-z0-9]|[A-Za-z0-9] +\p{Script=Han}/u.test(line),
    );
    expect(spaced.map(([toolKey]) => toolKey)).toEqual([]);
  });

  it("fits in 400 KB", () => {
    // 1000-based, as Finder counts, like the logo pack's budget.
    expect(statSync(tablePath).size).toBeLessThan(400_000);
  });
});

it("reads each built-in table by a dynamic import in one module, so a build puts each in a chunk of its own", () => {
  const modules = (dir: string): string[] =>
    readdirSync(dir).flatMap((entry) => {
      const full = path.join(dir, entry);
      if (statSync(full).isDirectory()) return modules(full);
      return /\.tsx?$/.test(full) && !/\.test\.tsx?$/.test(full) ? [full] : [];
    });
  const readers = modules(SRC).filter((file) => readFileSync(file, "utf-8").includes("tool-descriptions/"));
  expect(readers.map((file) => path.relative(SRC, file))).toEqual([path.join("lib", "toolDescriptions.ts")]);
  // Its mentions outside a comment: an `import(...)` of each file in the
  // folder, once each, and never `import ... from`.
  const code = (readFileSync(readers[0], "utf-8").match(/[^\n]*tool-descriptions\/[^\n]*/g) ?? []).filter(
    (line) => !/^\s*(\*|\/\/)/.test(line),
  );
  const imported = code.map(
    (line) => /(?:^|[^.\w])import\("\.\.\/assets\/tool-descriptions\/([^"/]+)"\)/.exec(line)?.[1] ?? line,
  );
  expect(imported.sort()).toEqual(
    readdirSync(TABLES)
      .filter((file) => file.endsWith(".json"))
      .sort(),
  );
});

describe("the built-in tables, as a window reads them", () => {
  const chinese = builtIn("zh-CN.json").lines;
  const english = builtIn("en.json").lines;
  // The first line in `lines` under `prefix`: the tool's name, and its line.
  const first = (lines: Record<string, string>, prefix: string): [string, string] => {
    const entry = Object.entries(lines).find(([toolKey]) => toolKey.startsWith(prefix));
    if (entry === undefined) throw new Error(`no ${prefix} line`);
    return [entry[0].slice(prefix.length), entry[1]];
  };

  it("gives a window in Chinese the Chinese file's lines", async () => {
    // No provider: the built-in tables, as the app reads them. A formula's
    // line and a cask's, the first of each in the file; in English, none.
    render(<Probe />);
    const [formula, formulaLine] = first(chinese, "brew:");
    const [cask, caskLine] = first(chinese, "cask:");
    expect(lookup(key(BREW, "Formula", formula), "brew")).toBeNull();

    await switchTo("zh-CN");
    await waitFor(() => expect(lookup(key(BREW, "Formula", formula), "brew")).toBe(formulaLine));
    expect(lookup(key(BREW, "Cask", cask), "brew")).toBe(caskLine);
  });

  it("gives a window in English the English file's lines, and one in Chinese none of them", async () => {
    // An npm package's line, a Python package's and a crate's, the first of
    // each in the file.
    render(<Probe />);
    const [packageName, packageLine] = first(english, "npm:");
    const [pythonName, pythonLine] = first(english, "pypi:");
    const [crate, crateLine] = first(english, "cargo:");
    const npmPackage = key(NPM, "Package", packageName);
    await waitFor(() => expect(lookup(npmPackage, "npm")).toBe(packageLine));
    expect(lookup(key("pipx", "Tool", pythonName), "pipx")).toBe(pythonLine);
    expect(lookup(key(CARGO, "Binary", crate), "cargo")).toBe(crateLine);

    // In Chinese: the Chinese file's line for each, or none.
    await switchTo("zh-CN");
    await waitFor(() => expect(lookup(npmPackage, "npm")).toBe(chinese[`npm:${packageName}`] ?? null));
    expect(lookup(key("pipx", "Tool", pythonName), "pipx")).toBe(chinese[`pypi:${pythonName}`] ?? null);
    expect(lookup(key(CARGO, "Binary", crate), "cargo")).toBe(chinese[`cargo:${crate}`] ?? null);
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
  // English lines of this test's own: npm, PyPI and crates.io packages'.
  const ENGLISH: Record<string, string> = {
    "npm:@openai/codex": "Local coding agent CLI from OpenAI",
    "npm:prettier": "Opinionated code formatter",
    "pypi:charset-normalizer": "Character encoding detector",
    "cargo:tokei": "Code line counter",
  };
  const english = () => lazyDescriptionTable(async () => ENGLISH);

  it("finds a tool's line under its logo's key: a formula without its @version, a tapped one by its tap", async () => {
    renderWithProviders(<Probe />, { toolDescriptions: { "zh-CN": table() } });
    await switchTo("zh-CN");

    await waitFor(() => expect(lookup(key(BREW, "Formula", "git"), "brew")).toBe("分布式版本控制系统"));
    expect(lookup(key(BREW, "Formula", "python@3.13"), "brew")).toBe("解释型编程语言");
    expect(lookup(key(BREW, "Formula", "someone/tap/git"), "brew")).toBe("某人的 git");
    expect(lookup(key(BREW, "Formula", "someone/tap/wget"), "brew")).toBeNull();
    // A cask's token whole, `@` and all.
    expect(lookup(key(BREW, "Cask", "firefox@developer-edition"), "brew")).toBe("开发者版火狐");
    expect(lookup(key(BREW, "Cask", "firefox"), "brew")).toBeNull();
    expect(lookup(key(NPM, "Package", "@openai/codex"), "npm")).toBe("OpenAI 的编程助手");
    expect(lookup(key(CARGO, "Binary", "tokei"), "cargo")).toBe("代码行数统计工具");
  });

  it("spaces a Chinese line's Latin words narrowly where the web view cannot, and only in Chinese", async () => {
    const supports = vi.spyOn(CSS, "supports").mockReturnValue(false);
    try {
      const own = () => lazyDescriptionTable(async () => ({ "npm:@openai/codex": "OpenAI的编程助手" }));
      renderWithProviders(<Probe />, { toolDescriptions: { "zh-CN": own(), en: english() } });
      await switchTo("zh-CN");
      await waitFor(() => expect(lookup(key(NPM, "Package", "@openai/codex"), "npm")).toBe("OpenAI\u2006的编程助手"));

      await switchTo("en");
      await waitFor(() =>
        expect(lookup(key(NPM, "Package", "@openai/codex"), "npm")).toBe("Local coding agent CLI from OpenAI"),
      );
    } finally {
      supports.mockRestore();
    }
  });

  it("finds a Python package under its PEP 503 name, from pip, pipx and uv alike", async () => {
    renderWithProviders(<Probe />, { toolDescriptions: { "zh-CN": table() } });
    await switchTo("zh-CN");

    await waitFor(() => expect(lookup(key("pipx", "Tool", "charset-normalizer"), "pipx")).toBe("字符编码探测库"));
    expect(lookup(key("pip:/usr/bin/python3", "Package", "Charset_Normalizer"), "pip")).toBe("字符编码探测库");
    expect(lookup(key("uv", "Tool", "charset.normalizer"), "uv")).toBe("字符编码探测库");
  });

  it("has no line for a tool the table does not list, one with no key, or a name on Object's prototype", async () => {
    renderWithProviders(<Probe />, { toolDescriptions: { "zh-CN": table() } });
    await switchTo("zh-CN");

    await waitFor(() => expect(lookup(key(BREW, "Formula", "git"), "brew")).not.toBeNull());
    expect(lookup(key(BREW, "Formula", "wget"), "brew")).toBeNull();
    expect(lookup(key("ollama:http://127.0.0.1:11434", "Model", "llama3.2:3b"), "ollama")).toBeNull();
    expect(lookup(key("gem", "Package", "rails"), "gem")).toBeNull();
    expect(lookup(key(BREW, "Formula", "toString"), "brew")).toBeNull();
    expect(lookup(key(NPM, "Package", "constructor"), "npm")).toBeNull();
  });

  it("finds a package's English line under its logo's key, and none for a tool the table does not list", async () => {
    // In English, which the tests start in: no switch.
    renderWithProviders(<Probe />, { toolDescriptions: { en: english() } });

    const codex = key(NPM, "Package", "@openai/codex");
    await waitFor(() => expect(lookup(codex, "npm")).toBe("Local coding agent CLI from OpenAI"));
    expect(lookup(key("pip:/usr/bin/python3", "Package", "Charset_Normalizer"), "pip")).toBe(
      "Character encoding detector",
    );
    expect(lookup(key("pipx", "Tool", "charset-normalizer"), "pipx")).toBe("Character encoding detector");
    expect(lookup(key("uv", "Tool", "charset.normalizer"), "uv")).toBe("Character encoding detector");
    expect(lookup(key(CARGO, "Binary", "tokei"), "cargo")).toBe("Code line counter");
    // None: a package it does not list, a formula, a model, and a name on
    // Object's prototype. The row says what it did without it.
    expect(lookup(key(NPM, "Package", "corepack"), "npm")).toBeNull();
    expect(lookup(key(BREW, "Formula", "git"), "brew")).toBeNull();
    expect(lookup(key("ollama:http://127.0.0.1:11434", "Model", "llama3.2:3b"), "ollama")).toBeNull();
    expect(lookup(key(NPM, "Package", "constructor"), "npm")).toBeNull();
  });

  it("reads the Chinese table only once the window is in Chinese, once, and gives none of its lines in English", async () => {
    const read = vi.fn(async () => LINES);
    renderWithProviders(<Probe />, { toolDescriptions: { "zh-CN": lazyDescriptionTable(read) } });
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

  it("reads the English table only once the window is in English, once, and gives none of its lines in Chinese", async () => {
    const readEnglish = vi.fn(async () => ENGLISH);
    const readChinese = vi.fn(async () => LINES);
    const tokei = key(CARGO, "Binary", "tokei");
    const prettier = key(NPM, "Package", "prettier");

    // A window that opens in Chinese: its table, and nothing read in English.
    await switchTo("zh-CN");
    renderWithProviders(<Probe />, {
      toolDescriptions: { en: lazyDescriptionTable(readEnglish), "zh-CN": lazyDescriptionTable(readChinese) },
    });
    await waitFor(() => expect(lookup(tokei, "cargo")).toBe("代码行数统计工具"));
    expect(lookup(prettier, "npm")).toBeNull();
    expect(readEnglish).not.toHaveBeenCalled();

    await switchTo("en");
    await waitFor(() => expect(lookup(tokei, "cargo")).toBe("Code line counter"));
    expect(lookup(prettier, "npm")).toBe("Opinionated code formatter");
    expect(readEnglish).toHaveBeenCalledTimes(1);

    // Back in Chinese: its lines, at once, and none of the English ones.
    await switchTo("zh-CN");
    expect(lookup(tokei, "cargo")).toBe("代码行数统计工具");
    expect(lookup(prettier, "npm")).toBeNull();

    // In English again: the lines already read, at once, and not read again.
    await switchTo("en");
    expect(lookup(tokei, "cargo")).toBe("Code line counter");
    expect(readEnglish).toHaveBeenCalledTimes(1);
    expect(readChinese).toHaveBeenCalledTimes(1);
  });

  it.each(["zh-CN", "en"] as const)(
    "gives no line while the %s table is on its way, nor after it failed to arrive",
    async (language) => {
      let fail: (reason: unknown) => void = () => {};
      const read = vi.fn(
        () =>
          new Promise<Record<string, string>>((_resolve, reject) => {
            fail = reject;
          }),
      );
      const logged = vi.spyOn(console, "error").mockImplementation(() => {});
      const codex = key(NPM, "Package", "@openai/codex");
      try {
        await switchTo(language);
        const failing = lazyDescriptionTable(read);
        renderWithProviders(<Probe />, {
          toolDescriptions: language === "en" ? { en: failing } : { "zh-CN": failing },
        });
        await act(async () => {});
        expect(read).toHaveBeenCalledTimes(1);
        expect(lookup(codex, "npm")).toBeNull();

        await act(async () => {
          fail(new Error("no chunk"));
        });
        expect(lookup(codex, "npm")).toBeNull();
        expect(logged).toHaveBeenCalled();
      } finally {
        logged.mockRestore();
      }
    },
  );
});
