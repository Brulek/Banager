import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useCancelOperation, useOperations, useSettings } from "../lib/queries";
import {
  OP_KIND_KEYS,
  cancelState,
  currentOf,
  isActive,
  outcomeTone,
  outcomeWords,
  runsToItsEnd,
  statusKey,
  trackRun,
  useOperationName,
  type OperationRun,
  type OutcomeTone,
} from "../lib/operations";
import { useUiStore } from "../store/ui";
import type { OpSummary } from "../lib/types";
import { OutcomeIcon } from "./OutcomeIcon";
import { CloseIcon } from "./icons";
import { BUTTON, SMALL_ICON_BUTTON } from "./ui/controls";

/**
 * The bar's buttons: View log and Stop small and grey, as a status line's
 * are (spec §3.5), and its close × a 20 square with a 16 cross, drawn to
 * the bar's edge.
 */
const VIEW_LOG_BUTTON = BUTTON.small.grey;
const STOP_BUTTON = BUTTON.small.grey;
const DISMISS_BUTTON = `-mr-1 ${SMALL_ICON_BUTTON}`;

/** Whether an ending is one to look at: its log is offered beside it. */
function needsALook(tone: OutcomeTone): boolean {
  return tone === "attention" || tone === "failure";
}

/**
 * The strip at the foot of the window that says what Banager is doing, in
 * the manner of a Mac window's status bar (spec §3.10): 28 high, the
 * window's own background, a hairline over it, its words 11/14 in the
 * muted grey. Nothing at all until something has run: the window has that
 * height back. While something runs, 「更新ffmpeg：正在处理…」 -- what it
 * does, what it does it to, where it stands -- with Stop where stopping
 * can still do something, and the way to its log; with several, how far
 * along the run is: 「正在处理第2个，共3个」 and a 4 by 60 bar, and 「全部停止」
 * for all of it that can still be stopped -- 「停止其余」 while one of it
 * runs that nothing can stop. Once everything is done, how it went in
 * place of where it stood -- 「已更新3个工具」, 「1个未能更新」, 「更新git：
 * 网络连接失败」 -- with its log where it needs a look, and a close ×.
 * What a program wrote -- a tool's error, macOS's reason a program would
 * not start -- is said here only with "Show technical details" on
 * (`outcomeWords`); its log always has it. Closed, it stays away until the
 * next operation starts.
 *
 * Its operations are a run (`trackRun`): the ones started while others
 * were still under way belong together, and a new one started after
 * everything had finished replaces the last run's result. The one a run
 * names while under way is the one nothing can stop (`runsToItsEnd`),
 * while one runs, and otherwise the oldest actually working (`currentOf`);
 * with a single operation that is it, and its Stop is that one's alone.
 * Once a run of one is done, that one -- so an update that finished while
 * the user was looking elsewhere is still on screen, with how it went.
 */
