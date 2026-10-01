import type { CommandFact, InstalledArtifact } from "./types";

/**
 * Why a tool is in the Installed page's search results: by its name -- the
 * name its row shows or the package's own, anywhere in either
 * ("visual-studio-code" finds "Microsoft Visual Studio Code") -- or, failing
 * that, by a command it puts on the Mac (`ArtifactFacts.commands`), the
 * word a user types in Terminal and may know better than the package's
 * name: "rg" finds ripgrep, "pip3.13" python@3.13. A command matches by its
 * start only, so "g" does not find ripgrep through "rg"; the command it
 * matched by is the one the row names (「命令：rg」).
 */
export type SearchMatch = { by: "name" } | { by: "command"; command: string };

const BY_NAME: SearchMatch = { by: "name" };

/**
 * How `artifact` matches `needle` -- the search field's text, trimmed and
 * lowercased -- or null where it does not. An empty needle matches every
 * tool, by its name.
 */
export function searchMatch(
  artifact: Pick<InstalledArtifact, "display_name" | "key" | "facts">,
  needle: string,
): SearchMatch | null {
  if (
    needle === "" ||
    artifact.display_name.toLowerCase().includes(needle) ||
    artifact.key.name.toLowerCase().includes(needle)
  ) {
    return BY_NAME;
  }
  const command = commandMatching(artifact.facts?.commands ?? [], needle);
  return command === null ? null : { by: "command", command };
}

/**
 * The command `needle` names, case aside: one that is exactly it, else the
 * first (by name, as the facts are sorted) that starts with it; null for
 * none. As the facts spell it, for the row's hint.
 */
export function commandMatching(commands: readonly CommandFact[], needle: string): string | null {
  if (needle === "") return null;
  let prefix: string | null = null;
  for (const { name } of commands) {
    const lower = name.toLowerCase();
    if (lower === needle) return name;
    if (prefix === null && lower.startsWith(needle)) prefix = name;
  }
  return prefix;
}
