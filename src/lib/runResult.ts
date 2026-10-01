/**
 * What a finished run of several came to, in the operation bar's one
 * line, where some of it failed: the failures named as failures, never
 * folded into 「需要查看」 -- 「2个更新失败，3个已成功」 -- and after them
 * what else the run holds: the ones that need a look (NeedsAttention,
 * Unconfirmed), the ones cancelled. 「需要查看」 alone stays the bar's
 * words for a run with nothing failed in it (`operations.batch.
 * needsAttention`).
 *
 * Uninstalls keep the heading of the result block over the Installed
 * list (`BatchUninstallResult`), counted as it counts them: every one not
 * `Succeeded` -- failed, cancelled or needing a look -- is one that 「没有
 * 卸载」 (`batchUninstallMore.mixed`, `batchUninstall.bar.notUninstalled`),
 * and the block's list says how each ended. So a run with a failure in it
 * is said the same way in both. A run of uninstalls with no failure in
 * it, but one needing a look, still says 「需要查看」 on the bar, as before.
 *
 * Pure: `t` gives the words, `outcomeTone` sorts the endings.
 */
import type { OpSummary } from "./types";
import type { Translate } from "./diagnostics";
import { outcomeTone } from "./operations";

/** How many of `ops` ended each way, by `outcomeTone`. */
export function runTally(ops: readonly OpSummary[]) {
  const tally = { failed: 0, attention: 0, succeeded: 0, cancelled: 0 };
  for (const op of ops) {
    const tone = outcomeTone(op.outcome);
    if (tone === "failure") tally.failed += 1;
    else if (tone === "attention") tally.attention += 1;
    else if (tone === "success") tally.succeeded += 1;
    else tally.cancelled += 1;
  }
  return tally;
}

/**
 * The bar's words for a finished run of several with at least one
 * failure in it, or null for one with none. Updates are said as updates
 * (「N个更新失败」), uninstalls as the result block says them, and a run
 * of different kinds as plain failures (「N个失败」).
 */
export function failedRunWords(t: Translate, ops: readonly OpSummary[]): string | null {
  const { failed, attention, succeeded, cancelled } = runTally(ops);
  if (failed === 0) return null;
  if (ops.every((op) => op.kind === "Uninstall")) {
    // The result block's heading, word for word: the successes are in it,
    // and so is every other ending, as not uninstalled.
    const notUninstalled = failed + attention + cancelled;
    return succeeded > 0
      ? t("batchUninstallMore.mixed", { done: succeeded, count: notUninstalled })
      : t("batchUninstall.bar.notUninstalled", { count: notUninstalled });
  }
  const updates = ops.every((op) => op.kind === "Upgrade");
  const parts = [
    updates ? t("failureSteps.bar.updatesFailed", { count: failed }) : t("failureSteps.bar.failed", { count: failed }),
  ];
  if (succeeded > 0) parts.push(t("failureSteps.bar.succeeded", { count: succeeded }));
  if (attention > 0) parts.push(t("failureSteps.bar.needsAttention", { count: attention }));
  if (cancelled > 0) parts.push(t("failureSteps.bar.cancelled", { count: cancelled }));
  return parts.join(t("overview.listSeparator"));
}
