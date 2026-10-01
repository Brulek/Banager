import { formatBytes } from "./format";
import type { Translate } from "./sizes";
import type { InstalledArtifact, Measured, Sizes, Snapshot } from "./types";
import { artifactKeyId } from "../store/ui";

/**
 * What several tools take together, as the Installed page says it: under
 * each source's heading while the list is sorted by source (「86 · 约4.1
 * GB」), and in the toolbar after the count (「55个工具 · 共约9.8 GB」).
 *
 * The numbers are the ones Rust adds up (`Sizes.total`, `Sizes.sources`,
 * crates/banager-core/src/size.rs): a file with several hard links counts
 * once, which adding up each tool's own size would not. `atLeast` is true
 * whenever the number is short of what the tools in it take -- a part
 * could not be read, the budget ran out, or a tool in it has no size at
 * all (a pip package, a cask with no app, a model whose folder was not
 * measured) -- and the words then say 「至少约」. Never what could be freed:
 * nothing here offers to remove anything.
 */
export interface SizeTotal {
  bytes: number;
  atLeast: boolean;
}

/**
 * Whether `artifact`'s size is in the total: its own, measured at the
 * version listed, or -- for an Ollama model, which keeps the size Ollama
 * reports -- its Ollama's models folder, measured.
 */
function counted(artifact: InstalledArtifact, sizes: Sizes, measured: ReadonlyMap<string, Sizes["artifacts"][number]>) {
  if (artifact.key.kind === "Model") {
    return sizes.models.some((models) => models.instance_id === artifact.key.instance_id && models.measured !== null);
  }
  const size = measured.get(artifactKeyId(artifact.key));
  return size !== undefined && size.measured !== null && size.version === artifact.version;
}

function totalOf(measured: Measured | null | undefined, whole: boolean): SizeTotal | null {
  // Nothing measured, or nothing reached: no number rather than 「至少约0 KB」.
  if (measured == null || measured.bytes === 0) return null;
  return { bytes: measured.bytes, atLeast: measured.partial || measured.at_least || !whole };
}

/**
 * Each source's total, by instance id, and every source's together
 * (`all`) -- once the newest round of measuring is done, and only when it
 * measured the snapshot shown (its `round`): a total from before a check
 * would still count what the check found gone. Empty before that.
 */
export function sizeTotalsOf(
  sizes: Sizes | undefined,
  snapshot: Pick<Snapshot, "round" | "artifacts"> | undefined,
): { all: SizeTotal | null; bySource: Map<string, SizeTotal> } {
  const bySource = new Map<string, SizeTotal>();
  if (sizes === undefined || snapshot === undefined || !sizes.done || sizes.round !== snapshot.round) {
    return { all: null, bySource };
  }
  const measured = new Map(sizes.artifacts.map((size) => [artifactKeyId(size.key), size]));
  // The sources with a tool whose size is not in their total.
  const short = new Set<string>();
  for (const artifact of snapshot.artifacts) {
    if (!counted(artifact, sizes, measured)) short.add(artifact.key.instance_id);
  }
  for (const source of sizes.sources) {
    const total = totalOf(source.measured, !short.has(source.instance_id));
    if (total !== null) bySource.set(source.instance_id, total);
  }
  return { all: totalOf(sizes.total, short.size === 0), bySource };
}

/** A source's heading's: 「约4.1 GB」, 「至少约4.1 GB」. */
export function sourceTotalText(t: Translate, total: SizeTotal): string {
  const size = formatBytes(total.bytes);
  return total.atLeast ? t("sizes.sourceTotalAtLeast", { size }) : t("sizes.sourceTotal", { size });
}

/** The toolbar's, after the count: 「共约9.8 GB」, 「共至少约9.8 GB」. */
export function viewTotalText(t: Translate, total: SizeTotal): string {
  const size = formatBytes(total.bytes);
  return total.atLeast ? t("sizes.viewTotalAtLeast", { size }) : t("sizes.viewTotal", { size });
}
