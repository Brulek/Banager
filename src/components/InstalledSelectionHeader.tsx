import type { MouseEvent } from "react";
import { useTranslation } from "react-i18next";
import { batchSizeOf, MAX_BATCH_UNINSTALL, selectAllAction } from "../lib/batchUninstall";
import { sizeText } from "../lib/sizes";
import type { InstalledArtifact, Sizes } from "../lib/types";
import { useUiStore } from "../store/ui";
import { BUTTON } from "./ui/controls";

export interface InstalledSelectionHeaderProps {
  /** The rows the list shows now that can be ticked (`tickable`), in its order. */
  shown: readonly InstalledArtifact[];
  /** Those of them that are ticked: what 「卸载所选」 would uninstall (`countedTicks`). */
  counted: readonly InstalledArtifact[];
  /** How much each tool takes (`useSizes`), for what the ticked ones take together. */
  sizes: Sizes | undefined;
}

/**
 * What the Installed page's list header says on its right: how many rows
 * are ticked and about how much they take together -- 「已选择3个 · 约1.2
 * GB」 -- or, past the most one batch takes, that limit; nothing while
 * nothing is ticked and the rows shown are few enough to tick at once.
 */
function statusOf(
  t: ReturnType<typeof useTranslation>["t"],
  shown: number,
  counted: readonly InstalledArtifact[],
  sizes: Sizes | undefined,
): string | null {
  const max = MAX_BATCH_UNINSTALL;
  if (counted.length > max) return t("batchUninstall.overLimit", { count: counted.length, max });
  if (counted.length > 0) {
    const { measured } = batchSizeOf(sizes, counted);
    return measured === null
      ? t("batchUninstall.selected", { count: counted.length })
      : t("batchUninstall.selectedSize", { count: counted.length, size: sizeText(t, measured) });
  }
  return shown > max ? t("batchUninstall.limit", { max }) : null;
}

/**
 * The Installed page's list header (author decision 1, in the Updates
 * page's own look): 28 high over the list, still while it scrolls, a box
 * in the rows' checkbox column that ticks every row the list shows that
 * can be ticked -- ticked for all, a dash for some -- and on its right
 * what is ticked. Its box ticks them only when there are no more of them
 * than one batch takes (`MAX_BATCH_UNINSTALL`); past that it only clears.
 * Off when no row the list shows can be ticked.
 */
export function InstalledSelectionHeader({ shown, counted, sizes }: InstalledSelectionHeaderProps) {
  const { t } = useTranslation();
  const selectUninstalls = useUiStore((s) => s.selectUninstalls);
  const deselectUninstalls = useUiStore((s) => s.deselectUninstalls);
  const all = shown.length > 0 && counted.length === shown.length;
  const toggleAll = () => {
    const keys = shown.map((artifact) => artifact.key);
    if (selectAllAction(shown.length, counted.length) === "select") selectUninstalls(keys);
    else deselectUninstalls(keys);
  };
  return (
    <div data-selection-header="" className="flex h-7 shrink-0 items-center gap-3 border-b border-separator px-5">
      <label className="flex min-w-0 items-center gap-3 text-body text-foreground">
        <input
          type="checkbox"
          ref={(box) => {
            if (box !== null) box.indeterminate = counted.length > 0 && !all;
          }}
          checked={all}
          disabled={shown.length === 0}
          onChange={toggleAll}
          aria-label={t("batchUninstall.selectAllLabel")}
          className="h-4 w-4 shrink-0"
        />
        <span aria-hidden="true" className={shown.length === 0 ? "text-tertiary" : undefined}>
          {t("batchUninstall.selectAll")}
        </span>
      </label>
      <p role="status" data-selection-status="" className="ml-auto truncate text-small text-muted">
        {statusOf(t, shown.length, counted, sizes)}
      </p>
    </div>
  );
}

export interface UninstallSelectedButtonProps {
  /** How many ticked rows the list shows (`countedTicks`). */
  count: number;
  /** The batch's sheet is up: one batch at a time. */
  sheetOpen: boolean;
  /** Opens the sheet, handed the button, which gets the focus back when it closes. */
  onOpen: (opener: HTMLElement) => void;
}

/**
 * 「卸载所选（3）…」, in the toolbar after the search field: grey, as every
 * Uninstall is -- offered, not recommended, never red -- and only while
 * something is ticked: there is no "uninstall all". Off past the most one
 * batch takes, and while its sheet is up.
 */
export function UninstallSelectedButton({ count, sheetOpen, onOpen }: UninstallSelectedButtonProps) {
  const { t } = useTranslation();
  if (count === 0) return null;
  return (
    <button
      type="button"
      data-uninstall-selected=""
      disabled={count > MAX_BATCH_UNINSTALL || sheetOpen}
      onClick={(event: MouseEvent<HTMLButtonElement>) => onOpen(event.currentTarget)}
      className={BUTTON.regular.grey}
    >
      {t("batchUninstall.uninstallSelected", { number: count })}
    </button>
  );
}
