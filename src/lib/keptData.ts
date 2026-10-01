import type { KeptData, Measured, Warning } from "./types";

/**
 * One folder or file an uninstall leaves behind, as its preview names it
 * (`Warning.KeepsData`, added by `Session::issue_plan` from
 * crates/banager-core/src/kept_data.rs): the path as the table spells it,
 * `~` and all; what it holds; and about how much it takes, or null when
 * that is not known -- it leads somewhere Banager never looks into, it
 * could not be read, or the preview's budget ran out first.
 */
export interface KeptDataItem {
  path: string;
  what: KeptData;
  size: Measured | null;
  /** Folders inside it that `size` does not count (`Warning.KeepsData`'s `left_out`). */
  leftOut: string[];
}

/** The `KeepsData` lines of a plan's warnings, in order. */
export function keptDataOf(warnings: readonly Warning[]): KeptDataItem[] {
  const items: KeptDataItem[] = [];
  for (const warning of warnings) {
    if (typeof warning !== "string" && "KeepsData" in warning) {
      const { path, what, size, left_out } = warning.KeepsData;
      // An older line may have none.
      items.push({ path, what, size, leftOut: left_out ?? [] });
    }
  }
  return items;
}
