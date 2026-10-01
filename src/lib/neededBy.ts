/**
 * The other sources an uninstall preview says run on a Homebrew package
 * (`Warning.NeededBySource`, found by crates/banager-core/src/needed_by.rs):
 * npm and its tools on `node@22`, pipx's tools on `python@3.13`, Ollama
 * and its models on `ollama`. Homebrew's own `brew uses` cannot know them,
 * so the preview adds them, and the uninstall confirmation lists them with
 * what Homebrew names under 「依赖此工具的软件」 and offers no Uninstall --
 * as for Homebrew's dependents -- and a batch leaves the package out with
 * the same words. Pure: `t` and each source's name are parameters.
 */
import { adapterIdOf, namesInSentence } from "./sources";
import type { Warning } from "./types";

/** One source that runs on the package, as the preview says it. */
export interface NeededBy {
  instance_id: string;
  /** The source itself runs on it: every tool it lists goes with it. */
  program: boolean;
  /** How many of its tools need it. */
  tools: number;
}

type Translate = (key: string, options?: Record<string, unknown>) => string;

/** The sources a plan's warnings say run on its package, in their order. */
export function neededBy(warnings: readonly Warning[]): NeededBy[] {
  return warnings.flatMap((warning) =>
    typeof warning !== "string" && "NeededBySource" in warning ? [warning.NeededBySource] : [],
  );
}

/** Whether a source's rows are models (Ollama's), not tools. */
function models(entry: NeededBy): boolean {
  return adapterIdOf(entry.instance_id) === "ollama";
}

/**
 * How the list of what still needs the package names one source, beside
 * Homebrew's dependents: 「npm和它的4个工具」, 「Ollama和它的2个模型」 -- the
 * source with every tool it lists -- or 「pipx装的2个工具」, those of its
 * tools whose environment's Python it is. `source` is its name, as the
 * sidebar has it (`instanceLabels`).
 */
export function neededByItem(t: Translate, entry: NeededBy, source: string): string {
  if (!entry.program) return t("runtimeGuard.environments", { source, count: entry.tools });
  return t(models(entry) ? "runtimeGuard.programModels" : "runtimeGuard.program", { source, count: entry.tools });
}

/**
 * What to uninstall first, as a sentence says it: 「npm装的4个工具」,
 * 「Ollama的2个模型」 -- the tools, not the source, whose own program goes
 * with the package or stays without anything to run.
 */
function toRemove(t: Translate, entry: NeededBy, source: string): string {
  return t(models(entry) ? "runtimeGuard.removeModels" : "runtimeGuard.remove", { source, count: entry.tools });
}

/**
 * Why the confirmation of uninstalling `name` offers no Uninstall, said
 * under its list where any source is in it: 「要卸载“node@22”，请先卸载npm装的4个
 * 工具。」, and with Homebrew's dependents listed too, 「…请先卸载上面的Homebrew
 * 软件和npm装的4个工具。」.
 */
export function neededBySentence(
  t: Translate,
  name: string,
  entries: readonly NeededBy[],
  sourceOf: (instanceId: string) => string,
  withDependents: boolean,
): string {
  const tools = namesInSentence(t, [
    ...(withDependents ? [t("runtimeGuard.dependentsAbove")] : []),
    ...entries.map((entry) => toRemove(t, entry, sourceOf(entry.instance_id))),
  ]);
  return t("runtimeGuard.blocks", { name, tools });
}

/**
 * Why a batch leaves a package out that sources run on: 「还有软件要用到它：
 * npm和它的4个工具。要卸载它，请先卸载npm装的4个工具。」 -- what still needs
 * it, Homebrew's dependents (`dependents`, as `brew uses` names them)
 * first, then what to uninstall before it.
 */
export function neededByReason(
  t: Translate,
  entries: readonly NeededBy[],
  dependents: readonly string[],
  sourceOf: (instanceId: string) => string,
): string {
  const names = namesInSentence(t, [
    ...dependents,
    ...entries.map((entry) => neededByItem(t, entry, sourceOf(entry.instance_id))),
  ]);
  const tools = namesInSentence(t, [
    ...dependents,
    ...entries.map((entry) => toRemove(t, entry, sourceOf(entry.instance_id))),
  ]);
  return t("runtimeGuard.reason", { names, tools });
}
