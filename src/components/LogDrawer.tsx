import { useEffect, useId, useRef, useState, type UIEvent } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import type { LogNote, OpSummary } from "../lib/types";
import { useUiStore } from "../store/ui";
import { useCancelOperation, useOperations, useSettings } from "../lib/queries";
import { copyStatusText, useCopyCommand } from "../lib/clipboard";
import { FAILURE_CAUSE_KEYS, outcomeCause } from "../lib/failureCause";
import { outcomeDetailKey } from "../lib/format";
import {
  OP_CANCEL_KEYS,
  cancelState,
  isActive,
  outcomeTone,
  operationWords,
  statusKey,
  useOperationName,
} from "../lib/operations";
import { Dialog } from "./ui/Dialog";
import { BUTTON } from "./ui/controls";
import { ScrollArea } from "./ui/ScrollArea";
import { OutcomeIcon } from "./OutcomeIcon";
import { PasswordCommand } from "./PasswordCommand";
import { FailureNextStep, SubtitleStep, failureLogStep, subtitleStep } from "./FailureNextStep";
import { SpinnerIcon } from "./icons";

const NEAR_BOTTOM_PX = 32;

/**
 * The words for one of Banager's own log notes, in the user's language.
 * Each `LogNote` variant needs a case here: the log is the one place the
 * app shows text it did not write, and a remark of Banager's that arrived
 * as plain text would be English sitting among the tool's lines.
 */
function noteText(t: TFunction, note: LogNote): string {
  if ("WaitingForBrewUpdate" in note) {
    return t("operations.logNote.waitingForBrewUpdate", {
      minutes: note.WaitingForBrewUpdate.minutes,
    });
  }
  if ("ReadFailed" in note) {
    const { stream, error } = note.ReadFailed;
    return stream === "Stderr"
      ? t("operations.logNote.readFailedStderr", { error })
      : t("operations.logNote.readFailedStdout", { error });
  }
  if ("MovedToTrash" in note) {
    const { path, trashed_to } = note.MovedToTrash;
    return t("operations.logNote.movedToTrash", { path, trashedTo: trashed_to });
  }
  if ("TrashFailed" in note) {
    const { path, error } = note.TrashFailed;
    return t("operations.logNote.trashFailed", { path, error });
  }
  if ("OutOfTime" in note) {
    const { path, seconds } = note.OutOfTime;
    return t("operations.logNote.outOfTime", { path, seconds });
  }
  if ("BackAfterUninstall" in note) {
    return t("operations.logNote.backAfterUninstall", { path: note.BackAfterUninstall.path });
  }
  // U9: an update's follow-up `brew cleanup` -- its own lines follow it,
  // and whatever it wrote of why it stopped is right above the second.
  if ("CleaningUpOldVersions" in note) {
    return t("brewVersions.logCleaningUp", { name: note.CleaningUpOldVersions.name });
  }
  // With an exit code or without -- stopped, out of time, or never
  // started -- it did not finish; "stopped" would not be true of all.
  if ("OldVersionsNotCleanedUp" in note) {
    return t("brewVersions.logNotCleanedUp");
  }
  if ("OldVersionsKept" in note) {
    const { name, versions } = note.OldVersionsKept;
    return t("brewVersions.logKept", {
      name,
      versions: versions.join(t("common.listSeparator")),
      count: versions.length,
    });
  }
  // Review F4 (r6): it did not start -- the settings, asked again at its
  // turn, no longer allowed it.
  if ("OldVersionsCleanupSkipped" in note) {
    return t("brewVersions.logCleanupSkipped");
  }
  // y1-keg: what became of a keg-only formula's link after its update --
  // `brew link` starts (its own lines follow), Homebrew had linked it back
  // itself, or which commands typed in Terminal no longer run it.
  if ("RelinkingAfterUpdate" in note) {
    return t("kegLinks.logRelinking", { name: note.RelinkingAfterUpdate.name });
  }
  if ("StillLinkedAfterUpdate" in note) {
    return t("kegLinks.logStillLinked", { name: note.StillLinkedAfterUpdate.name });
  }
  if ("NoLongerLinked" in note) {
    const { name, commands } = note.NoLongerLinked;
    return t("kegLinks.logNoLongerLinked", { name, commands: commands.join(t("common.listSeparator")) });
  }
  const unhandled: never = note;
  return unhandled;
}

