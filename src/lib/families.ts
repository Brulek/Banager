import type { InstalledArtifact } from "./types";
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
export type InstalledShow = ToolShow | "twins";

/** Whether this artifact is a copy of a known AI coding tool. */
export function isAiTool(artifact: Pick<InstalledArtifact, "facts"> | undefined): boolean {
  return artifact !== undefined && artifact.facts.family !== null;
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
  return artifact !== undefined && twins !== undefined && twins.has(artifactKeyId(artifact.key));
}
