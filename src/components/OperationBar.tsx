import { useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useCancelOperation, useOperations, useSettings } from "../lib/queries";
import {
  cancelState,
  currentOf,
  isActive,
  operationTone,
  outcomeTone,
  operationWords,
  printedAnything,
  runsToItsEnd,
  trackRun,
  useOperationName,
  type OperationRun,
  type OutcomeTone,
} from "../lib/operations";
import { cancelledRunWords, failedRunWords } from "../lib/runResult";
import { viewLogKey } from "./FailureNextStep";
import { useUiStore } from "../store/ui";
import type { TFunction } from "i18next";
import type { OpSummary } from "../lib/types";
import { OutcomeIcon } from "./OutcomeIcon";
import { CloseIcon } from "./icons";
import { BUTTON, SMALL_ICON_BUTTON } from "./ui/controls";

/**
 * The bar's buttons: View log and Cancel small and grey, as a status line's
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
 * height back. While something runs, 「ffmpeg：正在更新…」 -- what it acts
 * on, then what it does and where it stands, never a bare verb in front,
 * which in English reads as a command (walk-3 W3-3) -- with Cancel where cancelling
 * can still do something, and the way to its log; with several, how far
 * along the run is: 「正在处理第2个，共3个」 and a 4 by 60 bar, and 「全部取消」
 * for all of it that can still be stopped -- 「取消其余」 while one of it
 * runs that nothing can stop. Once everything is done, how it went in
 * place of where it stood -- 「已更新3个工具」, 「1个更新失败，2个已成功」, 「git：
 * 更新 · 网络连接失败」, 「2个已取消」 -- with its log where it needs a look
 * or was cancelled once it had printed something, and a close ×.
 * What it does is said in front wherever the words do not say it
 * (`operationWords`). What a program wrote -- a tool's error, macOS's
 * reason a program would not start -- is said here only with "Show
 * technical details" on (`outcomeWords`); its log always has it. Closed, it stays away until the
 * next operation starts.
 *
 * Its operations are a run (`trackRun`): the ones started while others
 * were still under way belong together, and a new one started after
 * everything had finished replaces the last run's result. The one a run
 * names while under way is the one nothing can stop (`runsToItsEnd`),
 * while one runs, and otherwise the oldest actually working (`currentOf`);
 * with a single operation that is it, and its Cancel is that one's alone.
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
  const openLogRun = useUiStore((s) => s.openLogRun);
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
  const openLog = (op: OpSummary) => {
    setFocusedOpId(op.id);
    setDrawerOpen(true);
  };
  // Keyed, as are Cancel and Close beside it: one node from the run's first
  // step to how it went, wherever it stands among them. Unkeyed, the
  // running bar's View Log (second) became the finished bar's Close
  // (second): the log, closed after the run ended, gave the focus back to a
  // Close, which the next Space pressed.
  //
  // Named for what it opens: 「查看步骤」 where sudo wanted the Mac's
  // password, whose log has the command for Terminal (walk-2 W2-5).
  const viewLog = (op: OpSummary) => (
    <button key="viewLog" type="button" onClick={() => openLog(op)} className={VIEW_LOG_BUTTON}>
      {t(viewLogKey(op))}
    </button>
  );
  // Several to look at, once a run is done: 「查看N个日志」, which opens
  // the log on the first of them, from where it steps through the rest
  // (`openLogRun`), in the order they ran (walk-2 W2-4).
  const viewLogs = (ops: OpSummary[]) => {
    const ids = ops.map((op) => op.id).sort((a, b) => a - b);
    return (
      <button key="viewLog" type="button" onClick={() => openLogRun(ids, ids[0])} className={VIEW_LOG_BUTTON}>
        {t("failureSteps.viewLogs", { count: ids.length })}
      </button>
    );
  };

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
  // such a one while it runs -- the one its Cancel leaves running, before
  // the press and after -- and otherwise the oldest actually working
  // (`currentOf`).
  const unstoppable = active.filter(runsToItsEnd);
  const current = (batch ? currentOf(unstoppable) : undefined) ?? currentOf(active);
  if (current !== undefined) {
    const done = total - active.length;
    const line = t("operations.current", { name: nameOf(current), status: operationWords(t, current, logs, technical) });
    // With several, one Cancel for the run: every operation of it that can
    // still be cancelled -- each one queued, whatever its plan, and each
    // one running whose plan allows it, the current one among them when it
    // can be. That is all of them, 「全部取消」, unless one running cannot
    // be (a NoCancel op already running): then it is the rest of them,
    // 「取消其余」, and that one goes on to its end, named on the bar. The
    // queued ones go first, oldest first, so none of them starts in the
    // moment its turn comes. Pressable while there is one to stop; held,
    // not pressable, while the cancels are on their way, as a single
    // Cancel is.
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
          <span className="shrink-0">{runWords(t, active, done, total)}</span>
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
            key="progress"
            aria-hidden="true"
            data-run-progress=""
            className="h-1 w-15 shrink-0 overflow-hidden rounded-full bg-fill"
          >
            <span className="block h-full rounded-full bg-accent" style={{ width: `${(done / total) * 100}%` }} />
          </span>
        ) : null}
        {viewLog(current)}
        {cancel !== "none" ? (
          <button
            key="stop"
            type="button"
            onClick={cancelNow}
            disabled={cancel === "disabled"}
            className={STOP_BUTTON}
          >
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
    const toLook = inRun.filter((op, index) =>
      needsALook(tones[index]) || Boolean(op.follow_up_warnings?.length),
    );
    const newestToLook = toLook.reduce<OpSummary | undefined>(
      (found, op) => (found === undefined || op.id > found.id ? op : found),
      undefined,
    );
    // Every one of it an update: said as updates, 「已更新3个工具」; every
    // one an uninstall -- a batch from the Installed page -- as uninstalls,
    // 「已卸载3个工具」.
    const updates = inRun.every((op) => op.kind === "Upgrade");
    const uninstalls = inRun.every((op) => op.kind === "Uninstall");
    let tone: OutcomeTone;
    let words: string;
    let logOf: OpSummary | undefined;
    let logsOf: OpSummary[] = [];
    if (total === 1) {
      const [op] = inRun;
      tone = operationTone(op);
      words = t("operations.current", { name: nameOf(op), status: operationWords(t, op, logs, technical) });
      // The log of anything but a plain success: to see what went wrong,
      // or -- after a cancel -- what had already happened, where anything had.
      logOf = tone === "success" || (tone === "cancelled" && !printedAnything(op, logs)) ? undefined : op;
    } else if (newestToLook !== undefined) {
      tone = tones.includes("failure") ? "failure" : "attention";
      // Failures said as failures, with what else the run came to --
      // 「2个更新失败，3个已成功」; 「需要查看」 only for a run with none.
      // With some cancelled, each part, as a failure's run says them --
      // 「3个需要查看，10个已取消」 after Cancel All (r35 U1); 「13个中有3个
      // 需要查看」 would leave the ten not updated unsaid.
      // All succeeded but for a warning left after: 「已更新2个工具，1个有
      // 警告」 where every one was an update, and in words for any
      // operation where not -- an uninstall started while an update ran
      // was not updated (r31 E1): 「2个都已成功，1个有警告」.
      words =
        failedRunWords(t, inRun) ?? cancelledRunWords(t, inRun, toLook) ?? (tones.every((each) => each === "success")
          ? t(updates ? "followUpWarning.batch" : "followUpWarning.batchSucceeded", { count: toLook.length, total })
          : t("operations.batch.needsAttention", { count: toLook.length, total }));
      if (toLook.length > 1) logsOf = toLook;
      else logOf = newestToLook;
    } else if (tones.every((each) => each === "success")) {
      tone = "success";
      words = updates
        ? t("operations.batch.allUpdated", { count: total })
        : uninstalls
          ? t("batchUninstall.bar.allUninstalled", { count: total })
          : t("operations.batch.allSucceeded", { count: total });
    } else {
      tone = "cancelled";
      const succeeded = tones.filter((each) => each === "success").length;
      const cancelled = inRun.filter((_, index) => tones[index] === "cancelled");
      // 「1个已成功，2个已取消」; with none succeeded, not 「0个已成功」 first,
      // read as one that was to succeed and did not: 「2个已取消」, a zero
      // left out as `failedRunWords` leaves it (r24 W5).
      words =
        succeeded > 0
          ? t("operations.batch.finished", { succeeded, cancelled: cancelled.length })
          : t("failureSteps.bar.cancelled", { count: cancelled.length });
      // Their logs, as one operation's bar offers its own after a cancel:
      // what had already happened -- 「查看2个日志」, stepping through them --
      // of those that had started and printed something. Cancel All while
      // most still waited their turn: the one that ran, never an empty page
      // for each of the rest; none at all where none had started.
      const looked = cancelled.filter((op) => printedAnything(op, logs));
      if (looked.length > 1) logsOf = looked;
      else logOf = looked[0];
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
        {logsOf.length > 0 ? viewLogs(logsOf) : logOf !== undefined ? viewLog(logOf) : null}
        <button
          key="close"
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

/**
 * Where a run of several stands: 「正在处理第2个，共5个」 while they go one
 * at a time, and, where several run at once -- updates on different
 * sources do -- how many, and how many wait: 「正在同时处理3个，还有2个在
 * 排队」, never 「第1个」 of a run three of which are under way.
 */
function runWords(t: TFunction, active: readonly OpSummary[], done: number, total: number): string {
  const running = active.filter((op) => op.status === "Running").length;
  const queued = active.filter((op) => op.status === "Queued").length;
  if (running > 1) {
    return queued > 0
      ? t("operationsMore.runningSeveralQueued", { running, queued })
      : t("operationsMore.runningSeveral", { running });
  }
  return t("operations.batch.running", { current: Math.min(done + 1, total), total });
}