/**
 * One operation's log, as a dialog 560 wide (spec §3.10, R11: the log
 * answers an operation, so it stays a dialog): the tool it acts on
 * (「ffmpeg」) as its title -- not 「更新ffmpeg」, whose English "Update
 * ffmpeg" reads as a command (walk-3 W3-3) -- and what it does, where it
 * stands or how it ended under that, in the words the operation bar uses
 * (`operationWords`): 「正在更新…」, 「更新 · 网络连接失败」 where the
 * tool's words give the cause -- what it does said in front wherever the
 * words do not say it -- 「未能更新」 where they do not, and what the
 * tool or macOS wrote only with "Show technical details" on, since it is
 * right below, in the log; what to do next about an outcome that needs it
 * -- the next step for a failure whose cause the tool's own words give
 * (`outcomeCause`), or the outcome's own -- as its text; then everything
 * the tool printed, in its own words, in
 * a grouped container in 11/14 monospace, what it wrote to stderr in red,
 * keeping to its end while more arrives, with Banager's own notes among
 * the lines as plain sentences; under it, where it holds a tool's words
 * for a failure, whose words they are and what to do next, Copy Log among
 * it (`FailureNextStep`) -- or, with technical details on and none of the
 * tool's lines left in this window's log, under the subtitle that still
 * has its words (`SubtitleStep`). That text selects, as nothing else in the
 * dialog does (`select-text`), and Copy Log puts all of it on the clipboard,
 * to be pasted into a search or a report of what went wrong. While the
 * operation can still be stopped, a button beside Close stops it: the page
 * under the dialog is out of reach while it is open, the operation bar's
 * Cancel with it.
 *
 * A modal dialog: Escape, its default button or a click beside it closes it, Tab stays
 * inside, and the focus goes back to what opened it. It opens by itself
 * when an uninstall starts, so the focus lands on the dialog, not on Done.
 *
 * Opened by the operation bar's 「查看N个日志」 (`logRun`), it steps through
 * each operation of the run that needs a look: over the log, which one of
 * how many it shows -- 「第2个，共6个」 -- with Previous and Next, small and
 * grey (`LogRunStepper`; walk-2 W2-4).
 */
