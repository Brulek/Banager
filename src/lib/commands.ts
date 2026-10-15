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
 * What typing the copies' shared command runs, from `artifact`'s side:
 * this copy (`runs`); another copy, `by` (`unused`); or nothing to say --
 * no twins, the commands disagree, or the one that runs is no copy.
 * Said from `ArtifactFacts.commands` alone, as the 「在终端里输入时」 group
 * judges them (against the `PATH` read when the app opened). The
 * inspector's advice and the Updates page's 「终端用另一份」
 * (src/components/TwinAdvice.tsx) go by it. A formula Homebrew has not
 * linked (`unlinked`) has no verdict of its own for a command named from
 * its keg; where another copy's command of that name runs, typing it runs
 * that copy, and so this one is `unused` -- the `gemini-cli` formula
 * `brew install` could not link over npm's `gemini` (q1b skeptic 5).
 */
export type TwinVerdict =
  | { kind: "runs"; command: string; others: Twin[] }
  | { kind: "unused"; command: string; by: InstalledArtifact }
  | null;

export function twinVerdict(artifact: InstalledArtifact, twins: readonly Twin[] | undefined): TwinVerdict {
  if (twins === undefined || twins.length === 0) return null;
  const shared = [...new Set(twins.flatMap((twin) => twin.commands))];
  const states = shared.map((name) => ownOrRunning(artifact, twins, name));
  const state: CommandState | null = states[0] ?? null;
  if (state === null || !states.every((other) => other !== null && stateId(other) === stateId(state))) return null;
  const command = twins[0].commands[0];
  if (state === "Runs") return { kind: "runs", command, others: [...twins] };
  if ("ShadowedBy" in state && state.ShadowedBy.by !== null) {
    const by = state.ShadowedBy.by;
    const twin = twins.find((other) => artifactKeyId(other.artifact.key) === artifactKeyId(by));
    if (twin !== undefined) return { kind: "unused", command, by: twin.artifact };
  }
  return null;
}

/**
 * `artifact`'s verdict for the command `name` -- or, for one with none of
 * a formula Homebrew has not linked, `ShadowedBy` the copy among `twins`
 * whose command of that name runs (typing it runs that one), or null.
 */
function ownOrRunning(artifact: InstalledArtifact, twins: readonly Twin[], name: string): CommandState | null {
  const own = artifact.facts.commands.find((fact) => fact.name === name)?.state ?? null;
  if (own !== null || !artifact.facts.unlinked) return own;
  const running = twins.find(
    (twin) =>
      twin.commands.includes(name) &&
      twin.artifact.facts.commands.some((fact) => fact.name === name && fact.state === "Runs"),
  );
  return running === undefined ? null : { ShadowedBy: { by: running.artifact.key } };
}

/**
 * The copies Terminal does not run, by `artifactKeyId`: every artifact
 * whose `twinVerdict` is `unused` -- npm's `@openai/codex` where typing
 * `codex` runs Codex's own install. Updating one changes nothing the user
 * types, so Update all leaves its update unticked and no count of updates
 * counts it (decision U4, `countedUpdatesOf` in src/lib/updateState.ts);
 * its row still offers it, with 「终端用另一份」. `twins` is
 * `twinsByArtifact(artifacts)`, where the caller has it already.
 */
