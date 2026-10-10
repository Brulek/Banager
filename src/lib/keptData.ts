import { artifactKeyId } from "../store/ui";
import type { InstalledArtifact, KeptData, Measured, OthersData, Warning } from "./types";

/**
 * One folder or file an uninstall leaves behind, as its preview names it
 * (`Warning.KeepsData`, added by `Session::issue_plan` from
 * crates/banager-core/src/kept_data.rs): the path as the table spells it,
 * `~` and all; what it holds; and about how much it takes, or null when
 * that is not known -- it leads somewhere Banager never looks into, it
 * could not be read, or the preview's budget ran out first.
 */
export interface KeptDataItem {
  path: string;
  what: KeptData;
  size: Measured | null;
  /** Folders inside it that `size` does not count (`Warning.KeepsData`'s `left_out`). */
  leftOut: string[];
  /** Another tool's data inside it, not counted in `size` (`~/.gemini/antigravity-cli` in `~/.gemini`). */
  others: OthersData[];
}

/**
 * Folders an uninstall keeps that apps no source lists keep their own
 * settings and data in too, at the top of the folder rather than in one of
 * their own (which `kept_data::APPS_INSIDE` would leave out of the size
 * instead), with each app's product name: `~/.codex`, the ChatGPT app's
 * and the Codex app's (Homebrew's casks `chatgpt` and `codex-app`, whose
 * `zap` only removes it if empty, `rmdir`, as a folder others share), and
 * `~/.copilot`, the GitHub Copilot app's (the cask `github-copilot-app`,
 * whose `zap` trashes it). Read in Homebrew's API cache on the author's
 * Mac on 2026-10-10, never run. Under such a folder's line the group says
 * the apps keep their data there too, if installed, and it says nothing of
 * the Trash: an app downloaded from its vendor is not on any list, so
 * Banager cannot tell it is not there.
 */
export const SHARED_WITH_APPS: Readonly<Record<string, readonly string[]>> = {
  "~/.codex": ["ChatGPT", "Codex"],
  "~/.copilot": ["GitHub Copilot"],
};

/** The `KeepsData` lines of a plan's warnings, in order. */
export function keptDataOf(warnings: readonly Warning[]): KeptDataItem[] {
  const items: KeptDataItem[] = [];
  for (const warning of warnings) {
    if (typeof warning !== "string" && "KeepsData" in warning) {
      const { path, what, size, left_out, others } = warning.KeepsData;
      // An older line may have neither.
      items.push({ path, what, size, leftOut: left_out ?? [], others: others ?? [] });
    }
  }
  return items;
}

/**
 * Whether a tool of the same family (`facts.family`, the AI coding tools'
 * table) as one of `going` stays installed once they are uninstalled:
 * another copy of it -- npm's `@openai/codex` beside Codex's own install,
 * either way round -- or another of the family's tools, Ollama's app
 * beside Homebrew's `ollama`. The folders an uninstall keeps are the
 * family's (`kept_data::data_paths`), so a tool that stays may still run
 * from them or sign in with them: Codex's own install runs from
 * `~/.codex/packages/standalone` and keeps its login in `~/.codex`. The
 * confirmation then does not say they can go to the Trash
 * (`KeptDataGroup`, the author's decision U15 e).
 */
export function familyStaysAfter(going: readonly InstalledArtifact[], artifacts: readonly InstalledArtifact[]): boolean {
  const ids = new Set(going.map((artifact) => artifactKeyId(artifact.key)));
  const families = new Set(going.flatMap((artifact) => (artifact.facts.family === null ? [] : [artifact.facts.family])));
  return artifacts.some(
    (artifact) =>
      artifact.facts.family !== null && families.has(artifact.facts.family) && !ids.has(artifactKeyId(artifact.key)),
  );
}
