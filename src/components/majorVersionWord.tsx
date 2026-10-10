import type { TFunction } from "i18next";
import type { UpdateCandidate } from "../lib/types";
import { majorJump } from "../lib/versionJump";
import type { StatusChipProps } from "./StatusChip";

/**
 * The Updates page's status word for an update that changes the major
 * version (`majorJump`, research synthesis §4.2 item 3): 「大版本更新」, muted
 * like every other word in the column, with an ⓘ that says what changes --
 * 「从3升到4，用法或设置可能会变。」 -- and says where the row's own way to
 * wait is: "Skip This Version" in the ⋯ at the row's right, by that item's
 * own words, and what it does (`updates.skipVersionHint`). It promises no
 * release notes: Banager has none to show. Nothing about the tools it
 * never marks (a 0.x one): a line about other tools, under this one's,
 * read as something about this one that was missed (walk-2 W2-14).
 *
 * The word is the row's status word, so it is in the row's accessible name
 * (`ToolRow`'s `statusText`) and its button is named with the tool's name.
 * It neither ticks nor unticks the row, nor moves it: what "Update All"
 * selects is unchanged.
 *
 * `undefined` for an update that does not change the major version.
 */
export function majorVersionWord(
  t: TFunction,
  candidate: UpdateCandidate,
  name: string,
): StatusChipProps | undefined {
  const jump = majorJump(candidate);
  if (jump === null) return undefined;
  return {
    label: t("majorVersion.tag"),
    ariaLabel: t("majorVersion.tagLabel", { name }),
    detail: (
      <p data-major-version-detail="">
        {t("majorVersion.detail", { from: jump.from, to: jump.to, skip: t("updates.skipVersion") })}
      </p>
    ),
  };
}