export function LogDrawer() {
  const { t } = useTranslation();
  const drawerOpen = useUiStore((s) => s.drawerOpen);
  const setDrawerOpen = useUiStore((s) => s.setDrawerOpen);
  const focusedOpId = useUiStore((s) => s.focusedOpId);
  const logRun = useUiStore((s) => s.logRun);
  const stepLogRun = useUiStore((s) => s.stepLogRun);
  const logs = useUiStore((s) => s.logs);
  const { data: operations } = useOperations();
  const { data: settings } = useSettings();
  const technical = settings?.show_technical_details ?? false;
  const nameOf = useOperationName(operations);
  const cancelMutation = useCancelOperation();
  const { status: copyStatus, copy } = useCopyCommand();
  const viewportRef = useRef<HTMLDivElement>(null);
  const [stickToBottom, setStickToBottom] = useState(true);

  const visibleLogs = logs.filter((l) => l.opId === focusedOpId);
  const operation = (operations ?? []).find((op) => op.id === focusedOpId);

  // Another operation's log -- a step through a run, or another row's --
  // starts at its end, where the tool's error is, however far up the last
  // one was scrolled.
  useEffect(() => {
    setStickToBottom(true);
  }, [focusedOpId]);

  // Opening counts too: the log may already be long when the dialog opens.
  useEffect(() => {
    const viewport = viewportRef.current;
    if (drawerOpen && viewport && stickToBottom) {
      viewport.scrollTop = viewport.scrollHeight;
    }
  }, [drawerOpen, visibleLogs.length, stickToBottom, focusedOpId]);

  function handleScroll(event: UIEvent<HTMLDivElement>) {
    const el = event.currentTarget;
    const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
    setStickToBottom(distanceFromBottom <= NEAR_BOTTOM_PX);
  }

  /** The log as text, a line each, Banager's notes in the user's words. */
  const logText = () =>
    visibleLogs.map((line) => ("note" in line ? noteText(t, line.note) : line.line)).join("\n");

  /** The subtitle, the next step and the Cancel button for one operation. */
  function partsOf(op: OpSummary) {
    const status = statusKey(op, logs);
    const done = op.status === "Done" && op.outcome !== null;
    const cause = done ? outcomeCause(op.outcome) : null;
    const detailKey = done && op.outcome !== null ? outcomeDetailKey(op.outcome) : null;
    const cancel = cancelState(op);
    const words = operationWords(t, op, logs, technical);
    return {
      title: nameOf(op),
      // The title and the subtitle as one line, as the operation bar says
      // them -- 「git：更新 · 网络连接失败」 -- for a screen reader stepping through a run.
      line: t("operations.current", { name: nameOf(op), status: words }),
      // Where it stands while under way; once done, how it ended.
      subtitle:
        status !== null ? (
          <span className="inline-flex items-center gap-1.5">
            <SpinnerIcon size={12} className="shrink-0" />
            {words}
          </span>
        ) : (
          <span className="inline-flex items-start gap-1">
            <OutcomeIcon tone={outcomeTone(op.outcome)} size={12} className="mt-px" />
            <span className="min-w-0 break-words">{words}</span>
          </span>
        ),
      // What to do now, in one sentence: the cause's next step where the
      // tool's words give one, else the outcome's own, else nothing.
      next: cause !== null ? t(FAILURE_CAUSE_KEYS[cause].next) : detailKey === null ? null : t(detailKey),
      stop:
        cancel === "none" ? null : (
          <button
            type="button"
            onClick={() => cancelMutation.mutate(op.id)}
            disabled={cancel === "disabled"}
            className={BUTTON.large.grey}
          >
            {t(OP_CANCEL_KEYS[op.kind])}
          </button>
        ),
    };
  }

  const parts = operation === undefined ? null : partsOf(operation);
  const nextId = useId();
  const stepId = useId();
  // The sentence under the log, where a tool's own words are in it.
  const step = operation === undefined ? null : failureLogStep(operation, logs);
  // The same, under the subtitle, where only the subtitle still has them.
  const overStep = operation === undefined ? null : subtitleStep(operation, logs, technical);
  const overStepId = useId();
  const copyWords = copyStatusText(t, copyStatus);
  // Done once the operation has ended; until then Close, which is all the
  // button does: beside 「取消卸载」 and over 「正在卸载…」, a 「完成」
  // read as the operation being finished (walk-3 W3-4). Close too before
  // the list has it, a moment after it started.
  const ended = operation !== undefined && !isActive(operation);

  return (
    <Dialog
      open={drawerOpen}
      onOpenChange={(open) => {
        if (!open) setDrawerOpen(false);
      }}
      width="log"
      // Before the list of operations has it -- a moment after an
      // operation starts -- the dialog is simply the operation log.
      title={parts?.title ?? t("operations.logDrawerTitle")}
      subtitle={parts?.subtitle}
      // What to do next, where the log says: said after its subtitle as it
      // opens -- the cause's step over the log, else the one under it.
      describedBy={parts?.next ? nextId : step !== null ? stepId : overStep !== null ? overStepId : undefined}
      focusSelf
      fillBody
      footerStart={
        <>
          <button
            type="button"
            onClick={() => copy(logText())}
            disabled={visibleLogs.length === 0}
            className={BUTTON.large.grey}
          >
            {t("operations.copyLog")}
          </button>
          <span role="status" className="text-small text-muted">
            {copyWords}
          </span>
        </>
      }
      footer={
        <>
          {parts?.stop}
          <button type="button" onClick={() => setDrawerOpen(false)} className={BUTTON.large.default}>
            {ended ? t("common.done") : t("common.close")}
          </button>
        </>
      }
    >
      {focusedOpId !== null ? (
        <LogRunStepper run={logRun} at={focusedOpId} title={parts?.line ?? null} onStep={stepLogRun} />
      ) : null}
      {parts?.next ? (
        <p id={nextId} className="mb-3 break-words text-body text-foreground">
          {parts.next}
        </p>
      ) : null}
      {/* With technical details on, where the log no longer has the
          tool's own words, but the subtitle does: whose they are. */}
      {operation !== undefined ? (
        <SubtitleStep op={operation} logs={logs} technical={technical} id={overStepId} />
      ) : null}
      {/* Where sudo wanted a password: the command to run in Terminal. */}
      {operation !== undefined ? <PasswordCommand op={operation} /> : null}
      <ScrollArea
        className="min-h-0 flex-1 overflow-hidden rounded-group bg-group"
        ref={viewportRef}
        onViewportScroll={handleScroll}
      >
        <div
          role="log"
          aria-label={t("operations.logDrawerTitle")}
          className="flex min-h-40 select-text flex-col px-2.5 py-2 font-mono text-small text-foreground"
        >
          {visibleLogs.map((line) =>
            "note" in line ? (
              // Banager's own voice, set apart from the tool's output so
              // nobody mistakes it for something the tool said: a sentence
              // in the window's own type, marked at its side.
              <p
                key={line.seq}
                className="my-1 break-words border-l-2 border-accent/50 pl-2 font-sans text-small text-foreground"
              >
                {noteText(t, line.note)}
              </p>
            ) : (
              <p
                key={line.seq}
                className={`whitespace-pre-wrap break-words ${line.stream === "Stderr" ? "text-danger-text" : ""}`}
              >
                {line.line}
              </p>
            ),
          )}
        </div>
      </ScrollArea>
      {/* Under the tool's own words: whose they are, and what to do next. */}
      {operation !== undefined ? <FailureNextStep op={operation} logs={logs} id={stepId} /> : null}
    </Dialog>
  );
}

