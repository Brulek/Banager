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
  OP_KIND_KEYS,
  cancelState,
  outcomeTone,
  outcomeWords,
  statusKey,
  useOperationName,
} from "../lib/operations";
import { Dialog } from "./ui/Dialog";
import { BUTTON } from "./ui/controls";
import { ScrollArea } from "./ui/ScrollArea";
import { OutcomeIcon } from "./OutcomeIcon";
import { PasswordCommand } from "./PasswordCommand";
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
  const unhandled: never = note;
  return unhandled;
}

/**
 * One operation's log, as a dialog 560 wide (spec §3.10, R11: the log
 * answers an operation, so it stays a dialog): what it is (「更新ffmpeg」) as
 * its title, and where it stands or how it ended under that -- in the
 * words the operation bar uses (`outcomeWords`): 「网络连接失败」 where the
 * tool's words give the cause, 「未能完成」 where they do not, and what the
 * tool or macOS wrote only with "Show technical details" on, since it is
 * right below, in the log; what to do next about an outcome that needs it
 * -- the next step for a failure whose cause the tool's own words give
 * (`outcomeCause`), or the outcome's own -- as its text; then everything
 * the tool printed, in its own words, in
 * a grouped container in 11/14 monospace, what it wrote to stderr in red,
 * keeping to its end while more arrives, with Banager's own notes among
 * the lines as plain sentences. That text selects, as nothing else in the
 * dialog does (`select-text`), and Copy Log puts all of it on the clipboard,
 * to be pasted into a search or a report of what went wrong. While the
 * operation can still be stopped, a button beside Done stops it: the page
 * under the dialog is out of reach while it is open, the operation bar's
 * Cancel with it.
 *
 * A modal dialog: Escape, Done or a click beside it closes it, Tab stays
 * inside, and the focus goes back to what opened it. It opens by itself
 * when an uninstall starts, so the focus lands on the dialog, not on Done.
 */
export function LogDrawer() {
  const { t } = useTranslation();
  const drawerOpen = useUiStore((s) => s.drawerOpen);
  const setDrawerOpen = useUiStore((s) => s.setDrawerOpen);
  const focusedOpId = useUiStore((s) => s.focusedOpId);
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

  // Opening counts too: the log may already be long when the dialog opens.
  useEffect(() => {
    const viewport = viewportRef.current;
    if (drawerOpen && viewport && stickToBottom) {
      viewport.scrollTop = viewport.scrollHeight;
    }
  }, [drawerOpen, visibleLogs.length, stickToBottom]);

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
    return {
      title: t("operations.title", { kind: t(OP_KIND_KEYS[op.kind]), name: nameOf(op) }),
      // Where it stands while under way; once done, how it ended.
      subtitle:
        status !== null ? (
          <span className="inline-flex items-center gap-1.5">
            <SpinnerIcon size={12} className="shrink-0" />
            {t(status)}
          </span>
        ) : (
          <span className="inline-flex items-start gap-1">
            <OutcomeIcon tone={outcomeTone(op.outcome)} size={12} className="mt-px" />
            <span className="min-w-0 break-words">{outcomeWords(t, op.outcome, technical)}</span>
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
  const copyWords = copyStatusText(t, copyStatus);

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
      // What to do next, where the log says: said after its subtitle as it opens.
      describedBy={parts?.next ? nextId : undefined}
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
            {t("common.done")}
          </button>
        </>
      }
    >
      {parts?.next ? (
        <p id={nextId} className="mb-3 break-words text-body text-foreground">
          {parts.next}
        </p>
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
    </Dialog>
  );
}
