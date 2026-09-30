import { describe, expect, it } from "vitest";
import { existsSync, readdirSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import * as simpleIcons from "simple-icons";
import pack from "../assets/tool-icons/pack.json";
import { ADAPTER_LABEL_KEYS } from "./sources";
import {
  GLYPH_INK_DARK,
  GLYPH_INK_LIGHT,
  glyphInk,
  loadToolIcons,
  resolveSourceIcon,
  resolveToolIcon,
  toolIconKey,
  type ToolIconPack,
} from "./toolIcons";
import type { ArtifactKey, ArtifactKind } from "./types";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

function key(instanceId: string, kind: ArtifactKind, name: string): ArtifactKey {
  return { instance_id: instanceId, kind, name };
}

/**
 * A pack of this test's own, so that no test here depends on what the
 * committed pack happens to list: it is built from the reviewed mapping,
 * which the next review may change.
 */
const FIXTURE: ToolIconPack = {
  version: 1,
  generated: "2026-09-28",
  glyphs: { "si-git": { path: "M0 0h24v24H0z", hex: "F03C2E", title: "Git" } },
  rasters: {
    "gh-openai": { file: "gh-openai.webp", title: "openai" },
    // Named, but with no file to serve: the URL map below leaves it out.
    "gh-gone": { file: "gh-gone.webp", title: "gone" },
  },
  tools: {
    "brew:git": "si-git",
    "brew:wget": "gh-gone",
    "npm:@openai/codex": "gh-openai",
    // Ollama families. `phi` (Microsoft's) is a prefix of `phind` (Phind's).
    "ollama:llama": "si-git",
    "ollama:qwen": "si-git",
    "ollama:phi": "si-git",
    "ollama:phind": "gh-openai",
  },
  sources: { brew: "si-git", npm: "gh-openai", cargo: "gh-gone" },
};
const fixture = loadToolIcons(FIXTURE, new Map([["gh-openai.webp", "/assets/gh-openai.webp"]]));

describe("toolIconKey", () => {
  const brew = "brew:/opt/homebrew";

  it("keys a Homebrew formula by its name without a trailing @version", () => {
    expect(fixture.toolIconKey(key(brew, "Formula", "git"), "brew")).toBe("brew:git");
    expect(fixture.toolIconKey(key(brew, "Formula", "python@3.13"), "brew")).toBe("brew:python");
    expect(fixture.toolIconKey(key(brew, "Formula", "openssl@3"), "brew")).toBe("brew:openssl");
  });

  it("keeps a tapped formula's tap: it is not Homebrew's formula of that name", () => {
    expect(fixture.toolIconKey(key(brew, "Formula", "hashicorp/tap/terraform"), "brew")).toBe(
      "brew:hashicorp/tap/terraform",
    );
    expect(fixture.toolIconKey(key(brew, "Formula", "someone/tap/node@22"), "brew")).toBe(
      "brew:someone/tap/node",
    );
  });

  it("keys a Homebrew cask by its token, exactly", () => {
    expect(fixture.toolIconKey(key(brew, "Cask", "visual-studio-code"), "brew")).toBe(
      "cask:visual-studio-code",
    );
    expect(fixture.toolIconKey(key(brew, "Cask", "firefox@nightly"), "brew")).toBe("cask:firefox@nightly");
  });

  it("keys an npm package by its name, a scope and all", () => {
    expect(fixture.toolIconKey(key("npm:/opt/homebrew", "Package", "@openai/codex"), "npm")).toBe(
      "npm:@openai/codex",
    );
    expect(fixture.toolIconKey(key("npm:/opt/homebrew", "Package", "typescript"), "npm")).toBe(
      "npm:typescript",
    );
  });

  it("keys a Python package by its PEP 503 name, from pip, pipx and uv alike", () => {
    expect(fixture.toolIconKey(key("pipx", "Tool", "Foo.Bar_baz--Qux"), "pipx")).toBe("pypi:foo-bar-baz-qux");
    expect(fixture.toolIconKey(key("uv", "Tool", "pre-commit"), "uv")).toBe("pypi:pre-commit");
    const pip = "pip:/opt/homebrew/bin/python3";
    expect(fixture.toolIconKey(key(pip, "Package", "charset_normalizer"), "pip")).toBe("pypi:charset-normalizer");
  });

  it("keys a Cargo program by its crate", () => {
    expect(fixture.toolIconKey(key("cargo:/Users/you/.cargo", "Binary", "jj-cli"), "cargo")).toBe("cargo:jj-cli");
  });

  describe("an Ollama model", () => {
    const model = (name: string) =>
      fixture.toolIconKey(key("ollama:http://127.0.0.1:11434", "Model", name), "ollama");

    it("takes the longest family in the pack it is of", () => {
      expect(model("phind-codellama:34b")).toBe("ollama:phind");
      expect(model("phi4:14b")).toBe("ollama:phi");
      expect(model("qwen2.5-coder:7b")).toBe("ollama:qwen");
      expect(model("llama3.2:3b")).toBe("ollama:llama");
      expect(model("qwen")).toBe("ollama:qwen");
    });

    it("reads a registry-namespaced name after its last /, in lower case, without its tag", () => {
      expect(model("modelscope.cn/Qwen/Qwen2.5-Coder:7b")).toBe("ollama:qwen");
      expect(model("hf.co/bartowski/Llama-3.2-1B-Instruct-GGUF:Q4_K_M")).toBe("ollama:llama");
    });

    it("has no key when no family matches", () => {
      expect(model("mistral:7b")).toBeNull();
      // A family matches the start of the name, not any part of it.
      expect(model("tinyllama:1.1b")).toBeNull();
    });

    /** A model's key, by its name, in a pack with these Ollama families and no other tool. */
    const inPackOf = (...families: string[]) => {
      const icons = loadToolIcons(
        { ...FIXTURE, tools: Object.fromEntries(families.map((family) => [`ollama:${family}`, "si-git"])) },
        new Map(),
      );
      return (name: string) => icons.toolIconKey(key("ollama:http://127.0.0.1:11434", "Model", name), "ollama");
    };

    it("is of a family only where its name goes on from the family's with nothing, a digit, -, ., _ or :", () => {
      // Phi and no Phind, Llama and no LLaVA, Mistral and no MistralLite.
      const of = inPackOf("phi", "llama", "mistral", "qwen", "deepseek");
      expect(of("phi")).toBe("ollama:phi");
      expect(of("phi:latest")).toBe("ollama:phi");
      expect(of("phi3:mini")).toBe("ollama:phi");
      expect(of("qwen2.5-coder:7b")).toBe("ollama:qwen");
      expect(of("deepseek-r1:8b")).toBe("ollama:deepseek");
      expect(of("llama.custom")).toBe("ollama:llama");
      expect(of("mistral_custom:latest")).toBe("ollama:mistral");
      // A letter after the family's name makes it another model's name.
      expect(of("phind-codellama:34b")).toBeNull();
      expect(of("mistrallite:7b")).toBeNull();
      expect(of("llava:7b")).toBeNull();
    });

    it("passes over a longer family its name goes on from with a letter", () => {
      const of = inPackOf("command", "command-r");
      expect(of("command-r-plus:104b")).toBe("ollama:command-r");
      expect(of("command-r7b")).toBe("ollama:command-r");
      expect(of("command-a:111b")).toBe("ollama:command");
      // It starts with `command-r`, and goes on from it with an "e".
      expect(of("command-reasoning")).toBe("ollama:command");
    });
  });

  it("keys a tool with its own installer by its adapter id", () => {
    for (const tool of ["claude", "rustup", "agy", "grok"]) {
      expect(fixture.toolIconKey(key(`standalone-${tool}`, "Binary", tool), `standalone-${tool}`)).toBe(
        `standalone:${tool}`,
      );
    }
  });

  it("has no key for a source it does not know", () => {
    expect(fixture.toolIconKey(key("gem", "Package", "rails"), "gem")).toBeNull();
    expect(fixture.toolIconKey(key("standalone-", "Binary", ""), "standalone-")).toBeNull();
  });
});

describe("resolveToolIcon and resolveSourceIcon", () => {
  const brew = "brew:/opt/homebrew";

  it("gives a glyph's path, colour and title", () => {
    expect(fixture.resolveToolIcon(key(brew, "Formula", "git"), "brew")).toEqual({
      kind: "glyph",
      path: "M0 0h24v24H0z",
      hex: "F03C2E",
      title: "Git",
    });
    expect(fixture.resolveSourceIcon("brew")).toEqual(fixture.resolveToolIcon(key(brew, "Formula", "git"), "brew"));
  });

  it("gives a raster's URL and title", () => {
    const raster = { kind: "raster", url: "/assets/gh-openai.webp", title: "openai" };
    expect(fixture.resolveToolIcon(key("npm:/opt/homebrew", "Package", "@openai/codex"), "npm")).toEqual(raster);
    expect(fixture.resolveSourceIcon("npm")).toEqual(raster);
  });

  it("gives an Ollama model its longest family's logo", () => {
    const ollama = "ollama:http://127.0.0.1:11434";
    expect(fixture.resolveToolIcon(key(ollama, "Model", "phind-codellama:34b"), "ollama")).toMatchObject({
      kind: "raster",
      title: "openai",
    });
    expect(fixture.resolveToolIcon(key(ollama, "Model", "phi4:14b"), "ollama")).toMatchObject({ kind: "glyph" });
  });

  it("gives nothing where the pack has nothing to draw", () => {
    // No entry for the tool, or for the source.
    expect(fixture.resolveToolIcon(key(brew, "Formula", "mtr"), "brew")).toBeNull();
    expect(fixture.resolveToolIcon(key("ollama:http://127.0.0.1:11434", "Model", "mistral"), "ollama")).toBeNull();
    expect(fixture.resolveSourceIcon("uv")).toBeNull();
    // An entry whose raster has no file to serve.
    expect(fixture.resolveToolIcon(key(brew, "Formula", "wget"), "brew")).toBeNull();
    expect(fixture.resolveSourceIcon("cargo")).toBeNull();
    // A name every object has a property for is not an entry.
    expect(fixture.resolveSourceIcon("toString")).toBeNull();
  });
});

describe("credits", () => {
  const licensed = (title: string, type: string) => ({
    path: "M0 0h24v24H0z",
    hex: "000000",
    title,
    license: { type, url: `https://spdx.org/licenses/${type}` },
    source: `https://example.org/${title.toLowerCase()}`,
  });

  it("lists every glyph with a license of its own, by title, and no other logo", () => {
    const icons = loadToolIcons(
      {
        ...FIXTURE,
        glyphs: { ...FIXTURE.glyphs, "si-rust": licensed("Rust", "CC-BY-SA-4.0"), "si-ajv": licensed("Ajv", "MIT") },
      },
      new Map([["gh-openai.webp", "/assets/gh-openai.webp"]]),
    );
    expect(icons.credits).toEqual([
      {
        id: "si-ajv",
        title: "Ajv",
        license: { type: "MIT", url: "https://spdx.org/licenses/MIT" },
        source: "https://example.org/ajv",
      },
      {
        id: "si-rust",
        title: "Rust",
        license: { type: "CC-BY-SA-4.0", url: "https://spdx.org/licenses/CC-BY-SA-4.0" },
        source: "https://example.org/rust",
      },
    ]);
  });

  it("is empty for a pack with no logo under a license of its own", () => {
    expect(fixture.credits).toEqual([]);
  });
});

describe("glyphInk", () => {
  it("draws a glyph white on a dark colour and near-black on a light one", () => {
    expect(glyphInk("000000")).toBe(GLYPH_INK_LIGHT);
    expect(glyphInk("CB3837")).toBe(GLYPH_INK_LIGHT); // npm's red
    expect(glyphInk("3776AB")).toBe(GLYPH_INK_LIGHT); // Python's blue
    expect(glyphInk("FFFFFF")).toBe(GLYPH_INK_DARK);
    expect(glyphInk("FBB040")).toBe(GLYPH_INK_DARK); // Homebrew's amber
  });

  it("picks by WCAG contrast, not by brightness", () => {
    // Pure red's brightness (luma) is 76 of 255, and a cut-off at half
    // would put white on it; the near-black has the higher contrast with
    // it, 4.2:1 against white's 4.0:1.
    expect(glyphInk("FF0000")).toBe(GLYPH_INK_DARK);
  });
});

/**
 * The pack the app ships, whatever it lists: these hold for the reviewed
 * mapping's hundreds of logos, and must for any mapping that replaces it.
 */
describe("the built-in pack", () => {
  const PACK_DIR = path.resolve(__dirname, "../assets/tool-icons");
  const RASTER_DIR = path.join(PACK_DIR, "raster");
  // 1000-based, as Finder counts and `formatBytes` shows.
  const BUDGET_BYTES = 5_000_000;
  const built: ToolIconPack = pack;
  // What the folder holds, but the .DS_Store Finder leaves in a folder it
  // has shown, which git ignores and the app never ships.
  const listing = (dir: string) => (existsSync(dir) ? readdirSync(dir) : []).filter((f) => f !== ".DS_Store");

  it("names only logos it has", () => {
    const logos = new Set([...Object.keys(built.glyphs), ...Object.keys(built.rasters)]);
    const dangling = [
      ...Object.entries(built.tools).map(([toolKey, id]) => [`tools["${toolKey}"]`, id]),
      ...Object.entries(built.sources).map(([adapterId, id]) => [`sources["${adapterId}"]`, id]),
    ]
      .filter(([, id]) => !logos.has(id))
      .map(([entry, id]) => `${entry} → ${id}`);
    expect(dangling, `names a logo the pack does not have: ${dangling.join(", ")}`).toEqual([]);
  });

  it("holds well-formed glyphs", () => {
    for (const [id, glyph] of Object.entries(built.glyphs)) {
      expect(id, id).toMatch(/^si-[a-z0-9_]+$/);
      expect(glyph.hex, id).toMatch(/^[0-9A-F]{6}$/);
      expect(glyph.path, id).toMatch(/^[Mm]/);
      expect(glyph.title, id).not.toBe("");
    }
  });

  describe("a logo's own license", () => {
    // The licenses a logo may carry of its own, besides none at all (Simple
    // Icons' CC0), as the maintainer set them: attribution and share-alike
    // in any version, CC0, MIT, Apache 2.0, BSD and ISC. Nothing NC or ND,
    // no GPL, LGPL or AGPL, no "custom" license, nothing else.
    // scripts/tool-icons/build.mjs refuses the rest too.
    const SHIPPABLE_LICENSE =
      /^(?:CC0-1\.0|MIT|Apache-2\.0|BSD-2-Clause|BSD-3-Clause|ISC|CC-BY-\d+\.\d+|CC-BY-SA-\d+\.\d+)$/;
    // Every glyph is a Simple Icons logo: the pinned devDependency's, the
    // one `icons:build` reads.
    const simpleIconsById = new Map(Object.values(simpleIcons).map((icon) => [`si-${icon.slug}`, icon]));
    const simpleIconOf = (id: string) => {
      const icon = simpleIconsById.get(id);
      if (icon === undefined) throw new Error(`simple-icons has no logo for ${id}`);
      return icon;
    };

    it("is the one Simple Icons gives the logo, and one Banager may ship", () => {
      for (const [id, glyph] of Object.entries(built.glyphs)) {
        // None where Simple Icons gives none: the logo is then under its CC0.
        expect(glyph.license, id).toEqual(simpleIconOf(id).license);
        if (glyph.license !== undefined) expect(glyph.license.type, id).toMatch(SHIPPABLE_LICENSE);
      }
    });

    it("keeps the logo exactly as Simple Icons draws it, with its source", () => {
      for (const [id, glyph] of Object.entries(built.glyphs)) {
        if (glyph.license === undefined) {
          expect(glyph.source, id).toBeUndefined();
          continue;
        }
        const icon = simpleIconOf(id);
        expect(glyph.path, id).toBe(icon.path);
        expect(glyph.source, id).toBe(icon.source);
      }
    });
  });

  it("has every raster's file, and no file it does not name", () => {
    const named = new Set<string>();
    for (const [id, raster] of Object.entries(built.rasters)) {
      expect(id, id).toMatch(/^gh-[a-z0-9-]+$/);
      expect(raster.file, id).toBe(`${id}.webp`);
      expect(existsSync(path.join(RASTER_DIR, raster.file)), `${raster.file} is missing`).toBe(true);
      named.add(raster.file);
    }
    const orphans = listing(RASTER_DIR).filter((file) => !named.has(file));
    expect(orphans, `raster/ has files pack.json does not name: ${orphans.join(", ")}`).toEqual([]);
  });

  it("resolves every source's logo, for a source Banager knows", () => {
    for (const adapterId of Object.keys(built.sources)) {
      expect(Object.keys(ADAPTER_LABEL_KEYS), adapterId).toContain(adapterId);
      expect(resolveSourceIcon(adapterId), adapterId).not.toBeNull();
    }
  });

  it("resolves every tool's logo, under a key toolIconKey can give", () => {
    // Each key back to a tool it is the key of: a key toolIconKey never
    // gives -- `pypi:Foo_Bar`, `brew:python@3.13`, an unknown prefix --
    // would be a logo no row ever shows.
    const toolFor = (toolKey: string): [ArtifactKey, string] | null => {
      const colon = toolKey.indexOf(":");
      const name = toolKey.slice(colon + 1);
      switch (toolKey.slice(0, colon)) {
        case "brew":
          return [key("brew:/opt/homebrew", "Formula", name), "brew"];
        case "cask":
          return [key("brew:/opt/homebrew", "Cask", name), "brew"];
        case "npm":
          return [key("npm:/opt/homebrew", "Package", name), "npm"];
        case "pypi":
          return [key("pipx", "Tool", name), "pipx"];
        case "cargo":
          return [key("cargo:/Users/you/.cargo", "Binary", name), "cargo"];
        case "ollama":
          return [key("ollama:http://127.0.0.1:11434", "Model", `${name}:latest`), "ollama"];
        case "standalone":
          return [key(`standalone-${name}`, "Binary", name), `standalone-${name}`];
        default:
          return null;
      }
    };
    const unreachable = Object.keys(built.tools).filter((toolKey) => {
      const tool = toolFor(toolKey);
      return tool === null || toolIconKey(...tool) !== toolKey || resolveToolIcon(...tool) === null;
    });
    expect(unreachable, `no tool resolves to: ${unreachable.join(", ")}`).toEqual([]);
  });

  it("gives the npm packages of projects Simple Icons draws their project's own logo, not npm's (round 3)", () => {
    // Each checked against its npm registry entry: the repository is the
    // project's own (typescript is microsoft/TypeScript, bun oven-sh/bun …).
    // Framework CLIs are "maker", as @angular/cli's Angular is, and so is
    // meteor, the Meteor team's installer (it has no repository; its
    // maintainers are the team's accounts), not the framework itself.
    const added: Record<string, string> = {
      "npm:typescript": "si-typescript",
      "npm:vite": "si-vite",
      "npm:vitest": "si-vitest",
      "npm:jest": "si-jest",
      "npm:mocha": "si-mocha",
      "npm:electron": "si-electron",
      "npm:electron-builder": "si-electronbuilder",
      "npm:esbuild": "si-esbuild",
      "npm:rollup": "si-rollupdotjs",
      "npm:@babel/cli": "si-babel",
      "npm:tailwindcss": "si-tailwindcss",
      "npm:@tailwindcss/cli": "si-tailwindcss",
      "npm:stylelint": "si-stylelint",
      "npm:bun": "si-bun",
      "npm:deno": "si-deno",
      "npm:puppeteer": "si-puppeteer",
      "npm:appium": "si-appium",
      "npm:node-red": "si-nodered",
      "npm:homebridge": "si-homebridge",
      "npm:typeorm": "si-typeorm",
      "npm:knex": "si-knexdotjs",
      "npm:@11ty/eleventy": "si-eleventy",
      "npm:astro": "si-astro",
      "npm:storybook": "si-storybook",
      "npm:renovate": "si-renovate",
      "npm:gulp": "si-gulp",
      "npm:meteor": "si-meteor",
      "npm:@capacitor/cli": "si-capacitor",
      "npm:nuxi": "si-nuxt",
      "npm:sv": "si-svelte",
      "npm:@apollo/rover": "si-apollographql",
    };
    for (const [toolKey, id] of Object.entries(added)) expect(built.tools[toolKey], toolKey).toBe(id);
    // The TypeScript row shows TypeScript's logo, as Homebrew's typescript
    // does -- not npm's, its source's, which it showed before.
    const typescript = key("npm:/opt/homebrew", "Package", "typescript");
    expect(resolveToolIcon(typescript, "npm")).toEqual(resolveToolIcon(key("brew:/opt/homebrew", "Formula", "typescript"), "brew"));
    expect(resolveToolIcon(typescript, "npm")).not.toEqual(resolveSourceIcon("npm"));
    // An npm twin of a Homebrew formula has the formula's logo.
    for (const name of ["typescript", "vite", "esbuild", "tailwindcss", "stylelint", "bun", "deno", "appium", "renovate"]) {
      expect(built.tools[`npm:${name}`], name).toBe(built.tools[`brew:${name}`]);
    }
    // And a reviewed "no logo" stays one: webpack's, Sass's and Vue's
    // logos carry licenses Banager does not ship.
    for (const toolKey of ["npm:webpack-cli", "npm:sass", "npm:@vue/cli"]) {
      expect(built.tools[toolKey], toolKey).toBeUndefined();
    }
  });

  it("gives mtr no logo: Simple Icons' MTR is Hong Kong's railway, not the network tool", () => {
    expect(Object.keys(built.tools)).not.toContain("brew:mtr");
    expect(resolveToolIcon(key("brew:/opt/homebrew", "Formula", "mtr"), "brew")).toBeNull();
  });

  it("fits in its 5 MB budget", () => {
    const bytes = (dir: string): number =>
      listing(dir).reduce((sum, entry) => {
        const full = path.join(dir, entry);
        return sum + (statSync(full).isDirectory() ? bytes(full) : statSync(full).size);
      }, 0);
    expect(bytes(PACK_DIR)).toBeLessThanOrEqual(BUDGET_BYTES);
  });
});
