/**
 * Each tool's line in Chinese, which a window in Chinese shows under the
 * tool's name in place of the description its source gives in English,
 * or of what the row says when the source gives none. The table --
 * src/assets/tool-descriptions/zh-CN.json, one object from a tool's key to
 * its line -- was translated at development time from the description
 * each tool's own source gives it (Homebrew's, npm's, PyPI's,
 * crates.io's), checked, and committed: nothing is fetched to show a
 * line, so no server learns what is installed. Its keys are the ones the
 * logos are listed under (`toolIconKey`, src/lib/toolIcons.ts): `brew:`,
 * `cask:`, `npm:`, `pypi:` and `cargo:`.
 *
 * The table is read only once the window is in Chinese, and then from a
 * file of its own: a dynamic `import`, which Vite builds into a chunk
 * apart from the app's script, so a window never in Chinese never loads
 * it. Until it has arrived, a row says what it would without it: its
 * source's words, or what its source says it is (`toolDescription`,
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
          console.error("reading the Chinese tool descriptions failed", e);
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

/** The built-in table, from its own chunk. */
const BUILT_IN = lazyDescriptionTable(() =>
  import("../assets/tool-descriptions/zh-CN.json").then((module) => module.default),
);

/**
 * The Chinese table the rows read (`useTranslatedDescription`): the
 * built-in one, unless a provider above them hands them another. The
 * tests do (`renderWithProviders`), so that none passes or fails on
 * whatever lines the built-in table happens to hold.
 */
export const DescriptionTableContext = createContext<DescriptionTable>(BUILT_IN);

/** A tool's line in the window's language, by its key and its source's adapter id; `null` where there is none. */
export type TranslatedDescription = (key: ArtifactKey, adapterId: string) => string | null;

/**
 * A tool's line in Chinese while the window is in Chinese -- i18next
 * resolved `zh-CN`, which is when the table is read -- and `null`
 * otherwise: in English, before the table has arrived, and for a tool it
 * has no line for. Looked up under the key the tool's logo is listed
 * under (`toolIconKey`, from the pack the avatars draw from): a versioned
 * formula's name without its `@<version>`, a Python package's name
 * PEP 503-normalized.
 */
export function useTranslatedDescription(): TranslatedDescription {
  const table = useContext(DescriptionTableContext);
  const { toolIconKey } = useToolIcons();
  const chinese = useTranslation().i18n.resolvedLanguage === "zh-CN";
  const lines = useSyncExternalStore(table.subscribe, table.lines);
  useEffect(() => {
    if (chinese) table.load();
  }, [chinese, table]);
  return useCallback(
    (key: ArtifactKey, adapterId: string): string | null => {
      if (!chinese || lines === null) return null;
      const toolKey = toolIconKey(key, adapterId);
      return toolKey === null ? null : (lines.get(toolKey) ?? null);
    },
    [chinese, lines, toolIconKey],
  );
}
