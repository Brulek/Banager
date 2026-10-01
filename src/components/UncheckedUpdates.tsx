import type { ReactNode } from "react";
import type { TFunction } from "i18next";
import type { InstalledArtifact, ManagerInstance } from "../lib/types";
import { uncheckedUpdatesOf } from "../lib/uncheckedStandalone";
import { detailLines } from "./updateDetails";

/**
 * The Installed page's word, in place of 「已是最新」, for a tool whose
 * updates Banager does not check (`UNCHECKED_STANDALONE`: Codex's own
 * install) -- 「它自己更新」 when its install follows the latest release,
 * else 「不检查更新」 -- with the why behind its ⓘ, shaped as the page's
 * `RowChip`. `null` for every other row. Quiet like the page's other
 * neutral words: a source's own limits, not a problem.
 */
export function uncheckedUpdatesChip(
  t: TFunction,
  artifact: InstalledArtifact,
  instance: ManagerInstance,
  label: string,
): { id: string; label: string; ariaLabel: string; detail: ReactNode; tone: "neutral" } | null {
  const state = uncheckedUpdatesOf(artifact, instance);
  if (state === null) return null;
  const name = artifact.display_name;
  return state === "updatesItself"
    ? {
        id: "updates-itself",
        label: t("codexStandalone.updatesItself"),
        ariaLabel: t("codexStandalone.updatesItselfAria", { name }),
        detail: detailLines([t("codexStandalone.updatesItselfDetail", { source: label })]),
        tone: "neutral",
      }
    : {
        id: "updates-not-checked",
        label: t("codexStandalone.notChecked"),
        ariaLabel: t("codexStandalone.notCheckedAria", { name }),
        detail: detailLines([t("codexStandalone.notCheckedDetail", { source: label })]),
        tone: "neutral",
      };
}