export function OperationBar() {
  const { t } = useTranslation();
  const { data: operations } = useOperations();
  const { data: settings } = useSettings();
  const technical = settings?.show_technical_details ?? false;
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
    <button type="button" onClick={() => openLog(op)} className={VIEW_LOG_BUTTON}>
      {t("common.viewLog")}
    </button>
  );

  // What the bar shows, in three parts about the one line it says: what
  // goes before the line (how a finished run went, as a symbol), the line,
  // and its buttons after it.
  let lead: ReactNode;
  let said: ReactNode;
  let saidClassName: string;
  let after: ReactNode;
  const batch = total > 1;
  // What nothing can stop now (`runsToItsEnd`): rustup's self update or
  // self uninstall once it has started. In a run of several, the bar names
  // such a one while it runs -- the one its Stop leaves running, before
  // the press and after -- and otherwise the oldest actually working
  // (`currentOf`).
  const unstoppable = active.filter(runsToItsEnd);
  const current = (batch ? currentOf(unstoppable) : undefined) ?? currentOf(active);
  if (current !== undefined) {
    const done = total - active.length;
    const status = statusKey(current, logs);
    const line = t("operations.current", { ...titleOf(current), status: status === null ? "" : t(status) });
    // With several, one Stop for the run: every operation of it that can
    // still be cancelled -- each one queued, whatever its plan, and each
    // one running whose plan allows it, the current one among them when it
    // can be. That is all of them, 「全部停止」, unless one running cannot
    // be (a NoCancel op already running): then it is the rest of them,
    // 「停止其余」, and that one goes on to its end, named on the bar. The
    // queued ones go first, oldest first, so none of them starts in the
    // moment its turn comes. Pressable while there is one to stop; held,
    // not pressable, while the cancels are on their way, as a single
    // Stop is.
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
    lead = null;
    said = (
      <>
        {batch ? (
          <span className="shrink-0">{t("operations.batch.running", { current: Math.min(done + 1, total), total })}</span>
        ) : null}
        <span title={line} className="min-w-0 truncate">
          {line}
        </span>
      </>
    );
    saidClassName = "flex min-w-0 flex-1 items-baseline gap-2";
    after = (
      <>
        {batch ? (
          // How much of the run is done, beside the words that say it: a
          // 4 by 60 capsule, the accent over the fill.
          <span
            aria-hidden="true"
            data-run-progress=""
            className="h-1 w-15 shrink-0 overflow-hidden rounded-full bg-fill"
          >
            <span className="block h-full rounded-full bg-accent" style={{ width: `${(done / total) * 100}%` }} />
          </span>
        ) : null}
        {viewLog(current)}
        {cancel !== "none" ? (
          <button type="button" onClick={cancelNow} disabled={cancel === "disabled"} className={STOP_BUTTON}>
            {!batch
              ? t("operations.stop")
              : unstoppable.length > 0
                ? t("operations.batch.cancelRest")
                : t("operations.batch.cancelAll")}
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
    // Every one of it an update: said as updates, 「已更新3个工具」.
    const updates = inRun.every((op) => op.kind === "Upgrade");
    let tone: OutcomeTone;
    let words: string;
    let logOf: OpSummary | undefined;
    if (total === 1) {
      const [op] = inRun;
      tone = tones[0];
      words = t("operations.current", { ...titleOf(op), status: outcomeWords(t, op.outcome, technical) });
      // The log of anything but a plain success: to see what went wrong,
      // or -- after a cancel -- what had already happened.
      logOf = tone === "success" ? undefined : op;
    } else if (newestToLook !== undefined) {
      tone = tones.includes("failure") ? "failure" : "attention";
      // Only failures, of updates: how many did not update.
      const onlyFailed = updates && toLook.every((op) => outcomeTone(op.outcome) === "failure");
      words = onlyFailed
        ? t("operations.batch.notUpdated", { count: toLook.length })
        : t("operations.batch.needsAttention", { count: toLook.length, total });
      logOf = newestToLook;
    } else if (tones.every((each) => each === "success")) {
      tone = "success";
      words = updates
        ? t("operations.batch.allUpdated", { count: total })
        : t("operations.batch.allSucceeded", { count: total });
    } else {
      tone = "cancelled";
      words = t("operations.batch.finished", {
        succeeded: tones.filter((each) => each === "success").length,
        cancelled: tones.filter((each) => each === "cancelled").length,
      });
    }
    lead = <OutcomeIcon tone={tone} size={12} />;
    said = (
      <span title={words} className="min-w-0 truncate">
        {words}
      </span>
    );
    saidClassName = "flex min-w-0 flex-1";
    after = (
      <>
        {logOf !== undefined ? viewLog(logOf) : null}
        <button
          type="button"
          aria-label={t("common.close")}
          onClick={() => setDismissedThrough(newest)}
          className={DISMISS_BUTTON}
        >
          <CloseIcon size={16} />
        </button>
      </>
    );
  }

  return (
    <footer
      aria-label={t("app.operationBarRegion")}
      className="flex h-7 shrink-0 items-center gap-2 border-t border-separator bg-content px-5 text-small text-muted motion-safe:animate-fade-in"
    >
      {lead}
      {/* The one line, live: one node from the first step of a run to how
          it went, its words changing in it, so a screen reader hears each
          change -- a new node for the outcome would be one it may not. */}
      <p aria-live="polite" data-operation-line="" className={saidClassName}>
        {said}
      </p>
      {after}
    </footer>
  );
}
