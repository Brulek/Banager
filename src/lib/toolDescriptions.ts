/**
 * Each tool's line in the window's language, which a row shows under the
 * tool's name (`describeTool`, src/lib/sources.ts). Two tables, each one
 * object from a tool's key to its line, made at development time and
 * committed: nothing is fetched to show a line, so no server learns what
 * is installed.
 *
 * - src/assets/tool-descriptions/zh-CN.json, for a window in Chinese:
 *   Homebrew's formulae and casks and npm, PyPI and crates.io packages,
 *   each line translated from the description the tool's own source gives
 *   it (Homebrew's, npm's, PyPI's, crates.io's). It takes the place of the
 *   description its source gives in English, or of what the row says when
 *   the source gives none.
 * - src/assets/tool-descriptions/en.json, for a window in English: npm,
 *   PyPI and crates.io packages only, each line rewritten, shorter, from
 *   the description the package's own registry gives it. npm's, pip's,
 *   pipx's, uv's and Cargo's inventories give none, so it takes the place
 *   of what the row says when there is none ("npm package"); Homebrew
 *   gives its own words in English, and has no line in it.
 *
 * Their keys are the ones the logos are listed under (`toolIconKey`,
 * src/lib/toolIcons.ts): `brew:`, `cask:`, `npm:`, `pypi:` and `cargo:`.
 *
 * A table is read only once the window is in its language, and then from
 * a file of its own: a dynamic `import`, which Vite builds into a chunk
 * apart from the app's script, so a window never in Chinese never loads
 * the Chinese one, nor a window never in English the English one. Until
 * it has arrived, a row says what it would without it: its source's
 * words, or what its source says it is (`toolDescription`,
 * src/lib/sources.ts).
 */
import { createContext, useCallback, useContext, useEffect, useSyncExternalStore } from "react";
import { useTranslation } from "react-i18next";
import { useToolIcons } from "./toolIconsContext";
import type { ArtifactKey } from "./types";

/** A table of lines by tool key, read the first time it is asked for. */
export interface DescriptionTable {
  /** Its lines, once read; `null` until then, and after a read that failed. */
  lines(): ReadonlyMap<string, string> | null;
  /** Reads the lines, the first time it is called; does nothing after. */
  load(): void;
  /** Calls `listener` when the lines arrive; returns what stops it. */
  subscribe(listener: () => void): () => void;
}

/**
 * A table whose lines `read` gives, called the first time `load` is. A
 * Map rather than the JSON's plain object, so a name like "toString"
 * finds nothing. A read that fails leaves the rows their source's words;
 * it is not tried again: a chunk of the app's own that will not load
 * would not load the second time either.
 */
export function lazyDescriptionTable(read: () => Promise<Readonly<Record<string, string>>>): DescriptionTable {
  let lines: ReadonlyMap<string, string> | null = null;
  let started = false;
  const listeners = new Set<() => void>();
  return {
    lines: () => lines,
    load: () => {
      if (started) return;
      started = true;
      read().then(
        (record) => {
          lines = new Map(Object.entries(record));
          for (const listener of listeners) listener();
        },
        (e: unknown) => {
          console.error("reading a table of tool descriptions failed", e);
        },
      );
    },
    subscribe: (listener) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
  };
}

/** The languages a window can be in (src/i18n/index.ts), each with a table of its own. */
export type DescriptionLanguage = "en" | "zh-CN";

/** A table for each language, read only once the window is in it. */
export type DescriptionTables = Readonly<Record<DescriptionLanguage, DescriptionTable>>;

/** The built-in tables, each from its own chunk. */
const BUILT_IN: DescriptionTables = {
  en: lazyDescriptionTable(() => import("../assets/tool-descriptions/en.json").then((module) => module.default)),
  "zh-CN": lazyDescriptionTable(() =>
    import("../assets/tool-descriptions/zh-CN.json").then((module) => module.default),
  ),
};

/** No lines, and nothing to read: the table while i18next has resolved no language. */
const NO_TABLE: DescriptionTable = {
  lines: () => null,
  load: () => {},
  subscribe: () => () => {},
};

/**
 * The tables the rows read (`useTranslatedDescription`): the built-in
 * ones, unless a provider above them hands them others. The tests do
 * (`renderWithProviders`), so that none passes or fails on whatever lines
 * the built-in tables happen to hold.
 */
export const DescriptionTablesContext = createContext<DescriptionTables>(BUILT_IN);

/** A tool's line in the window's language, by its key and its source's adapter id; `null` where there is none. */
export type TranslatedDescription = (key: ArtifactKey, adapterId: string) => string | null;

/**
 * A tool's line in the window's language -- the one i18next resolved,
 * which is when that language's table is read: in Chinese the Chinese
 * table's, in English the English one's -- and `null` before the table
 * has arrived, after it failed to, and for a tool it has no line for (in
 * English, anything but an npm, PyPI or crates.io package). Looked up
 * under the key the tool's logo is listed under (`toolIconKey`, from the
 * pack the avatars draw from): a versioned formula's name without its
 * `@<version>`, a Python package's name PEP 503-normalized.
 */
export function useTranslatedDescription(): TranslatedDescription {
  const tables = useContext(DescriptionTablesContext);
  const { toolIconKey } = useToolIcons();
  const language = useTranslation().i18n.resolvedLanguage;
  const table = language === "en" || language === "zh-CN" ? tables[language] : NO_TABLE;
  const lines = useSyncExternalStore(table.subscribe, table.lines);
  useEffect(() => {
    table.load();
  }, [table]);
  return useCallback(
    (key: ArtifactKey, adapterId: string): string | null => {
      if (lines === null) return null;
      const toolKey = toolIconKey(key, adapterId);
      return toolKey === null ? null : (lines.get(toolKey) ?? null);
    },
    [lines, toolIconKey],
  );
}
