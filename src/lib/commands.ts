import { artifactKeyId } from "../store/ui";
import type { SourceNoticeSpec } from "./sources";
import type { CommandFact, CommandState, InstalledArtifact } from "./types";

/**
 * What a tool's commands run when typed in Terminal
 * (`ArtifactFacts.commands`, worked out by `commands::judge` in
 * crates/banager-core/src/commands.rs), arranged for the Installed page:
 * one line per verdict, and the other copies of the same tool.
 */

/** A verdict as a string: equal for two commands that run the same thing. */
export function stateId(state: CommandState): string {
  if (state === "Runs") return "Runs";
  if ("ShadowedBy" in state) {
    const by = state.ShadowedBy.by;
    return `ShadowedBy:${by === null ? "" : artifactKeyId(by)}`;
  }
  return `NotOnPath:${state.NotOnPath.dir}`;
}

/** Commands with one verdict: what one line of the inspector says. */
export interface CommandGroup {
  /** In name order. */
  names: string[];
  state: CommandState;
}

/**
 * The commands Banager said something about, one group per verdict --
 * rustup's fourteen that all run this copy are one line, and a Homebrew
 * `rust` that comes first for `cargo` and `rustc` makes a second -- in the
 * name order of each group's first command. Commands with no verdict
 * (`state: null`) are left out.
 */
export function commandGroups(commands: readonly CommandFact[]): CommandGroup[] {
  const groups = new Map<string, CommandGroup>();
  for (const command of commands) {
    if (command.state === null) continue;
    const id = stateId(command.state);
    const group = groups.get(id);
    if (group === undefined) groups.set(id, { names: [command.name], state: command.state });
    else group.names.push(command.name);
  }
  return [...groups.values()];
}

/** Another copy of the same tool, and the commands it shares with this one. */
export interface Twin {
  artifact: InstalledArtifact;
  /** In name order. */
  commands: string[];
}

/**
 * For every artifact with another copy of the same tool -- an artifact of
 * the same `family` (the AI coding tools' table, `families.rs`) providing
 * a command of the same name: npm's `@anthropic-ai/claude-code` and Claude
 * Code's own install both put `claude` on the Mac -- those copies, by
 * `artifactKeyId`. Two artifacts that share a command name but no family
 * are two programs with one name, not two copies: Homebrew's `grok`, a
 * regular-expression tool, is not Grok Build. Built once per snapshot.
 */
export function twinsByArtifact(artifacts: readonly InstalledArtifact[]): Map<string, Twin[]> {
  const byFamily = new Map<string, InstalledArtifact[]>();
  for (const artifact of artifacts) {
    const family = artifact.facts.family;
    if (family === null || artifact.facts.commands.length === 0) continue;
    const members = byFamily.get(family) ?? [];
    members.push(artifact);
    byFamily.set(family, members);
  }
  const twins = new Map<string, Twin[]>();
  for (const members of byFamily.values()) {
    for (const artifact of members) {
      const names = new Set(artifact.facts.commands.map((command) => command.name));
      const id = artifactKeyId(artifact.key);
      const found: Twin[] = [];
      for (const other of members) {
        if (artifactKeyId(other.key) === id) continue;
        const shared = other.facts.commands.map((command) => command.name).filter((name) => names.has(name));
        if (shared.length > 0) found.push({ artifact: other, commands: shared });
      }
      if (found.length > 0) twins.set(id, found);
    }
  }
  return twins;
}

/**
 * The standalone source's own sentences about its launcher on `PATH`
 * (`sourceNoticesFor`: the `NotOnPath` and three `ShadowedBy*` notes),
 * by title. Each names the launcher as `values.command`.
 */
const PATH_NOTICE_TITLES = new Set([
  "sourceNotice.notOnPath.title",
  "sourceNotice.shadowedByHomebrew.title",
  "sourceNotice.shadowedByNpm.title",
  "sourceNotice.shadowedByOther.title",
]);

/**
 * `notices` less the source's sentence about the launcher on `PATH` when
 * `artifact`'s own command group already says what typing that command
 * runs. The group knows more -- which copy, from which source -- and the
 * notice, which cannot tell whether npm's `claude` is Claude Code, would
 * contradict it a few lines up ("could not confirm it is Claude Code"
 * under "runs the copy npm installed"). A launcher with no verdict keeps
 * its notice: then the notice is all there is.
 */
export function withoutJudgedPathNotices(notices: SourceNoticeSpec[], artifact: InstalledArtifact): SourceNoticeSpec[] {
  const judged = new Set(
    artifact.facts.commands.filter((command) => command.state !== null).map((command) => command.name),
  );
  if (judged.size === 0) return notices;
  return notices.filter(
    (notice) =>
      !(PATH_NOTICE_TITLES.has(notice.titleKey) && judged.has(String(notice.values?.command ?? ""))),
  );
}