export function unusedCopies(
  artifacts: readonly InstalledArtifact[],
  twins: ReadonlyMap<string, Twin[]> = twinsByArtifact(artifacts),
): Set<string> {
  const unused = new Set<string>();
  for (const artifact of artifacts) {
    const id = artifactKeyId(artifact.key);
    if (twinVerdict(artifact, twins.get(id))?.kind === "unused") unused.add(id);
  }
  return unused;
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
 * The commands of `artifact` that its command group says something about:
 * what typing each of them runs is known (`state` not null).
 */
export function judgedCommands(artifact: InstalledArtifact): Set<string> {
  return new Set(artifact.facts.commands.filter((command) => command.state !== null).map((command) => command.name));
}

/**
 * The commands a tool shares with its other copies (`twins`) whose verdict
 * here is about those copies: typing it runs this copy, or one of them --
 * what the inspector's 「在终端里输入时」 says as 「运行的是这一份」 /
 * 「运行的是npm装的那一份」, under a row that says 「装了两份」. Each command on
 * its own: Grok Build's `grok` that runs Homebrew's copy is one, beside an
 * `agent` that runs Cursor's (r36 V3). Not a command with no verdict, one
 * not on `PATH`, or one another program comes first for.
 */
function sharedCommandsSaid(artifact: InstalledArtifact, twins: readonly Twin[]): string[] {
  const copies = new Set(twins.map((twin) => artifactKeyId(twin.artifact.key)));
  const shared = new Set(twins.flatMap((twin) => twin.commands));
  return artifact.facts.commands
    .filter(({ name, state }) => {
      if (!shared.has(name) || state === null) return false;
      if (state === "Runs") return true;
      return "ShadowedBy" in state && state.ShadowedBy.by !== null && copies.has(artifactKeyId(state.ShadowedBy.by));
    })
    .map(({ name }) => name);
}

/**
 * The commands the rows of each source say what typing them runs, by the
 * source's instance id: those a tool of the source shares with another
 * copy of itself where typing it runs this copy or another of them
 * (`sharedCommandsSaid`), each command on its own -- the 「装了两份」 word,
 * the inspector's 「在终端里输入时」 line for `grok`, 「运行的是Homebrew装的
 * 那一份」, the 「装了两份」 word's ⓘ where every shared command has that one verdict
 * (`twinVerdict`), and the Updates page's 「终端用另一份」. What the lists'
 * notices of each source are held to (`withoutJudgedPathNotices`), as the
 * inspector's are to its command group (`judgedCommands`): never
 * 「无法确认它是不是另一份Grok Build」 over rows that say it is installed
 * twice, though Cursor's `agent` comes first for Grok Build's other
 * command (r36 V3). A command whose rows say nothing of it -- typing
 * `claude` runs an npm program that is no copy of Claude Code, or the
 * launcher is not on `PATH` (the 「终端里找不到」 word, which agrees with
 * its notice) -- keeps its notice, and with it the notice's Show (W2-9).
 * `twins` is `twinsByArtifact(artifacts)`, where the caller has it
 * already.
 */
export function commandsSaidOnRows(
  artifacts: readonly InstalledArtifact[],
  twins: ReadonlyMap<string, Twin[]> = twinsByArtifact(artifacts),
): Map<string, Set<string>> {
  const bySource = new Map<string, Set<string>>();
  for (const artifact of artifacts) {
    const copies = twins.get(artifactKeyId(artifact.key));
    if (copies === undefined) continue;
    const names = sharedCommandsSaid(artifact, copies);
    if (names.length === 0) continue;
    const id = artifact.key.instance_id;
    const said = bySource.get(id) ?? new Set<string>();
    for (const name of names) said.add(name);
    bySource.set(id, said);
  }
  return bySource;
}

/**
 * `notices` less the source's sentence about the launcher on `PATH` when
 * what is on screen with it already says what typing that command runs
 * (`judged`): in the inspector, the tool's command group
 * (`judgedCommands`); where the Overview, the Updates page and the
 * Installed list give the source's notices, the rows of its tools
 * (`commandsSaidOnRows`). They know more -- which copy, from which source
 * -- and the notice, which cannot tell whether npm's `claude` is Claude
 * Code, would contradict them ("could not confirm it is Claude Code" over
 * "runs the copy npm installed", and over rows that say "Installed
 * twice": r24 W8). A launcher with no verdict keeps its notice: then the
 * notice is all there is.
 */
export function withoutJudgedPathNotices(
  notices: SourceNoticeSpec[],
  judged: ReadonlySet<string> | undefined,
): SourceNoticeSpec[] {
  if (judged === undefined || judged.size === 0) return notices;
  return notices.filter(
    (notice) =>
      !(PATH_NOTICE_TITLES.has(notice.titleKey) && judged.has(String(notice.values?.command ?? ""))),
  );
}
