import { formatBytes } from "./format";
import type { InstalledArtifact, Measured, Sizes } from "./types";

/** Whatever `useTranslation()`'s `t` needs here; the same convention as `Translate` in src/lib/sources.ts. */
export type Translate = (key: string, options?: Record<string, string | number>) => string;

/**
 * What the details say of an installed thing's size: still being measured,
 * or measured -- with, for a Homebrew formula, its old versions.
 */
export type SizeView =
  | { kind: "measuring" }
  | { kind: "measured"; measured: Measured; oldVersions: Measured | null };

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
  return { kind: "measured", measured: entry.measured, oldVersions: entry.old_versions };
}

/**
 * 「约312 MB」, "About 312 MB": every measured number says it is rough. A
 * round the budget cut short says 「至少约…」, "At least about …"; one that
 * could not read part of it says so after the number.
 */
export function sizeText(t: Translate, measured: Measured): string {
  const size = formatBytes(measured.bytes);
  if (measured.at_least) return t("sizes.atLeast", { size });
  if (measured.partial) return t("sizes.partial", { size });
  return t("sizes.about", { size });
}

/** 「旧版本约1.2 GB」: a Homebrew formula's other kegs, together; "at least" when part of them was not measured. */
export function oldVersionsText(t: Translate, measured: Measured): string {
  const size = formatBytes(measured.bytes);
  return measured.at_least || measured.partial
    ? t("sizes.oldVersionsAtLeast", { size })
    : t("sizes.oldVersions", { size });
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
