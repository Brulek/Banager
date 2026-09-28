import { useState } from "react";
import { useTranslation } from "react-i18next";
import { useCancelOperation, useOperations } from "../lib/queries";
import {
  OP_KIND_KEYS,
  cancelState,
  currentOf,
  isActive,
  outcomeSentence,
  outcomeTone,
  statusKey,
  trackRun,
  useOperationName,
  type OperationRun,
  type OutcomeTone,
} from "../lib/operations";
import { useUiStore } from "../store/ui";
import type { OpSummary } from "../lib/types";
import { OutcomeIcon } from "./OutcomeIcon";
import { CloseIcon, SpinnerIcon } from "./icons";

const LINK_BUTTON =
  "shrink-0 rounded-sm text-small font-medium text-accent-text outline-none hover:underline focus-visible:ring-2 focus-visible:ring-accent";
const CANCEL_BUTTON =
  "h-7 shrink-0 rounded-button border border-border bg-surface px-3 text-small font-medium text-foreground outline-none transition-colors hover:bg-hover focus-visible:ring-2 focus-visible:ring-accent disabled:opacity-50 disabled:hover:bg-surface";
const DISMISS_BUTTON =
  "-mr-1.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-button text-muted outline-none transition-colors hover:bg-hover hover:text-foreground focus-visible:ring-2 focus-visible:ring-accent";

/** Whether an ending is one to look at: its log is offered beside it. */
function needsALook(tone: OutcomeTone): boolean {
  return tone === "attention" || tone === "failure";
}

/**
 * The strip at the foot of the window that says what Canager is doing, in
 * the manner of 360's download manager. Nothing at all until something
 * has run: the window has that height back. While something runs, a
 * spinner and 「更新 ffmpeg：进行中」 -- what it does, what it does it to,
 * where it stands -- with Cancel where Cancel can still do something, and
 * the way to its log; with several, how far along the run is: 「正在处理
 * 3 个中的第 2 个」, and 「全部取消」 for all of it that can still be
 * cancelled. Once everything is done, how it went in place of where
 * it stood: 「更新 ffmpeg：已成功」, or the outcome with a warning sign and
 * its log when it needs a look -- and a close button. Closed, it stays away
 * until the next operation starts.
 *
 * Its operations are a run (`trackRun`): the ones started while others
 * were still under way belong together, and a new one started after
 * everything had finished replaces the last run's result. The one a run
 * names while under way is the oldest actually working (`currentOf`);
 * with a single operation that is it, and its Cancel is that one's alone.
 * Once a run of one is done, that one -- so an update that finished while
 * the user was looking elsewhere is still on screen, with how it went.
 */
