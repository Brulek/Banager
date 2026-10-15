import { countsAsTool } from "../lib/neededBy";
import type { OpRequest, Plan, Warning } from "../lib/types";
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

/**
 * Whether a formula is named for a program a source is found as on `PATH`,
 * or runs on (`needed_by::unseen_sources`, judged by `Look::could_be`):
 * npm and its `node`, a Python, `pipx`, `uv`, `cargo`, `ollama`. The
 * pretend Mac keeps none of these under another name.
 */
function runtimeForASource(name: string): boolean {
  return /^(npm|node|python|pipx|uv|cargo|ollama)(@.*)?$/.test(name);
}

/**
 * `plan`, with a `NeededBySource` for each source of `world` that runs on
 * the Homebrew formula `request` uninstalls, and how many of its tools
 * need it -- for a source with any. With `pathRead` false (`?path=default`:
 * the login shell's `PATH` never read, so a source that runs on it may not
 * have been found), a formula a source could run on also says the look did
 * not finish (`DependentsUnknown`, `needed_by::needed_by_on_path`).
 */
export function withMockNeededBy(plan: Plan, world: World, request: OpRequest, pathRead = true): Plan {
  if (request.kind !== "Uninstall" || request.artifact_kind !== "Formula" || request.instance_id !== IDS.brew) {
    return plan;
  }
  const needed: Warning[] = (RUNS_ON[request.name] ?? []).flatMap(({ instance: id, program }) => {
    const instance = world.instances.find((candidate) => candidate.id === id);
    if (instance === undefined) return [];
    const tools = world.artifacts.filter(
      (artifact) =>
        artifact.key.instance_id === id &&
        countsAsTool(instance.adapter_id, artifact) &&
        // A pipx tool's own environment, for one only its venv runs on.
        (program || artifact.path !== null),
    );
    return tools.length === 0 ? [] : [{ NeededBySource: { instance_id: id, program, tools: tools.length } }];
  });
  // After what it found, as `Session::with_needed_by` adds it.
  if (!pathRead && runtimeForASource(request.name) && !plan.warnings.includes("DependentsUnknown")) {
    needed.push("DependentsUnknown");
  }
  return needed.length === 0 ? plan : { ...plan, warnings: [...plan.warnings, ...needed] };
}

/** Whether a preview named a source that runs on its package: `Session::submit` refuses it. */
export function namesASource(plan: Plan): boolean {
  return plan.warnings.some((warning) => typeof warning !== "string" && "NeededBySource" in warning);
}
