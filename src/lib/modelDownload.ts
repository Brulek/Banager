/**
 * How much an Ollama model's update downloads at most
 * (`UpdateCandidate.download_bytes`, from the two manifests the check
 * already has: crates/banager-core/src/adapters/ollama/parse.rs
 * `changed_blob_bytes`), said where the update is decided on: the model's
 * note in the update confirmation -- one tool, or Update All's list. Not
 * on its row: 「有新版本」 there leaves a long model name its room. An
 * upper bound, as a file another model shares is already on this Mac:
 * 「最多约4.7 GB」, "up to about 4.7 GB". Not known, the note says what it
 * said before: that the files that changed are downloaded
 * (`warnings.downloadsModelChanges`).
 */

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

const UNITS = ["B", "KB", "MB", "GB", "TB"] as const;

/**
 * `bytes` as `formatBytes` (src/lib/format.ts) writes a size -- decimal
 * units, one decimal at most, "4.7 GB" -- but rounded up to that decimal,
 * never to the nearest: an upper bound that reads lower than itself
 * (4,640,000,000 bytes as 「最多约4.6 GB」) would not be one. Worked in
 * tenths of the unit from the whole bytes, so 4.7 GB exactly stays 4.7
 * rather than a binary fraction of it rounding up to 4.8.
 */
export function formatBytesRoundedUp(bytes: number): string {
  let unit = 0;
  while (unit < UNITS.length - 1 && bytes >= 1000 ** (unit + 1)) unit += 1;
  if (unit === 0) return `${Math.ceil(bytes)} ${UNITS[0]}`;
  let tenths = Math.ceil((bytes * 10) / 1000 ** unit);
  // 999.95 KB rounds up to "1000 KB"; that is 1 MB.
  if (tenths >= 10_000 && unit < UNITS.length - 1) {
    unit += 1;
    tenths = Math.ceil((bytes * 10) / 1000 ** unit);
  }
  return `${(tenths / 10).toFixed(1).replace(/\.0$/, "")} ${UNITS[unit]}`;
}

/**
 * The model's note in the update confirmation, with the most it
 * downloads: 「需要下载已更改的模型文件，最多约4.7 GB。」 -- or null where
 * that is not known, for `warnings.downloadsModelChanges` to stand.
 */
export function modelDownloadNote(t: Translate, bytes: number | null | undefined): string | null {
  const known = downloadBytesWorthSaying(bytes);
  return known === null ? null : t("modelDownload.note", { size: formatBytesRoundedUp(known) });
}
