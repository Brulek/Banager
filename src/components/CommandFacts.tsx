import { useId, useMemo, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { CopyButton } from "./CopyButton";
import { commandGroups, stateId, twinsByArtifact, type CommandGroup, type Twin } from "../lib/commands";
import { namesInSentence } from "../lib/sources";
import type { CommandState, InstalledArtifact } from "../lib/types";
import { artifactKeyId } from "../store/ui";
import { InfoDetail, TextWithInfo } from "./InfoDetail";
import { GROUP, GROUP_ROW_TWO_LINES, GROUP_TITLE, SMALL_WRAPPING } from "./ui/group";
import { installedBy } from "./TwinAdvice";
import { detailLines } from "./updateDetails";

/**
 * What typing a tool's commands in Terminal runs, for the Installed page
 * (`ArtifactFacts.commands`): the inspector's group, and the row's word
 * for a tool installed more than once. Nothing here fixes anything -- no
 * button edits a shell file or changes the order of `PATH`; the one action
 * is Copy Path, for the folder Terminal does not search.
 */

/** A source's name in the user's language, by instance id (`InstalledPage`'s `sourceLabelFor`). */
type SourceLabel = (instanceId: string) => string;

type Translate = ReturnType<typeof useTranslation>["t"];

/** At most this many names on one line; the rest are counted (`commands.names`). */
const NAMES_SHOWN = 3;

/** Where the shown names go in `commands.names`, to set them apart from the words around them. */
const NAMES_SLOT = "\u0000names\u0000";

/**
 * `names` with the tool's own command first, where it has one: the name
 * the tool goes by (`python3.13` for `python@3.13`, `git` for `git`), not
 * `idle3.13` because it sorts first.
 */
export function mainCommandFirst(names: string[], artifact: InstalledArtifact): string[] {
  const base = (artifact.key.name.split("/").pop() ?? artifact.key.name).toLowerCase();
  const wanted = new Set([base, base.replace("@", ""), artifact.display_name.toLowerCase()]);
  const main = names.findIndex((name) => wanted.has(name.toLowerCase()));
  return main <= 0 ? names : [names[main], ...names.slice(0, main), ...names.slice(main + 1)];
}

/**
 * A group's commands, as its line names them: 「cargo、cargo-clippy、
 * cargo-fmt等14个」 -- the last name and 「等14个」 held on one line, so the
 * count never stands alone on the next.
 */
function namesOf(t: Translate, names: string[]): ReactNode {
  const separator = t("common.listSeparator");
  if (names.length <= NAMES_SHOWN) return names.join(separator);
  const shown = names.slice(0, NAMES_SHOWN);
  const [before, after = ""] = t("commands.names", {
    names: NAMES_SLOT,
    count: names.length,
    rest: names.length - NAMES_SHOWN,
  }).split(NAMES_SLOT);
  const last = shown[shown.length - 1];
  return (
    <>
      {before}
      {shown.slice(0, -1).map((name) => name + separator)}
      <span className="whitespace-nowrap">
        {last}
        {after}
      </span>
    </>
  );
}

/** `namesOf` as plain text, for an accessible name. */
function namesText(t: Translate, names: string[]): string {
  const separator = t("common.listSeparator");
  if (names.length <= NAMES_SHOWN) return names.join(separator);
  return t("commands.names", {
    names: names.slice(0, NAMES_SHOWN).join(separator),
    count: names.length,
    rest: names.length - NAMES_SHOWN,
  });
}

/**
 * What the line says that typing them runs, and the why behind its ⓘ:
 * this copy; the other copy of the same tool, from its source; another
 * program with the name, from its source when some artifact provides it;
 * or nothing of this copy's, in the folder the line names.
 */
function verdictOf(
  t: Translate,
  artifact: InstalledArtifact,
  state: CommandState,
  artifacts: readonly InstalledArtifact[],
  sourceLabelFor: SourceLabel,
): { text: string; detail: string | null; dir: string | null } {
  if (state === "Runs") return { text: t("commands.runs"), detail: null, dir: null };
  if ("NotOnPath" in state) {
    const dir = state.NotOnPath.dir;
    return { text: t("commands.notFound", { dir }), detail: t("notOnPathMore.notFoundDetail"), dir };
  }
  const detail = t("commands.behindDetail");
  const by = state.ShadowedBy.by;
  if (by === null) return { text: t("commands.runsNamesake"), detail, dir: null };
  const owner = artifacts.find((other) => artifactKeyId(other.key) === artifactKeyId(by));
  const source = sourceLabelFor(by.instance_id);
  const family = artifact.facts.family;
  const sameTool = family !== null && owner?.facts.family === family;
  return {
    // The copy from a tool's own installer by that installer
    // (「Codex自带的安装程序」), not by the tool's name, which reads as the
    // tool itself (`installedBy`).
    text: sameTool
      ? t("commands.runsCopyFrom", { source: installedBy(t, by.instance_id, sourceLabelFor) })
      : t("commands.runsNamesakeFrom", { source }),
    detail,
    dir: null,
  };
}

/**
 * The inspector's 「在终端里输入」 group (spec: advantages round, item 4),
 * under the facts: a line per verdict -- the commands it is about, and
 * under them, in the secondary colour, what typing them runs -- with
 * Copy Path beside a folder Terminal does not search, which copies the
 * folder as the line shows it, `~` and all. The ⓘ by the title says what
 * the verdicts are judged against, and what that does not see: an alias,
 * a terminal opened later or an editor's. Nothing when Banager said
 * nothing about any of the tool's commands.
 */
export function CommandsGroup({
  artifact,
  artifacts,
  sourceLabelFor,
}: {
  artifact: InstalledArtifact;
  artifacts: readonly InstalledArtifact[];
  sourceLabelFor: SourceLabel;
}) {
  const { t } = useTranslation();
  const titleId = useId();
  const groups: CommandGroup[] = commandGroups(artifact.facts.commands);
  if (groups.length === 0) return null;
  const title = t("commands.title");
  return (
    <section data-commands="" aria-labelledby={titleId} className="mt-4">
      {/* The ⓘ after the heading, not in it: in it, its name -- 「详情：
          在终端里输入」 -- would be part of the heading's, and of the
          group's, said twice over. */}
      <div className={`${GROUP_TITLE} whitespace-nowrap`}>
        <h3 id={titleId} className="inline">
          {title}
        </h3>{" "}
        <InfoDetail label={t("common.detailsLabel", { title })}>{t("commands.titleDetail")}</InfoDetail>
      </div>
      <ul className={GROUP}>
        {groups.map((group) => {
          const ordered = mainCommandFirst(group.names, artifact);
          const names = namesOf(t, ordered);
          const verdict = verdictOf(t, artifact, group.state, artifacts, sourceLabelFor);
          return (
            <li key={`${group.names[0]}`} data-command-line="" className={GROUP_ROW_TWO_LINES}>
              <div className="min-w-0 flex-1">
                <p className="break-words text-body text-foreground">{names}</p>
                <p data-command-verdict="" className={`mt-0.5 break-words ${SMALL_WRAPPING} text-muted`}>
                  {verdict.detail === null ? (
                    verdict.text
                  ) : (
                    <TextWithInfo text={verdict.text} label={t("common.detailsLabel", { title: namesText(t, ordered) })}>
                      {verdict.detail}
                    </TextWithInfo>
                  )}
                </p>
              </div>
              {verdict.dir !== null ? (
                <CopyButton
                  text={verdict.dir}
                  label={t("commands.copyPath")}
                  ariaLabel={t("commands.copyPathLabel", { dir: verdict.dir })}
                />
              ) : null}
            </li>
          );
        })}
      </ul>
    </section>
  );
}

/** Every artifact's other copies of the same tool, by `artifactKeyId`, once per snapshot. */
export function useTwins(artifacts: readonly InstalledArtifact[] | undefined): Map<string, Twin[]> {
  return useMemo(() => twinsByArtifact(artifacts ?? []), [artifacts]);
}

/** A row's status word, in the shape `InstalledPage`'s chips have. */
export interface TwinChip {
  id: "twin";
  label: string;
  detail: ReactNode;
  /**
   * What the ⓘ says in the details' 「状态」: where the other copies are
   * from, and no more -- the 「在终端里输入」 group under it says which
   * copy typing the command runs, with its own ⓘ on how that was judged.
   */
  inspectorDetail: ReactNode;
  ariaLabel: string;
  tone: "neutral";
}

/**
 * Where the other copies are from, behind the twin word's ⓘ: 「npm也装了一份。」;
 * a standalone tool's own install by its installer, not by its name
 * alone (「Claude Code自带的安装程序也装了一份。」); 「npm和Homebrew也各装了一份。」;
 * and, where one source has more than one (two npm folders), how many
 * there are in all (「另外2份由npm安装。」) -- never a count of sources
 * passed off as one of copies.
 */
function whereTheOthersAre(t: Translate, twins: Twin[], sourceLabelFor: SourceLabel): string {
  const bySource = new Map<string, number>();
  for (const twin of twins) {
    const source = sourceLabelFor(twin.artifact.key.instance_id);
    bySource.set(source, (bySource.get(source) ?? 0) + 1);
  }
  const sources = [...bySource.keys()];
  if (twins.length === 1) {
    const standalone = twins[0].artifact.key.instance_id.startsWith("standalone-");
    return t(standalone ? "commands.twinOtherStandalone" : "commands.twinOther", { source: sources[0] });
  }
  if (sources.length === twins.length) return t("commands.twinOthers", { sources: namesInSentence(t, sources) });
  return t("commands.twinOthersCount", { count: twins.length, sources: namesInSentence(t, sources) });
}

/**
 * The row's 「装了两份」 (「装了3份」 for three), for a tool another
 * source installed too (`twinsByArtifact`: the same AI coding tool, a
 * command of the same name) -- null for one that is not. Behind its ⓘ,
 * where the other copy is from, and what typing the shared command runs:
 * this copy, the other one, or not this one. A status word like the
 * others (`StatusChip`), shown on the row unless what the source allows
 * or why the tool cannot be uninstalled comes first (`chipsOf`).
 */
export function twinChip(
  t: Translate,
  artifact: InstalledArtifact,
  twins: Twin[] | undefined,
  sourceLabelFor: SourceLabel,
): TwinChip | null {
  if (twins === undefined || twins.length === 0) return null;
  const label = twins.length === 1 ? t("commands.twin") : t("commands.twinMany", { number: twins.length + 1 });
  const where = whereTheOthersAre(t, twins, sourceLabelFor);
  // Named only when every command the copies share has one verdict here:
  // `claude` that runs this copy says nothing of a second name that may not.
  const shared = [...new Set(twins.flatMap((twin) => twin.commands))];
  const states = shared.map((name) => artifact.facts.commands.find((fact) => fact.name === name)?.state ?? null);
  const state = states[0];
  const oneVerdict = state !== null && states.every((other) => other !== null && stateId(other) === stateId(state));
  const command = twins[0].commands[0];
  let runs: string | null = null;
  if (oneVerdict) {
    if (state === "Runs") runs = t("commands.twinRuns", { command });
    else if ("NotOnPath" in state) runs = t("commands.twinNotFound", { command });
    else if (state.ShadowedBy.by !== null) {
      const by = state.ShadowedBy.by;
      if (twins.some((twin) => artifactKeyId(twin.artifact.key) === artifactKeyId(by))) {
        runs = t("commands.twinRunsCopyFrom", { command, source: installedBy(t, by.instance_id, sourceLabelFor) });
      }
    }
  }
  return {
    id: "twin",
    label,
    detail: detailLines(runs === null ? [where] : [where, runs]),
    // Said there once: a verdict for the shared command is what both
    // `runs` and the commands group are made of.
    inspectorDetail: detailLines([where]),
    ariaLabel: t("commands.twinLabel", { word: label, name: artifact.display_name }),
    tone: "neutral",
  };
}