/**
 * Where the log is in a run it steps through (`logRun`), over the log:
 * 「第2个，共6个」 in the muted grey, and Previous and Next, small and grey,
 * at its right, each off at its end of the run. Nothing for a log of one
 * operation, or one the run does not hold. The focus stays on a button:
 * where the one pressed turns off at an end, it moves to the other.
 *
 * A screen reader hears each step whole, in one polite announcement --
 * where it is and which log, 「第2个，共6个，git：未能更新」 (`title`, the
 * dialog's title and subtitle, which change with no announcement of their own) -- and the
 * buttons by what they move between, 「上一个日志」 (walk-2 review 2.1).
 */
function LogRunStepper({
  run,
  at,
  title,
  onStep,
}: {
  run: number[];
  at: number;
  title: string | null;
  onStep: (id: number) => void;
}) {
  const { t } = useTranslation();
  const previousRef = useRef<HTMLButtonElement>(null);
  const nextRef = useRef<HTMLButtonElement>(null);
  // The button to take the focus once the step is drawn: the other one
  // can only take it once it is on.
  const refocus = useRef<"previous" | "next" | null>(null);
  const index = run.indexOf(at);
  useEffect(() => {
    const which = refocus.current;
    refocus.current = null;
    if (which === "previous") previousRef.current?.focus();
    else if (which === "next") nextRef.current?.focus();
  }, [index]);
  if (run.length < 2 || index < 0) return null;
  const step = (to: number) => {
    if (to === 0) refocus.current = "next";
    else if (to === run.length - 1) refocus.current = "previous";
    onStep(run[to]);
  };
  return (
    <div data-log-run="" className="mb-3 flex items-center gap-2">
      <span role="status" className="min-w-0 flex-1 text-small text-muted">
        {t("failureSteps.position", { current: index + 1, total: run.length })}
        {title === null ? null : (
          <span className="sr-only">
            {t("overview.listSeparator")}
            {title}
          </span>
        )}
      </span>
      <button
        ref={previousRef}
        type="button"
        aria-label={t("failureSteps.previousLabel")}
        disabled={index === 0}
        onClick={() => step(index - 1)}
        className={BUTTON.small.grey}
      >
        {t("failureSteps.previous")}
      </button>
      <button
        ref={nextRef}
        type="button"
        aria-label={t("failureSteps.nextLabel")}
        disabled={index === run.length - 1}
        onClick={() => step(index + 1)}
        className={BUTTON.small.grey}
      >
        {t("failureSteps.next")}
      </button>
    </div>
  );
}
