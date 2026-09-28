/**
 * The logos Canager ships, and which one a tool or a source shows. The
 * pack -- src/assets/tool-icons/pack.json and its raster/ folder -- is
 * built at development time by `pnpm icons:build`
 * (scripts/tool-icons/build.mjs) from the reviewed
 * scripts/tool-icons/mapping.json, and committed: nothing is fetched to
 * show a logo, so no server learns what is installed. A logo is one of:
 *
 * - a glyph: a Simple Icons logo, one path on a 24×24 grid, drawn in
 *   `glyphInk(hex)` on a rounded square of its brand colour;
 * - a raster: a project's GitHub avatar, a 96 px WebP, drawn on white,
 *   since some are black on transparent.
 *
 * Pure: no React and no backend. `loadToolIcons` reads any pack; the
 * functions exported at the bottom read the built-in one.
 */
import packJson from "../assets/tool-icons/pack.json";
import type { ArtifactKey } from "./types";

/** pack.json, version 1. Ids are `si-<slug>` for a glyph, `gh-<login>` for a raster. */
export interface ToolIconPack {
  version: number;
  /** The day `icons:build` wrote it. */
  generated: string;
  /** Path data on a 24×24 grid; the brand colour as `RRGGBB`. */
  glyphs: Record<string, { path: string; hex: string; title: string }>;
  /** `file` is the WebP's name in raster/. */
  rasters: Record<string, { file: string; title: string }>;
  /** A tool's key (`toolIconKey`) → the id of its logo. */
  tools: Record<string, string>;
  /** An adapter id → the id of its source's logo. */
  sources: Record<string, string>;
}

/**
 * A logo ready to draw. `title` is the brand's or the account's name, for
 * whoever reads the pack: a logo is decorative, the row names the tool.
 */
export type ToolIcon =
  | { kind: "glyph"; path: string; hex: string; title: string }
  | { kind: "raster"; url: string; title: string };

export interface ToolIcons {
  /**
   * The key a tool's logo is listed under, from the tool's own key and
   * its source's adapter id; `null` for a tool that can have none.
   *
   * - Homebrew: a cask's token, `cask:<token>`; a formula's name without
   *   the `@<version>` a versioned formula carries, `brew:<name>`
   *   (`python@3.13` is Python). A tapped one keeps its tap: it is not
   *   the formula of the same name in Homebrew's own.
   * - npm: `npm:<name>`, a scope and all.
   * - pip, pipx and uv: `pypi:<name>`, PEP 503-normalized -- lower case,
   *   each run of `-`, `_` and `.` one `-` -- as PyPI itself does.
   * - Cargo: `cargo:<crate>`.
   * - Ollama: `ollama:<family>`, for the longest family the pack lists that
   *   the model's name starts with (`qwen2.5-coder` → `ollama:qwen`), the
   *   name read after its last `/` (a registry and a namespace go), in
   *   lower case, without its `:tag`; `null` when no family matches.
   * - A tool with its own installer: `standalone:<tool>`, from its
   *   adapter id `standalone-<tool>`.
   */
  toolIconKey(key: ArtifactKey, adapterId: string): string | null;
  /** The pack's logo for this tool, or `null` when it has none. */
  resolveToolIcon(key: ArtifactKey, adapterId: string): ToolIcon | null;
  /** The pack's logo for this source (Homebrew, npm, …), or `null` when it has none. */
  resolveSourceIcon(adapterId: string): ToolIcon | null;
}

const OLLAMA = "ollama:";
const STANDALONE = "standalone-";

/**
 * `pack`, ready to look logos up in. `rasterUrls` maps a raster's file
 * name to the URL it is served at; a raster without one resolves to
 * `null`, like a tool the pack has no logo for. Maps rather than the
 * pack's plain objects, so a name like "toString" finds nothing.
 */
