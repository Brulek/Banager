import type { InstalledArtifact } from "./types";
import type { SourceNoticeSpec } from "./sources";
import { artifactKeyId } from "../store/ui";

/**
 * What a list shows (the toolbar's 「显示」 popup, `ToolShowButton`): every
 * tool, or only the AI coding tools -- the artifacts Rust tagged with a
 * family from its bundled table (`families::assign`,
 * crates/banager-core/src/families.rs). The page reads `facts.family` and
 * nothing else: which package is which tool is decided once, in Rust.
 */
export type ToolShow = "all" | "ai";

/**
 * The Installed page's popup also offers 「装了不止一份」: the tools
 * another source installed a copy of too -- exactly the rows that carry
 * the 「装了两份」 word (`twinsByArtifact`, src/lib/commands.ts). The
 * Updates page keeps `ToolShow`'s two.
 */
export type InstalledShow = ToolShow | "twins" | DiscoverShow;

/**
 * The Installed page's choices that find a fact a tool's inspector shows,
 * without opening each tool's: 「终端里找不到」, the tools with a command
 * Terminal does not find (`notOnPath`), and 「Homebrew已停用或弃用」, the
 * formulae and casks Homebrew disabled or deprecated (`isBrewRetired`).
 * Each is offered with how many it shows (`discoverCounts`), and the
 * notices over the list point at them (`discoverNotices`).
 */
export type DiscoverShow = "notOnPath" | "brewRetired";

/** The discovery choices, in the popup's order. */
export const DISCOVER_SHOWS: readonly DiscoverShow[] = ["notOnPath", "brewRetired"];

/**
 * Whether `show` is a discovery choice. The list unfolds a source's
 * components under one, so every tool its count counted is a row.
 */
export function isDiscoverShow(show: InstalledShow): show is DiscoverShow {
  return (DISCOVER_SHOWS as readonly string[]).includes(show);
}

/** Whether this artifact is a copy of a known AI coding tool. */
export function isAiTool(artifact: Pick<InstalledArtifact, "facts"> | undefined): boolean {
  return artifact !== undefined && artifact.facts.family !== null;
}

/**
 * Whether Terminal does not find one of the commands this artifact puts on
 * the Mac: a command whose verdict is `NotOnPath` (`commands::judge`). A
 * command Banager says nothing about (`state: null`) is not one. One is
 * enough, so a tool whose other commands run is listed too; the line's ⓘ
 * says "at least one command" (`families.notOnPathNoticeDetail`).
 */
export function hasCommandNotOnPath(artifact: Pick<InstalledArtifact, "facts"> | undefined): boolean {
  return (
    artifact !== undefined &&
    artifact.facts.commands.some(
      ({ state }) => typeof state === "object" && state !== null && "NotOnPath" in state,
    )
  );
}

/** Whether Homebrew disabled or deprecated this formula or cask (`facts.homebrew`). */
export function isBrewRetired(artifact: Pick<InstalledArtifact, "facts"> | undefined): boolean {
  const homebrew = artifact?.facts.homebrew;
  return homebrew != null && (homebrew.disabled !== null || homebrew.deprecated !== null);
}

/** How many artifacts each discovery choice shows. */
export type DiscoverCounts = Record<DiscoverShow, number>;

/**
 * How many of `artifacts` each discovery choice shows: the popup's counts
 * and the notices' (`discoverNotices`), from the same rule the list goes by
 * (`shownBy`), so the number said is the number of rows 查看 shows.
 */
export function discoverCounts(artifacts: readonly Pick<InstalledArtifact, "facts">[]): DiscoverCounts {
  return {
    notOnPath: artifacts.filter((artifact) => hasCommandNotOnPath(artifact)).length,
    brewRetired: artifacts.filter((artifact) => isBrewRetired(artifact)).length,
  };
}

/**
 * Whether a list showing `show` shows this artifact. `twins` is the
 * snapshot's other copies by `artifactKeyId` (`useTwins`): what
 * 「装了不止一份」 goes by, and all it goes by.
 */
export function shownBy(
  show: InstalledShow,
  artifact: Pick<InstalledArtifact, "facts" | "key"> | undefined,
  twins?: ReadonlyMap<string, unknown>,
): boolean {
  if (show === "all") return true;
  if (show === "ai") return isAiTool(artifact);
  if (show === "notOnPath") return hasCommandNotOnPath(artifact);
  if (show === "brewRetired") return isBrewRetired(artifact);
  return artifact !== undefined && twins !== undefined && twins.has(artifactKeyId(artifact.key));
}

/** Each discovery choice's line over the list: its words and their ⓘ. */
const DISCOVER_NOTICE_KEYS: Record<DiscoverShow, { title: string; description: string }> = {
  notOnPath: { title: "families.notOnPathNotice", description: "families.notOnPathNoticeDetail" },
  brewRetired: { title: "families.brewRetiredNotice", description: "families.brewRetiredNoticeDetail" },
};

/**
 * The lines the Installed page adds after the sources' own notices while
 * it shows every tool: one for each discovery choice that has tools to
 * show, 「2个工具在终端里找不到」, whose 查看 picks that choice in the
 * 「显示」 popup. An info line, as a source's 「终端找不到它」 is: the tools
 * work, the user just would not see it without opening each one. With
 * another choice picked, none -- the list already is one of them, or
 * says what it shows.
 */
export function discoverNotices(show: InstalledShow, counts: DiscoverCounts): SourceNoticeSpec[] {
  if (show !== "all") return [];
  return DISCOVER_SHOWS.filter((choice) => counts[choice] > 0).map((choice) => ({
    id: `discover:${choice}`,
    variant: "info",
    titleKey: DISCOVER_NOTICE_KEYS[choice].title,
    descriptionKey: DISCOVER_NOTICE_KEYS[choice].description,
    values: { count: counts[choice] },
    action: { id: "showList", labelKey: "families.view", show: choice },
  }));
}
