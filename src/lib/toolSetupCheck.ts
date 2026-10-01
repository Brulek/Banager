/**
 * 「检查工具环境」, "Check Tool Setup": how this Mac's command-line tools are
 * set up, said in short lines a person reads at a glance -- whether the
 * login shell's settings were read, and how many of the folders Terminal
 * looks in for commands; each source's state; how many tools Terminal
 * cannot find, and how many are installed more than once; what Homebrew
 * disabled, deprecated or keeps other versions of; and the disk measured.
 * Help's 「检查工具环境…」 and a button in Settings' About open it
 * (`ToolSetupSheet`).
 *
 * Built only from what the window already holds -- the snapshot, the sizes
 * and `get_system_facts` -- with the rules the lists and the diagnostic
 * text go by (`discoverCounts`, `toolsInstalledTwice`, `commandsKnown`,
 * `sourceStateWords`, `sizeTotalsOf`), so a number here is the number the
 * list behind its 查看 shows. Nothing is run, read or asked for. No score
 * and no grade: a line that is fine says so with a muted ✓, one worth a
 * look has a muted ⓘ, and only what stops Banager from working -- a source
 * that does not answer, a check that did not finish, the login shell's
 * settings unread -- has the orange ⚠︎ the Overview gives a warning.
 */
import { create } from "zustand";
import { commandsKnown } from "./commandsKnown";
import { sourceStateWords, toolsInstalledTwice, type Translate } from "./diagnostics";
import { discoverCounts, keepsOtherVersions, notOnPathDetailKey, type DiscoverShow } from "./families";
import { modelsTotalText } from "./sizes";
import { sizeTotalsOf, sourceTotalText, type SizeTotal } from "./sizeTotals";
import { instanceLabels } from "./sources";
import type { InstalledArtifact, Sizes, Snapshot, SystemFacts } from "./types";
import { artifactKeyId } from "../store/ui";

/**
 * A line's one symbol: `fine` a muted ✓, `note` a muted ⓘ, `warning` the
 * orange ⚠︎, `busy` the spinner -- while what it is about is still being
 * found out.
 */
export type SetupSymbol = "fine" | "note" | "warning" | "busy";

/**
 * Where a line's 查看 goes, the sheet closing first: the Installed page on
 * every source with one of its 「显示」 choices; one source's own page; or
 * Other Programs.
 */
export type SetupView =
  | { kind: "installed"; show: DiscoverShow | "twins" }
  | { kind: "source"; instanceId: string }
  | { kind: "unknown" };

export interface SetupLine {
  /** Stable within its section: what a test and React's key go by. */
  id: string;
  symbol: SetupSymbol;
  text: string;
  /** The longer why, behind an ⓘ at the end of the text; null for none. */
  detail: string | null;
  /** A second line under it, 11 muted: the names of the sources it means, or paths. */
  secondary: string | null;
  /** Its 查看, or null for none. */
  view: SetupView | null;
}

export type SetupSectionId = "terminal" | "sources" | "commands" | "homebrew" | "disk";

/** Each section's title. A `Record`, so a section added without one fails `tsc`. */
const SECTION_TITLE_KEYS: Record<SetupSectionId, string> = {
  terminal: "setupCheck.sections.terminal",
  sources: "setupCheck.sections.sources",
  commands: "setupCheck.sections.commands",
  homebrew: "setupCheck.sections.homebrew",
  disk: "setupCheck.sections.disk",
};

export interface SetupSection {
  id: SetupSectionId;
  title: string;
  lines: SetupLine[];
}

export interface ToolSetupInput {
  /**
   * What the lists show: the snapshot, or while the first check still
   * checks for updates, its list (`previewSnapshot`); null while there is
   * neither.
   */
  snapshot: Snapshot | null;
  /** Whether the first check since launch has not finished: `snapshot` is its list, or null. */
  pending: boolean;
  /** `get_system_facts`'s answer; undefined while it is asked for, null when it could not be had. */
  facts: SystemFacts | null | undefined;
  sizes: Sizes | null;
  /** Settings' Show technical details: the unread folders by path. */
  technicalDetails: boolean;
}

export interface ToolSetupCheck {
  /** The first check has not finished: the sheet says so over the sections. */
  pending: boolean;
  sections: SetupSection[];
}

