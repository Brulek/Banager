import { formatBytes } from "./format";
import type { InstalledArtifact, Measured, Sizes } from "./types";
import { artifactKeyId } from "../store/ui";

/** Whatever `useTranslation()`'s `t` needs here; the same convention as `Translate` in src/lib/sources.ts. */
export type Translate = (key: string, options?: Record<string, string | number>) => string;

/**
 * What the details say of an installed thing's size: still being measured,
 * or measured -- with, for a Homebrew formula, its other versions' kegs
 * together (`otherVersionsFact`, beside the versions they are).
 */
export type SizeView =
  | { kind: "measuring" }
  | { kind: "measured"; measured: Measured; otherVersions: Measured | null };

/**
 * `artifact`'s size as the newest round of measuring has it (`useSizes`),
 * or null when there is none to show: nothing is measured for its kind,
 * its folder is not there, it is somewhere never looked into, or the sizes
 * have not been asked for yet. A size measured at another version than the
 * one listed belongs to the round before an update: the round now running
 * measures this one, so it is "measuring" until then.
 */
export function sizeViewOf(sizes: Sizes | undefined, artifact: InstalledArtifact): SizeView | null {
  const entry = sizes?.artifacts.find(
    (size) =>
      size.key.instance_id === artifact.key.instance_id &&
      size.key.kind === artifact.key.kind &&
      size.key.name === artifact.key.name,
  );
  if (entry === undefined) return null;
  if (entry.measured === null || entry.version !== artifact.version) return { kind: "measuring" };
  return { kind: "measured", measured: entry.measured, otherVersions: entry.old_versions };
}

/**
 * Whether a measured size is worth a number: not a measured 0 -- npm's
 * `corepack` under Homebrew's node is only links into the node keg, which
 * take no blocks -- since 「约0 B」 reads as something broken. Nothing is
 * said instead (the row, or the number, is left out), which is quieter
 * than a phrase and claims nothing. A 0 is still a measurement: By Size
 * (`sizeOrderOf`) and the totals (src/lib/sizeTotals.ts) count it.
 */
export function saysSize(measured: Measured): boolean {
  return measured.bytes > 0;
}

/**
 * 「约312 MB」, "About 312 MB": every measured number says it is rough. A
 * round the budget cut short says 「312 MB以上」, "312 MB or more" -- one
 * hedge, not two; one that could not read part of it says so after the
 * number.
 */
export function sizeText(t: Translate, measured: Measured): string {
  const size = formatBytes(measured.bytes);
  if (measured.at_least) return t("sizes.atLeast", { size });
  if (measured.partial) return t("sizes.partial", { size });
  return t("sizes.about", { size });
}

/**
 * What a measured size of `artifact` leaves out, or counts that is not
 * its alone, as the i18n key of the sentence an ⓘ after the number says
 * it in; null when the number needs no such word:
 *
 * - a Cargo crate and a tool with its own installer are their program
 *   files only (size.rs `roots_of`): not what they download or cache --
 *   rustup's toolchains, Cargo's registry;
 * - a uv tool's environment shares files with uv's cache (APFS clones),
 *   which count in full.
 */
export function sizeNoteOf(artifact: InstalledArtifact): "sizes.programOnly" | "sizes.sharedWithCache" | null {
  const adapterId = artifact.key.instance_id.split(":")[0];
  if (adapterId === "cargo" || adapterId.startsWith("standalone-")) return "sizes.programOnly";
  if (adapterId === "uv") return "sizes.sharedWithCache";
  return null;
}

/**
 * 「约120 MB」 under one other version, 「共约1.2 GB」 under several: what a
 * Homebrew formula's other kegs take together, said under the versions
 * they are. "At least" when part of them was not measured.
 */
export function otherVersionsSizeText(t: Translate, measured: Measured, count: number): string {
  const size = formatBytes(measured.bytes);
  const short = measured.at_least || measured.partial;
  if (count > 1) return t(short ? "sizes.otherVersionsTotalAtLeast" : "sizes.otherVersionsTotal", { size });
  return t(short ? "sizes.atLeast" : "sizes.about", { size });
}

/**
 * 「Ollama模型共约41 GB」: the models of the Ollama `instanceId`, measured
 * once as the folder they are in -- never their own sizes added up, which
 * would count a layer two models share twice. Null while it is measured,
 * and when there is no such line (the models are not on this Mac, or not
 * in the folder Banager looks in).
 */
export function modelsTotalText(t: Translate, sizes: Sizes | undefined, instanceId: string): string | null {
  const measured = sizes?.models.find((models) => models.instance_id === instanceId)?.measured ?? null;
  if (measured === null) return null;
  const size = formatBytes(measured.bytes);
  return measured.at_least || measured.partial
    ? t("sizes.ollamaModelsAtLeast", { size })
    : t("sizes.ollamaModels", { size });
}

/**
 * What the Installed page's "By Size" goes by, per row (`artifactKeyId`):
 * the bytes it takes -- a measured size of the version listed, or the size
 * its source reports (an Ollama model's) -- for every row that has one.
 */
export function sizeOrderOf(sizes: Sizes | undefined, artifacts: InstalledArtifact[]): Map<string, number> {
  const measured = new Map((sizes?.artifacts ?? []).map((size) => [artifactKeyId(size.key), size]));
  const order = new Map<string, number>();
  for (const artifact of artifacts) {
    const id = artifactKeyId(artifact.key);
    if (artifact.size_bytes !== null) {
      order.set(id, artifact.size_bytes);
      continue;
    }
    // As `sizeViewOf` reads it: of the version listed, and measured.
    const size = measured.get(id);
    if (size?.measured != null && size.version === artifact.version) order.set(id, size.measured.bytes);
  }
  return order;
}

/**
 * "By Size": the larger first; a row with no size to show -- none, or still
 * measuring -- after every row with one, so that 0 means "same size, or
 * neither has one", for the caller to order by name.
 */
export function compareBySize(order: Map<string, number>, a: InstalledArtifact, b: InstalledArtifact): number {
  const left = order.get(artifactKeyId(a.key));
  const right = order.get(artifactKeyId(b.key));
  if (left === right) return 0;
  if (left === undefined) return 1;
  if (right === undefined) return -1;
  return right - left;
}

/**
 * What a row shows in its version's place while the list is sorted by
 * size (`InstalledPage`'s "By Size"), as Finder's Size column does: the
 * size the order goes by -- 「约4.7 GB」 -- or 「正在计算…」 while it is
 * measured, or 「—」 for a row with none, and for a measured 0
 * (`saysSize`), which still sorts as the smallest measured. `muted` says
 * which are drawn in the secondary colour.
 */
export function sizeCellOf(
  t: Translate,
  sizes: Sizes | undefined,
  artifact: InstalledArtifact,
): { text: string; muted: boolean } {
  if (artifact.size_bytes !== null) return { text: t("sizes.about", { size: formatBytes(artifact.size_bytes) }), muted: false };
  const view = sizeViewOf(sizes, artifact);
  if (view === null) return { text: "—", muted: true };
  if (view.kind === "measuring") return { text: t("sizes.measuring"), muted: true };
  if (!saysSize(view.measured)) return { text: "—", muted: true };
  return { text: sizeText(t, view.measured), muted: false };
}
