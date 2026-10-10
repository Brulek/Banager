import { twinsByArtifact, type Twin } from "./commands";
import type { InstalledArtifact } from "./types";
import { artifactKeyId } from "../store/ui";

/**
 * Whether the list in view can say anything about which copy of a command
 * runs (`ArtifactFacts.commands`), for the 「显示」 choices that go by it:
 * 「装了不止一份」 needs the commands' names (`twinsByArtifact`), and
 * 「终端里找不到」 their verdicts (`hasCommandNotOnPath`).
 *
 * - `previewing`: the first check's list, shown while it is still checking
 *   for updates (`useInstalledSnapshot`'s `preview`): commands are judged
 *   only when the round ends (crates/banager-core/src/session/refresh.rs),
 *   so nothing is known of them yet.
 * - `unjudged`: the check ended without them -- reading the folders ran
 *   past its budget, or an earlier read was still running (no names
 *   anywhere), or the login shell's `PATH` was not restored (names, but no
 *   verdict anywhere; `commands::judge`).
 *
 * Either way an empty list would say 「没有发现…」 as if it had been
 * looked for; it says this instead. No artifact at all is `known`: there is
 * nothing to look for.
 */
export type CommandsKnown = "known" | "previewing" | "unjudged";

export function commandsKnown(
  artifacts: readonly Pick<InstalledArtifact, "facts">[],
  preview: boolean,
  need: "names" | "verdicts",
): CommandsKnown {
  if (artifacts.length === 0) return "known";
  if (preview) return "previewing";
  const known = artifacts.some(({ facts }) =>
    need === "names" ? facts.commands.length > 0 : facts.commands.some(({ state }) => state !== null),
  );
  return known ? "known" : "unjudged";
}

/** What an empty list says in place of 「没有发现…」, by why. */
export const COMMANDS_UNKNOWN_KEYS: Record<Exclude<CommandsKnown, "known">, string> = {
  previewing: "commandsKnown.previewing",
  unjudged: "commandsKnown.unjudged",
};

/**
 * How many tools a round that judged the commands still said nothing about
 * for at least one of theirs (`CommandFact.state: null` or
 * `commands_unavailable`, including dropped claims): a folder Terminal
 * looks in that could not be read comes first or might hold it, its link
 * was replaced, where its command was put is not known, or it is a
 * keg-only formula linked by hand that Terminal does not find
 * (crates/banager-core/src/commands.rs, `judge`). `commandsKnown` is
 * `known` for verdicts once any one tool has one, so 「终端都能找到…」 is
 * said of the tools checked only, with how many these are.
 *
 * Not counted: a Homebrew dependency, whose commands are never judged
 * (`judged` there: nobody typed its name to install it -- the Installed
 * page folds it away as a component); a tool with a command Terminal
 * cannot find, already counted as one (`hasCommandNotOnPath`); and a
 * formula Homebrew has not linked (`unlinked`), whose commands, named from
 * its keg, have no verdict because nothing puts them where Terminal looks
 * -- its details say so, not that it could not be checked (r36 V5), and
 * Check Tool Setup names it where it is no copy of another
 * (`toolsNotLinked`). Read from the commands as the window has them;
 * nothing is asked for.
 */
export function toolsNotJudged(artifacts: Pick<InstalledArtifact, "facts" | "reason">[]): number {
  return artifacts.filter(
    ({ facts, reason }) =>
      reason !== "Dependency" &&
      !(facts.unlinked && !facts.commands_unavailable) &&
      (facts.commands_unavailable || facts.commands.some(({ state }) => state === null)) &&
      !facts.commands.some(({ state }) => typeof state === "object" && state !== null && "NotOnPath" in state),
  ).length;
}

/**
 * The tools Homebrew has not linked (`unlinked`) whose commands, named
 * from their keg, Terminal does not find -- at least one with no verdict
 * -- and that are no copy of another tool (`twins`, `twinsByArtifact`):
 * `brew unlink ollama`, or Ollama.app's own CLI in the way of `brew
 * install ollama`. Their details say Homebrew didn't link them
 * (`CommandsGroup`); Check Tool Setup names them, and never says 「终端都能
 * 找到已安装的工具」 beside them, nor does the copied text leave them out
 * (q1b skeptic 2). One with another copy is among those installed more
 * than once, beside the copy Terminal runs, as any copy Terminal does not
 * run is; a dependency, whose commands are never judged, is not counted.
 * Said only where the round judged the commands (`commandsKnown`).
 */
export function toolsNotLinked(
  artifacts: readonly InstalledArtifact[],
  twins: ReadonlyMap<string, Twin[]> = twinsByArtifact(artifacts),
): InstalledArtifact[] {
  return artifacts.filter(
    (artifact) =>
      artifact.facts.unlinked &&
      artifact.reason !== "Dependency" &&
      artifact.facts.commands.some(({ state }) => state === null) &&
      !twins.has(artifactKeyId(artifact.key)),
  );
}

/**
 * How many tools' commands could not all be listed: a link in Homebrew's
 * or npm's `bin` that leads into a protected place, or could not be
 * followed, where its own first step goes into the tool's folder
 * (`commands_unavailable`; crates/banager-core/src/commands.rs, `linked`).
 * Their names may be missing, so 「没有装了不止一份的工具」 is said of the
 * tools whose commands were listed only. Dependencies count too: a tool
 * installed twice may be one.
 */
export function toolsNamesIncomplete(artifacts: Pick<InstalledArtifact, "facts">[]): number {
  return artifacts.filter(({ facts }) => facts.commands_unavailable).length;
}