export function loadToolIcons(pack: ToolIconPack, rasterUrls: ReadonlyMap<string, string>): ToolIcons {
  const icons = new Map<string, ToolIcon>();
  for (const [id, { path, hex, title }] of Object.entries(pack.glyphs)) {
    icons.set(id, { kind: "glyph", path, hex, title });
  }
  for (const [id, { file, title }] of Object.entries(pack.rasters)) {
    const url = rasterUrls.get(file);
    if (url !== undefined) icons.set(id, { kind: "raster", url, title });
  }
  const tools = new Map(Object.entries(pack.tools));
  const sources = new Map(Object.entries(pack.sources));
  // Longest first: the first family a model's name starts with is then the longest.
  const ollamaFamilies = [...tools.keys()]
    .filter((key) => key.startsWith(OLLAMA) && key.length > OLLAMA.length)
    .map((key) => key.slice(OLLAMA.length))
    .sort((a, b) => b.length - a.length);

  const toolIconKey = (key: ArtifactKey, adapterId: string): string | null => {
    switch (adapterId) {
      case "brew":
        // Homebrew lists formulae and casks, nothing else.
        return key.kind === "Cask" ? `cask:${key.name}` : `brew:${key.name.replace(/@[^@/]+$/, "")}`;
      case "npm":
        return `npm:${key.name}`;
      case "pip":
      case "pipx":
      case "uv":
        return `pypi:${key.name.toLowerCase().replace(/[-_.]+/g, "-")}`;
      case "cargo":
        return `cargo:${key.name}`;
      case "ollama": {
        const afterSlash = key.name.slice(key.name.lastIndexOf("/") + 1).toLowerCase();
        const model = afterSlash.split(":")[0];
        const family = ollamaFamilies.find((f) => model.startsWith(f));
        return family === undefined ? null : `${OLLAMA}${family}`;
      }
      default:
        return adapterId.startsWith(STANDALONE) && adapterId.length > STANDALONE.length
          ? `standalone:${adapterId.slice(STANDALONE.length)}`
          : null;
    }
  };
  const iconOf = (id: string | undefined): ToolIcon | null =>
    id === undefined ? null : (icons.get(id) ?? null);

  return {
    toolIconKey,
    resolveToolIcon: (key, adapterId) => {
      const toolKey = toolIconKey(key, adapterId);
      return toolKey === null ? null : iconOf(tools.get(toolKey));
    },
    resolveSourceIcon: (adapterId) => iconOf(sources.get(adapterId)),
  };
}

/**
 * A glyph's colour on its square: white, or the near-black the Homebrew
 * avatar's initial is drawn in (`--color-source-homebrew-ink`).
 */
export const GLYPH_INK_LIGHT = "#ffffff";
export const GLYPH_INK_DARK = "#1b1d2a";

/**
 * The colour a glyph is drawn in on its brand colour `hex` (`RRGGBB`, as
 * the pack holds it): white or near-black, whichever has the higher WCAG 2
 * contrast with it. Not a brightness cut-off: that picks white on a
 * saturated red, where the near-black has more (`FF0000`: 4.2:1, against
 * white's 4.0:1). The same in dark mode: the square is the brand's colour
 * in both.
 */
export function glyphInk(hex: string): typeof GLYPH_INK_LIGHT | typeof GLYPH_INK_DARK {
  const background = relativeLuminance(hex);
  return contrast(background, relativeLuminance(GLYPH_INK_LIGHT)) >=
    contrast(background, relativeLuminance(GLYPH_INK_DARK))
    ? GLYPH_INK_LIGHT
    : GLYPH_INK_DARK;
}

/** WCAG 2's relative luminance of an `RRGGBB` colour, a leading `#` allowed. */
function relativeLuminance(hex: string): number {
  const rgb = parseInt(hex.replace(/^#/, ""), 16);
  const channel = (shift: number) => {
    const c = ((rgb >> shift) & 0xff) / 255;
    return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0);
}

/** WCAG 2's contrast ratio of two relative luminances. */
function contrast(a: number, b: number): number {
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
}

/**
 * The built-in pack's rasters, by file name: the URL Vite serves each at.
 * `no-inline`: most avatars are under Vite's 4 KB inlining limit, and
 * inlined, hundreds of them would be base64 in the app's script, read at
 * every launch; as files, one is loaded when a row shows it.
 */
const RASTER_URLS = new Map(
  Object.entries(
    import.meta.glob<string>("../assets/tool-icons/raster/*.webp", {
      eager: true,
      query: "?url&no-inline",
      import: "default",
    }),
  ).map(([path, url]) => [path.slice(path.lastIndexOf("/") + 1), url]),
);

const BUILT_IN = loadToolIcons(packJson, RASTER_URLS);

export const toolIconKey = BUILT_IN.toolIconKey;
export const resolveToolIcon = BUILT_IN.resolveToolIcon;
export const resolveSourceIcon = BUILT_IN.resolveSourceIcon;
