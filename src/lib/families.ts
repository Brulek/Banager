import type { InstalledArtifact } from "./types";

/**
 * What a list shows (the toolbar's 「显示」 popup, `ToolShowButton`): every
 * tool, or only the AI coding tools -- the artifacts Rust tagged with a
 * family from its bundled table (`families::assign`,
 * crates/banager-core/src/families.rs). The page reads `facts.family` and
 * nothing else: which package is which tool is decided once, in Rust.
 */
export type ToolShow = "all" | "ai";

/** Whether this artifact is a copy of a known AI coding tool. */
export function isAiTool(artifact: Pick<InstalledArtifact, "facts"> | undefined): boolean {
  return artifact !== undefined && artifact.facts.family !== null;
}

/** Whether a list showing `show` shows this artifact. */
export function shownBy(show: ToolShow, artifact: Pick<InstalledArtifact, "facts"> | undefined): boolean {
  return show === "all" || isAiTool(artifact);
}
