import type { ReactNode } from "react";
import type { TFunction } from "i18next";
import { twinVerdict, type Twin } from "../lib/commands";
import type { InstalledArtifact } from "../lib/types";
import type { StatusChipProps } from "./StatusChip";
import { detailLines } from "./updateDetails";

/** Pure, and read outside the components too: src/lib/commands.ts. */
export { twinVerdict, type TwinVerdict } from "../lib/commands";

/**
 * What to make of a tool installed more than once (research synthesis
 * §2.1, pain #1/#4): not only that there are two copies (「装了两份」), but
 * which one typing its command runs, that Terminal does not use the other,
 * and -- for the copy it does not use -- that it may be uninstalled. Said
 * from `ArtifactFacts.commands` alone, as the 「在终端里输入时」 group judges
 * them (against the `PATH` read when the app opened); nothing is said where
 * the shared commands have no single verdict.
 */

/** A source's name in the user's language, by instance id (`InstalledPage`'s `sourceLabelFor`). */
type SourceLabel = (instanceId: string) => string;

/**
 * Who installed a copy, as a sentence names it: 「npm」, or for a tool's
 * own installer 「Codex自带的安装程序」 -- not 「Codex」 alone, which reads
 * as the tool itself.
 */
export function installedBy(t: TFunction, instanceId: string, sourceLabelFor: SourceLabel): string {
  const source = sourceLabelFor(instanceId);
  return instanceId.startsWith("standalone-") ? t("clarity.ownInstaller", { source }) : source;
}

/**
 * The sentences under the inspector's description for a tool installed
 * more than once, or null where the verdict says nothing
 * (`twinVerdict`): 「在终端里输入“codex”，运行的是Codex自带的安装程序装的
 * 那一份，版本0.159.3；终端不会用到这一份。」 and, where this copy can be
 * uninstalled here, 「不需要的话，可以卸载这一份。」; or, for the copy that
 * runs, which other one Terminal does not use.
 */
export function twinAdviceLines(
  t: TFunction,
  artifact: InstalledArtifact,
  twins: readonly Twin[] | undefined,
  sourceLabelFor: SourceLabel,
  removable: boolean,
): string[] | null {
  const verdict = twinVerdict(artifact, twins);
  if (verdict === null) return null;
  if (verdict.kind === "unused") {
    const by = installedBy(t, verdict.by.key.instance_id, sourceLabelFor);
    const version = verdict.by.version;
    const first =
      version === ""
        ? t("clarity.twinUnusedNoVersion", { command: verdict.command, by })
        : t("clarity.twinUnused", { command: verdict.command, by, version });
    return removable ? [first, t("clarity.twinUnusedRemovable")] : [first];
  }
  if (verdict.others.length === 1) {
    const other = verdict.others[0].artifact;
    const by = installedBy(t, other.key.instance_id, sourceLabelFor);
    return [
      other.version === ""
        ? t("clarity.twinUsedNoVersion", { command: verdict.command, by })
        : t("clarity.twinUsed", { command: verdict.command, by, version: other.version }),
    ];
  }
  return [t("clarity.twinUsedMany", { command: verdict.command, count: verdict.others.length })];
}

/**
 * The Updates page's word for an update of a copy Terminal does not run
 * (`twinVerdict`'s `unused`): 「终端用另一份」, its why behind an ⓘ --
 * which copy runs, and that updating this one does not change it.
 * `undefined` for any other row.
 */
export function notUsedWord(
  t: TFunction,
  artifact: InstalledArtifact | undefined,
  twins: readonly Twin[] | undefined,
  sourceLabelFor: SourceLabel,
  name: string,
): StatusChipProps | undefined {
  if (artifact === undefined) return undefined;
  const verdict = twinVerdict(artifact, twins);
  if (verdict?.kind !== "unused") return undefined;
  const lines = twinAdviceLines(t, artifact, twins, sourceLabelFor, false) ?? [];
  return {
    label: t("clarity.notUsedWord"),
    ariaLabel: t("clarity.notUsedAria", { name }),
    detail: detailLines([...lines, t("clarity.notUsedUpdate")]),
  };
}

/**
 * What the uninstall confirmation says of the other copies: that each
 * stays -- an uninstall removes only its own copy -- and, where Terminal
 * runs another copy now, that the command still works after. Nothing
 * where nothing says which copy runs.
 */
export function twinUninstallLine(
  t: TFunction,
  artifact: InstalledArtifact,
  twins: readonly Twin[] | undefined,
  sourceLabelFor: SourceLabel,
): string | null {
  if (twins === undefined || twins.length === 0) return null;
  const verdict = twinVerdict(artifact, twins);
  if (verdict?.kind === "unused") {
    return t("clarity.twinUninstallStillRuns", {
      by: installedBy(t, verdict.by.key.instance_id, sourceLabelFor),
      command: verdict.command,
    });
  }
  if (twins.length === 1) {
    return t("clarity.twinUninstallStays", { by: installedBy(t, twins[0].artifact.key.instance_id, sourceLabelFor) });
  }
  return t("clarity.twinUninstallOthersStay", { count: twins.length });
}

/**
 * The inspector's callout right under the description: what Homebrew's
 * mark means and what Homebrew suggests instead, and which copy of a tool
 * installed more than once Terminal runs -- the facts that ask something
 * of the user, set in one quiet box in the group's fill, before the
 * facts, as App Store and System Settings put a status above the details;
 * its text starts where the group rows' text does. Nothing when there is
 * nothing to say.
 */
export function InspectorCallout({ children }: { children: ReactNode[] }) {
  const lines = children.filter((child) => child !== null && child !== undefined && child !== false);
  if (lines.length === 0) return null;
  return (
    <div data-inspector-callout="" className="mt-4 flex flex-col gap-1.5 rounded-group bg-group px-2.5 py-2">
      {lines}
    </div>
  );
}
