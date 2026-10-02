/**
 * How much an Ollama model's update downloads at most
 * (`UpdateCandidate.download_bytes`, from the two manifests the check
 * already has: crates/banager-core/src/adapters/ollama/parse.rs
 * `changed_blob_bytes`), said where the update is decided on: the model's
 * note in the update confirmation -- one tool, or Update All's list --
 * and its row's version column. An upper bound, as a file another model
 * shares is already on this Mac: 「最多约4.7 GB」, "up to about 4.7 GB".
 * Not known, it says what it said before: that the files that changed
 * are downloaded (`warnings.downloadsModelChanges`), and 「有新版本」.
 */
import { formatBytes } from "./format";

/** Whatever `useTranslation()`'s `t` needs here (`Translate` in src/lib/warnings.ts). */
type Translate = (key: string, options?: Record<string, unknown>) => string;

/**
 * The bytes worth a number, or null: not known (`null`, or left out by a
 * candidate from before the field), or 0 -- 「最多约0 B」 reads as
 * something broken (`saysSize` in src/lib/sizes.ts), and a model whose
 * changed files are all here already says what it always said.
 */
export function downloadBytesWorthSaying(bytes: number | null | undefined): number | null {
  return typeof bytes === "number" && Number.isFinite(bytes) && bytes > 0 ? bytes : null;
}

/**
 * The model's note in the update confirmation, with the most it
 * downloads: 「需要下载已更改的模型文件，最多约4.7 GB。」 -- or null where
 * that is not known, for `warnings.downloadsModelChanges` to stand.
 */
export function modelDownloadNote(t: Translate, bytes: number | null | undefined): string | null {
  const known = downloadBytesWorthSaying(bytes);
  return known === null ? null : t("modelDownload.note", { size: formatBytes(known) });
}

/**
 * A model's row's version column, with the most its update downloads:
 * 「有新版本 · 最多约4.7 GB」 -- or null where that is not known, for
 * `updates.newVersion` to stand.
 */
export function modelDownloadVersion(t: Translate, bytes: number | null | undefined): string | null {
  const known = downloadBytesWorthSaying(bytes);
  return known === null ? null : t("modelDownload.rowVersion", { size: formatBytes(known) });
}
