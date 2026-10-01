import type { InstalledArtifact, OpRequest, Plan, Warning } from "../lib/types";
import { IDS, type World } from "./mockData";

/**
 * The preview's stand-in for the other half of what `Session::issue_plan`
 * adds to a Homebrew uninstall's preview (crates/banager-core/src/session/
 * needed_by.rs): the sources that run on the package, which `brew uses`
 * does not name. Rust finds them by following links; the pretend Mac's are
 * laid out as the author's: npm's `/opt/homebrew/bin/npm` and the `node`
 * on `PATH` lead into `node@22`'s keg (linked by hand), pip's
 * `/opt/homebrew/bin/python3` into `python@3.13`'s, Homebrew's `pipx` and
 * `ollama` are what those sources run, and every pipx venv's `bin/python`
 * leads into `python@3.13`'s keg. uv and its tools' Python are its own.
 * With `?state=notices` npm is the one from nodejs.org, in `/usr/local`,
 * and runs on no Homebrew package.
 */
const RUNS_ON: Record<string, ReadonlyArray<{ instance: string; program: boolean }>> = {
  "node@22": [{ instance: IDS.npm, program: true }],
  ollama: [{ instance: IDS.ollama, program: true }],
  pipx: [{ instance: IDS.pipx, program: true }],
  "python@3.13": [
    { instance: IDS.pip, program: true },
    { instance: IDS.pipx, program: false },
  ],
};

/** What a source lists of its own program, as `needed_by::comes_with_program` has it. */
const COMES_WITH_PROGRAM: Record<string, readonly string[]> = {
  npm: ["npm", "corepack"],
  pip: ["pip", "setuptools", "wheel"],
};

/** Whether `tool`, one of `adapterId`'s rows, counts (`needed_by::counts`). */
function counts(adapterId: string, tool: InstalledArtifact): boolean {
  const name = tool.key.name.toLowerCase().replace(/[_.]/g, "-");
  return tool.reason !== "Dependency" && !(COMES_WITH_PROGRAM[adapterId] ?? []).includes(name);
}

/**
 * `plan`, with a `NeededBySource` for each source of `world` that runs on
 * the Homebrew formula `request` uninstalls, and how many of its tools
 * need it -- for a source with any.
 */
export function withMockNeededBy(plan: Plan, world: World, request: OpRequest): Plan {
  if (request.kind !== "Uninstall" || request.artifact_kind !== "Formula" || request.instance_id !== IDS.brew) {
    return plan;
  }
  const needed: Warning[] = (RUNS_ON[request.name] ?? []).flatMap(({ instance: id, program }) => {
    const instance = world.instances.find((candidate) => candidate.id === id);
    if (instance === undefined) return [];
    const tools = world.artifacts.filter(
      (artifact) =>
        artifact.key.instance_id === id &&
        counts(instance.adapter_id, artifact) &&
        // A pipx tool's own environment, for one only its venv runs on.
        (program || artifact.path !== null),
    );
    return tools.length === 0 ? [] : [{ NeededBySource: { instance_id: id, program, tools: tools.length } }];
  });
  return needed.length === 0 ? plan : { ...plan, warnings: [...plan.warnings, ...needed] };
}

/** Whether a preview named a source that runs on its package: `Session::submit` refuses it. */
export function namesASource(plan: Plan): boolean {
  return plan.warnings.some((warning) => typeof warning !== "string" && "NeededBySource" in warning);
}