function line(
  id: string,
  symbol: SetupSymbol,
  text: string,
  more: Partial<Pick<SetupLine, "detail" | "secondary" | "view">> = {},
): SetupLine {
  return { id, symbol, text, detail: more.detail ?? null, secondary: more.secondary ?? null, view: more.view ?? null };
}

/** 「终端设置」: the login shell's settings, and the folders the last check read. */
function terminalLines(t: Translate, input: ToolSetupInput): SetupLine[] {
  const { facts } = input;
  if (facts === undefined) return [line("loading", "busy", t("common.loading"))];
  if (facts === null) return [line("noFacts", "warning", t("setupCheck.terminal.noFacts"))];
  // Not the login shell's: nothing is judged, and its few folders are the
  // system's, not the user's -- no count of them is worth saying.
  if (!facts.login_path) {
    return [
      line("loginNotRead", "warning", t("setupCheck.terminal.loginNotRead"), {
        detail: t("setupCheck.terminal.loginNotReadDetail"),
      }),
    ];
  }
  const folders = facts.path_folders ?? null;
  // No round has read them yet, or the last one could not in full: how
  // many there are, and nothing about which were read.
  if (folders === null) {
    return [
      line("loginRead", "fine", t("setupCheck.terminal.loginRead")),
      line("folders", "note", t("setupCheck.terminal.foldersCount", { number: facts.path_dirs.length })),
    ];
  }
  if (folders.unread.length === 0) {
    return [line("allFine", "fine", t("setupCheck.terminal.allFine", { count: folders.read }))];
  }
  return [
    line("loginRead", "fine", t("setupCheck.terminal.loginRead")),
    line(
      "foldersUnread",
      "note",
      t("setupCheck.terminal.foldersSomeUnread", { read: folders.read, unread: folders.unread.length }),
      {
        detail: t("setupCheck.terminal.foldersUnreadDetail"),
        // Paths only for someone who asked for them.
        secondary: input.technicalDetails
          ? t("setupCheck.terminal.unreadFolders", { folders: folders.unread.join(t("common.listSeparator")) })
          : null,
      },
    ),
  ];
}

/**
 * The diagnostics' status words, each written to stand alone ("View only",
 * "Version not tested"), as they read joined in one sentence: the first
 * as it is, the rest lower-cased. Every one starts with a common word, and
 * Chinese has no case to change.
 */
function inSentence(words: readonly string[]): string[] {
  return words.map((word, index) => (index === 0 ? word : word.charAt(0).toLowerCase() + word.slice(1)));
}

/**
 * 「来源」: each source not answering, read-only or on a version not tested
 * -- in the diagnostic text's words (`sourceStateWords`) -- or whose check
 * did not finish, a line of its own with 查看 to its page, those that do
 * not answer or did not finish first; the rest said
 * once, 「所有来源都正常回应」, with their names, two Homebrews named as the
 * sidebar names them (`instanceLabels`). Then where the programs no source
 * installed are: Other Programs. While the first check runs, the sources
 * have answered nothing yet, so the rest are only 「目前没有发现问题」.
 */
function sourceLines(t: Translate, input: ToolSetupInput): SetupLine[] {
  const { snapshot } = input;
  if (snapshot === null) return [line("checking", "busy", t("common.checking"))];
  if (snapshot.instances.length === 0) return [line("none", "note", t("setupCheck.sources.none"))];
  const labels = instanceLabels(t, snapshot.instances);
  const failed = new Set(snapshot.errors.map((error) => error.instance_id));
  const lines: SetupLine[] = [];
  const fine: string[] = [];
  for (const instance of snapshot.instances) {
    const label = labels.get(instance.id) ?? instance.adapter_id;
    const words = sourceStateWords(t, instance);
    const unfinished = failed.has(instance.id);
    if (unfinished) words.push(t("setupCheck.sources.checkUnfinished"));
    if (words.length === 0) {
      fine.push(label);
      continue;
    }
    lines.push(
      line(
        `source:${instance.id}`,
        instance.status.unavailable !== null || unfinished ? "warning" : "note",
        t("setupCheck.sources.state", { source: label, state: inSentence(words).join(t("common.listSeparator")) }),
        { view: { kind: "source", instanceId: instance.id } },
      ),
    );
  }
  // What stops a source from working first, as the Overview orders its problems.
  lines.sort((a, b) => Number(b.symbol === "warning") - Number(a.symbol === "warning"));
  if (fine.length > 0) {
    lines.push(
      line(
        "fine",
        "fine",
        // Before the first check is in, no source has answered anything yet.
        t(
          input.pending
            ? "setupCheck.sources.pendingFine"
            : lines.length === 0
              ? "setupCheck.sources.allFine"
              : "setupCheck.sources.othersFine",
        ),
        { secondary: fine.join(t("common.listSeparator")) },
      ),
    );
  }
  lines.push(line("otherPrograms", "note", t("setupCheck.sources.otherPrograms"), { view: { kind: "unknown" } }));
  return lines;
}

