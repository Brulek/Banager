import type { ReactNode } from "react";
import type { TFunction } from "i18next";
import type { InstalledArtifact, ManagerInstance } from "../lib/types";
import { uncheckedUpdatesOf } from "../lib/uncheckedStandalone";
import { detailLines } from "./updateDetails";

/**
 * The Installed page's word, in place of 「已是最新」, for a tool whose
 * updates Banager does not check (`UNCHECKED_STANDALONE`: Codex's and
 * opencode's own installs) -- 「会自行更新」 when its install follows the latest release
 * (opencode's: 「默认会自行更新」, its default, as its setting is not read),
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
  // opencode's own install: its documentation says it downloads updates
  // itself by default (not a marker Banager read, as Codex's is), and no
  // version is read for it, which its ⓘ says too.
  const opencode = instance.adapter_id === "standalone-opencode";
  const itselfDetail = opencode
    ? [
        t("standalone.opencode.updatesItselfDetail", { source: label }),
        t("standalone.opencode.versionNotRead", { source: label }),
      ]
    : [t("codexStandalone.updatesItselfDetail", { source: label })];
  // Codex's word rests on the marker Banager read; opencode's on its
  // default alone (its `autoupdate` setting is not read), so it says so.
  return state === "updatesItself"
    ? {
        id: "updates-itself",
        label: t(opencode ? "opencodeUpdates.word" : "codexStandalone.updatesItself"),
        ariaLabel: t(opencode ? "opencodeUpdates.aria" : "codexStandalone.updatesItselfAria", { name }),
        detail: detailLines(itselfDetail),
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
