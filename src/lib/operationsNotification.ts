/**
 * The page's part in the notification when operations finish, Settings'
 * 「操作完成时通知」 (src-tauri/src/notify_ops.rs): as each run of
 * operations ends -- the operations the operation bar shows together
 * (`trackRun`) -- it tells Rust how the run went, and Rust decides whether
 * a notification goes out: only with the setting on and another app in
 * front, never for a run the user watched finish in the focused window.
 * Closing the window does not stop the page, which is hidden with it and
 * hears the operations finish (src-tauri/src/window.rs): that is when the
 * notification is for.
 */
import { useEffect, useRef } from "react";
import { reportFinishedRun } from "./api";
import { outcomeTone, trackRun, type OperationRun } from "./operations";
import { useOperations } from "./queries";
import type { FinishedRun, OpSummary, RunKind } from "./types";

/**
 * How the finished operations `ran` went, as the report says it: the
 * newest one's id, what they did -- every one an update, every one an
 * uninstall, or anything else -- and how many ended each way
 * (`outcomeTone`), the cancelled ones in none. Null for no operation.
 */
export function finishedRunOf(ran: OpSummary[]): FinishedRun | null {
  if (ran.length === 0) return null;
  const kind: RunKind = ran.every((op) => op.kind === "Upgrade")
    ? "Upgrade"
    : ran.every((op) => op.kind === "Uninstall")
      ? "Uninstall"
      : "Other";
  const run: FinishedRun = {
    last_op: Math.max(...ran.map((op) => op.id)),
    kind,
    succeeded: 0,
    failed: 0,
    attention: 0,
  };
  for (const op of ran) {
    const tone = outcomeTone(op.outcome);
    if (tone === "success") run.succeeded += 1;
    else if (tone === "failure") run.failed += 1;
    else if (tone === "attention") run.attention += 1;
  }
  return run;
}

/**
 * Whether the look that took the run from `previous` to `next` saw it
 * end: something was under way at the last look, or operations started
 * since, and nothing is now. The first look (`previous` null) reports
 * nothing: what had finished before the page loaded is no news of this
 * page's.
 */
export function runEnded(previous: OperationRun | null, next: OperationRun): boolean {
  if (previous === null || next.open) return false;
  return previous.open || next.seen > previous.seen;
}

/**
 * Mounted once, by `App`'s `UpdateWatchers`: follows the operations as the
 * operation bar does (`trackRun`), and as a run ends (`runEnded`) reports
 * it (`reportFinishedRun`). Rust posts nothing unless the setting is on
 * and another app is in front, and posts a run once, however often it is
 * reported. A report that fails is not sent again.
 */
export function useOperationsNotification(): void {
  const { data: operations } = useOperations();
  const run = useRef<OperationRun | null>(null);

  useEffect(() => {
    if (operations === undefined) return;
    const previous = run.current;
    const next = trackRun(previous, operations);
    run.current = next;
    if (!runEnded(previous, next)) return;
    const finished = finishedRunOf(operations.filter((op) => op.id > next.floor));
    if (finished === null) return;
    reportFinishedRun(finished).catch((e: unknown) => {
      console.error("report_finished_run failed", e);
    });
  }, [operations]);
}
