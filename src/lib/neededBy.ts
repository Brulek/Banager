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
import type { InstalledArtifact, Warning } from "./types";

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
 * Homebrew's dependents: 「npm及其4个工具」, 「Ollama及其2个模型」 -- the
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
 * What a source lists of its own program, which `needed_by` does not count
 * (`comes_with_program` in crates/banager-core/src/needed_by.rs), spelled
 * as PyPI normalizes a name. Held to Rust's by
 * crates/banager-core/src/needed_by_tables.json (neededBy.test.ts).
 */
export const COMES_WITH_PROGRAM: Readonly<Record<string, readonly string[]>> = {
  npm: ["npm", "corepack"],
  pip: ["pip", "setuptools", "wheel"],
};

/**
 * Whether `tool` is one `needed_by` counts among its source's tools
 * (`counts` in crates/banager-core/src/needed_by.rs): not what the
 * source's own program comes with, nor what pip installed for another
 * package.
 */
export function countsAsTool(adapterId: string, tool: InstalledArtifact): boolean {
  const name = tool.key.name.toLowerCase().replace(/[_.]/g, "-");
  return tool.reason !== "Dependency" && !(COMES_WITH_PROGRAM[adapterId] ?? []).includes(name);
}

/**
 * Sources whose tools keep their own program or environment, which go on
 * running without it: only updating and uninstalling them goes with the
 * package. Held to Rust's `needed_by` tables by
 * crates/banager-core/src/needed_by_tables.json (neededBy.test.ts).
 */
export const MANAGES_ONLY: ReadonlySet<string> = new Set(["pipx", "uv", "cargo"]);

/**
 * What else a sentence about `entries` says after what to uninstall first:
 * that the tools of a source Banager cannot uninstall from (pip) are
 * uninstalled in Terminal, and, for pipx, uv and Cargo themselves, that
 * their tools are updated and uninstalled with the package -- which is all
 * they lose with it.
 */
function afterwards(
  t: Translate,
  entries: readonly NeededBy[],
  sourceOf: (instanceId: string) => string,
  canUninstallHere: (instanceId: string) => boolean,
): string[] {
  const inTerminal = entries.filter((entry) => !canUninstallHere(entry.instance_id));
  const managed = entries.filter(
    (entry) => entry.program && MANAGES_ONLY.has(adapterIdOf(entry.instance_id)) && canUninstallHere(entry.instance_id),
  );
  const names = (list: readonly NeededBy[]) => namesInSentence(t, list.map((entry) => sourceOf(entry.instance_id)));
  return [
    ...(managed.length > 0 ? [t("runtimeGuard.managedBy", { sources: names(managed) })] : []),
    ...(inTerminal.length > 0 ? [t("runtimeGuard.inTerminal", { sources: names(inTerminal) })] : []),
  ];
}

/** Sentences one after another, as the language spaces them. */
function sentences(t: Translate, parts: readonly string[]): string {
  return parts.reduce((first, then) => t("runtimeGuard.then", { first, then }));
}

const anywhere = () => true;

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
  canUninstallHere: (instanceId: string) => boolean = anywhere,
): string {
  const tools = namesInSentence(t, [
    ...(withDependents ? [t("runtimeGuard.dependentsAbove")] : []),
    ...entries.map((entry) => toRemove(t, entry, sourceOf(entry.instance_id))),
  ]);
  return sentences(t, [
    t("runtimeGuard.blocks", { name, tools }),
    ...afterwards(t, entries, sourceOf, canUninstallHere),
  ]);
}

/**
 * Why a batch leaves a package out that sources run on: 「还有软件要用到它：
 * npm及其4个工具。要卸载它，请先卸载npm装的4个工具。」 -- what still needs
 * it, Homebrew's dependents (`dependents`, as `brew uses` names them)
 * first, then what to uninstall before it. `goFirst`: every one of those
 * tools is uninstalled in this same batch (`toolsGoFirst`), so the package
 * can go in the next.
 */
export function neededByReason(
  t: Translate,
  entries: readonly NeededBy[],
  dependents: readonly string[],
  sourceOf: (instanceId: string) => string,
  canUninstallHere: (instanceId: string) => boolean = anywhere,
  goFirst = false,
): string {
  const names = namesInSentence(t, [
    ...dependents,
    ...entries.map((entry) => neededByItem(t, entry, sourceOf(entry.instance_id))),
  ]);
  const tools = namesInSentence(t, [
    ...dependents,
    ...entries.map((entry) => toRemove(t, entry, sourceOf(entry.instance_id))),
  ]);
  return sentences(t, [
    t("runtimeGuard.reason", { names, tools }),
    ...(goFirst ? [t("runtimeGuard.goFirst")] : afterwards(t, entries, sourceOf, canUninstallHere)),
  ]);
}