/**
 * 「命令」: how many tools Terminal cannot find, and how many are installed
 * more than once, each with 查看 to the Installed page's choice that lists
 * them -- or, where the check did not look at the commands, that it did
 * not (`commandsKnown`), as the diagnostic text says it.
 */
function commandLines(t: Translate, input: ToolSetupInput): SetupLine[] {
  const artifacts = input.snapshot?.artifacts ?? [];
  // Commands are judged only when the round ends.
  if (input.pending) return [line("previewing", "busy", t("commandsKnown.previewing"))];
  const verdicts = commandsKnown(artifacts, false, "verdicts") === "known";
  const names = commandsKnown(artifacts, false, "names") === "known";
  const notOnPath = discoverCounts(artifacts).notOnPath;
  const twins = toolsInstalledTwice(artifacts);
  if (verdicts && names && notOnPath === 0 && twins === 0) {
    return [line("allFine", "fine", t("setupCheck.commands.allFine"))];
  }
  const lines: SetupLine[] = [];
  if (!verdicts) {
    lines.push(line("notOnPath", "note", t("commandsKnown.notFoundUnknown")));
  } else if (notOnPath > 0) {
    lines.push(
      // A warning, not a note: Terminal cannot run these copies.
      line("notOnPath", "warning", t("families.notOnPathNotice", { count: notOnPath }), {
        detail: t(notOnPathDetailKey("notOnPath", notOnPath)!),
        view: { kind: "installed", show: "notOnPath" },
      }),
    );
  } else {
    lines.push(line("notOnPath", "fine", t("setupCheck.commands.notOnPathFine")));
  }
  if (!names) {
    lines.push(line("twins", "note", t("commandsKnown.twinsUnknown")));
  } else if (twins > 0) {
    lines.push(
      line("twins", "note", t("setupCheck.commands.twins", { count: twins }), {
        detail: t("setupCheck.commands.twinsDetail"),
        view: { kind: "installed", show: "twins" },
      }),
    );
  } else {
    lines.push(line("twins", "fine", t("setupCheck.commands.twinsFine")));
  }
  return lines;
}

/**
 * What the other versions of `formulae` take together, as measured for
 * `snapshot`'s round -- 「…以上」 when one of them has none measured -- or
 * null before that round is measured, or when nothing of them was.
 */
function otherVersionsTotal(
  formulae: readonly InstalledArtifact[],
  sizes: Sizes | null,
  snapshot: Snapshot,
): SizeTotal | null {
  if (sizes === null || !sizes.done || sizes.round !== snapshot.round) return null;
  const measured = new Map(sizes.artifacts.map((size) => [artifactKeyId(size.key), size]));
  let bytes = 0;
  let atLeast = false;
  for (const formula of formulae) {
    const size = measured.get(artifactKeyId(formula.key));
    const old = size?.old_versions ?? null;
    if (old === null || size?.version !== formula.version) {
      atLeast = true;
      continue;
    }
    bytes += old.bytes;
    atLeast ||= old.partial || old.at_least;
  }
  return bytes === 0 ? null : { bytes, atLeast };
}

/**
 * 「Homebrew」, on a Mac with one: how many formulae and casks it disabled
 * or deprecated, with 查看 to the Installed page's choice that lists them,
 * and how many keep other versions, with what those take once measured
 * and 查看 to the choice that lists those.
 * Of every Homebrew together.
 */