export function OperationBar() {
  const { t } = useTranslation();
  const { data: operations } = useOperations();
  const cancelMutation = useCancelOperation();
  const setDrawerOpen = useUiStore((s) => s.setDrawerOpen);
  const setFocusedOpId = useUiStore((s) => s.setFocusedOpId);
  const logs = useUiStore((s) => s.logs);
  const nameOf = useOperationName(operations);
  const [run, setRun] = useState<OperationRun | null>(null);
  // The newest operation when the bar was closed: it comes back for the
  // first one after it.
  const [dismissedThrough, setDismissedThrough] = useState(-1);

  // Kept up to date while rendering, not in an effect, which would draw
  // the last run's count under a new run's first operation for a frame.
  // `trackRun` hands `run` back when nothing changed, so this settles.
  const tracked = operations === undefined ? run : trackRun(run, operations);
  if (tracked !== run) setRun(tracked);

  const newest = (operations ?? []).reduce((highest, op) => Math.max(highest, op.id), -1);
  if (operations === undefined || tracked === null || newest < 0 || newest <= dismissedThrough) {
    return null;
  }

  const inRun = operations.filter((op) => op.id > tracked.floor);
  const total = inRun.length;
  const active = inRun.filter(isActive);
  const titleOf = (op: OpSummary) => ({ kind: t(OP_KIND_KEYS[op.kind]), name: nameOf(op) });
  const openLog = (op: OpSummary) => {
    setFocusedOpId(op.id);
    setDrawerOpen(true);
  };
  const viewLog = (op: OpSummary) => (
    <button type="button" onClick={() => openLog(op)} className={LINK_BUTTON}>
      {t("common.viewLog")}
    </button>
  );

  let body;
  const current = currentOf(active);
  if (current !== undefined) {
    const done = total - active.length;
    const status = statusKey(current, logs);
    const line = t("operations.current", { ...titleOf(current), status: status === null ? "" : t(status) });
    // With several, Cancel all: every operation of the run that can still
    // be cancelled -- each one queued, whatever its plan, and each one
    // running whose plan allows it, the current one among them when it
    // can be. One that cannot be (a NoCancel op already running) goes on
    // to its end, on the bar. The queued ones go first, oldest first, so
    // none of them starts in the moment its turn comes. Pressable while
    // there is one to cancel; held, not pressable, while the cancels are
    // on their way, as a single Cancel is.
    const batch = total > 1;
    const cancellable = active
      .filter((op) => cancelState(op) === "enabled")
      .sort((a, b) => Number(b.status === "Queued") - Number(a.status === "Queued") || a.id - b.id);
    const cancel = !batch
      ? cancelState(current)
      : cancellable.length > 0
        ? "enabled"
        : active.some((op) => cancelState(op) === "disabled")
          ? "disabled"
          : "none";
    const cancelNow = () => {
      if (!batch) {
        cancelMutation.mutate(current.id);
        return;
      }
      for (const op of cancellable) cancelMutation.mutate(op.id);
    };
    body = (
      <>
        <SpinnerIcon size={14} className="shrink-0 text-accent-text" />
        <p aria-live="polite" className="flex min-w-0 flex-1 items-baseline gap-2">
          {total > 1 ? (
            <>
              <span className="shrink-0 font-medium text-foreground">
                {t("operations.batch.running", { current: Math.min(done + 1, total), total })}
              </span>
              <span title={line} className="min-w-0 truncate text-muted">
                {line}
              </span>
            </>
          ) : (
            <span title={line} className="min-w-0 truncate text-foreground">
              {line}
            </span>
          )}
        </p>
        {total > 1 ? (
          // How much of the run is done, beside the words that say it.
          <span aria-hidden="true" className="h-1 w-14 shrink-0 overflow-hidden rounded-full bg-hover">
            <span
              className="block h-full rounded-full bg-accent transition-[width] duration-300"
              style={{ width: `${(done / total) * 100}%` }}
            />
          </span>
        ) : null}
        {viewLog(current)}
        {cancel !== "none" ? (
          <button type="button" onClick={cancelNow} disabled={cancel === "disabled"} className={CANCEL_BUTTON}>
            {batch ? t("operations.batch.cancelAll") : t("common.cancel")}
          </button>
        ) : null}
      </>
    );
  } else {
    // Done: one operation's own outcome, or what the run's came to.
    const tones = inRun.map((op) => outcomeTone(op.outcome));
    const toLook = inRun.filter((_, index) => needsALook(tones[index]));
    const newestToLook = toLook.reduce<OpSummary | undefined>(
      (found, op) => (found === undefined || op.id > found.id ? op : found),
      undefined,
    );
    let tone: OutcomeTone;
    let words: string;
    let logOf: OpSummary | undefined;
    if (total === 1) {
      const [op] = inRun;
      tone = tones[0];
      words = t("operations.current", { ...titleOf(op), status: outcomeSentence(t, op.outcome) });
      // The log of anything but a plain success: to see what went wrong,
      // or -- after a cancel -- what had already happened.
      logOf = tone === "success" ? undefined : op;
    } else if (newestToLook !== undefined) {
      tone = tones.includes("failure") ? "failure" : "attention";
      words = t("operations.batch.needsAttention", { count: toLook.length, total });
      logOf = newestToLook;
    } else if (tones.every((each) => each === "success")) {
      tone = "success";
      words = t("operations.batch.allSucceeded", { count: total });
    } else {
      tone = "cancelled";
      words = t("operations.batch.finished", {
        succeeded: tones.filter((each) => each === "success").length,
        cancelled: tones.filter((each) => each === "cancelled").length,
      });
    }
    body = (
      <>
        <OutcomeIcon tone={tone} size={15} />
        <p aria-live="polite" className="flex min-w-0 flex-1">
          <span title={words} className="min-w-0 truncate text-foreground">
            {words}
          </span>
        </p>
        {logOf !== undefined ? viewLog(logOf) : null}
        <button
          type="button"
          aria-label={t("common.close")}
          onClick={() => setDismissedThrough(newest)}
          className={DISMISS_BUTTON}
        >
          <CloseIcon size={14} />
        </button>
      </>
    );
  }

  return (
    <footer
      aria-label={t("app.operationBarRegion")}
      className="flex h-10 shrink-0 items-center gap-3 border-t border-border bg-surface px-4 text-body motion-safe:animate-fade-in"
    >
      {body}
    </footer>
  );
}