function homebrewLines(t: Translate, input: ToolSetupInput): SetupLine[] | null {
  const { snapshot } = input;
  if (snapshot === null) return null;
  const brews = new Set(
    snapshot.instances.filter((instance) => instance.adapter_id === "brew").map((instance) => instance.id),
  );
  if (brews.size === 0) return null;
  const artifacts = snapshot.artifacts.filter((artifact) => brews.has(artifact.key.instance_id));
  const retired = discoverCounts(artifacts).brewRetired;
  const keeping = artifacts.filter((artifact) => keepsOtherVersions(artifact));
  if (retired === 0 && keeping.length === 0) return [line("fine", "fine", t("setupCheck.homebrew.fine"))];
  const lines: SetupLine[] = [];
  lines.push(
    retired > 0
      ? line("retired", "note", t("families.brewRetiredNotice", { count: retired }), {
          detail: t("families.brewRetiredNoticeDetail"),
          view: { kind: "installed", show: "brewRetired" },
        })
      : line("retired", "fine", t("setupCheck.homebrew.retiredFine")),
  );
  if (keeping.length === 0) {
    lines.push(line("otherVersions", "fine", t("setupCheck.homebrew.otherVersionsFine")));
  } else {
    const total = otherVersionsTotal(keeping, input.sizes, snapshot);
    lines.push(
      line(
        "otherVersions",
        "note",
        total === null
          ? t("setupCheck.homebrew.otherVersions", { count: keeping.length })
          : t("setupCheck.homebrew.otherVersionsSize", { count: keeping.length, size: sourceTotalText(t, total) }),
        { detail: t("clarity.otherVersionsDetail"), view: { kind: "installed", show: "otherVersions" } },
      ),
    );
  }
  return lines;
}

/**
 * 「磁盘」: what the tools take together, with the toolbar's hedge
 * (`sizeTotalsOf`), and each Ollama's models (`modelsTotalText`) -- once
 * the snapshot's round is measured; until then, that it is being measured.
 */
function diskLines(t: Translate, input: ToolSetupInput): SetupLine[] | null {
  const { snapshot, sizes } = input;
  if (snapshot === null || snapshot.artifacts.length === 0) return null;
  const measuredForThis = !input.pending && sizes !== null && sizes.done && sizes.round === snapshot.round;
  if (!measuredForThis) return [line("measuring", "busy", t("setupCheck.disk.measuring"))];
  const lines: SetupLine[] = [];
  const total = sizeTotalsOf(sizes, snapshot).all;
  if (total !== null) {
    lines.push(
      line("total", "note", t("setupCheck.disk.total", { size: sourceTotalText(t, total) }), {
        detail: t("sizeTotals.note"),
      }),
    );
  }
  for (const instance of snapshot.instances) {
    if (instance.adapter_id !== "ollama") continue;
    const models = modelsTotalText(t, sizes, instance.id, snapshot.round);
    if (models !== null) lines.push(line(`models:${instance.id}`, "note", models));
  }
  return lines.length === 0 ? null : lines;
}

/**
 * The sheet's sections, in order, each with at least one line: 终端设置,
 * 来源, 命令, then Homebrew where there is one, then 磁盘 once there is
 * something on it. Pure: toolSetupCheck.test.ts builds it in both
 * languages.
 */
export function toolSetupCheck(t: Translate, input: ToolSetupInput): ToolSetupCheck {
  const sections: SetupSection[] = [];
  const add = (id: SetupSectionId, lines: SetupLine[] | null) => {
    if (lines !== null && lines.length > 0) sections.push({ id, title: t(SECTION_TITLE_KEYS[id]), lines });
  };
  add("terminal", terminalLines(t, input));
  add("sources", sourceLines(t, input));
  add("commands", commandLines(t, input));
  add("homebrew", homebrewLines(t, input));
  add("disk", diskLines(t, input));
  return { pending: input.pending, sections };
}

/** Whether the sheet is open: Help's item and Settings' button open it, wherever the window is. */
export const useToolSetupSheet = create<{ open: boolean }>(() => ({ open: false }));

/** Opens the sheet over whatever page is showing. */
export function openToolSetupSheet(): void {
  useToolSetupSheet.setState({ open: true });
}
