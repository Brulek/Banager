import { useEffect, useRef, useState, type UIEvent } from "react";
import { useTranslation } from "react-i18next";
import type { TFunction } from "i18next";
import type { LogNote, OpSummary } from "../lib/types";
import { useUiStore } from "../store/ui";
import { useCancelOperation, useOperations, useSnapshot } from "../lib/queries";
import { outcomeDetailKey } from "../lib/format";
import {
  OP_CANCEL_KEYS,
  OP_KIND_KEYS,
  cancelState,
  outcomeSentence,
  outcomeTone,
  statusKey,
  useOperationName,
} from "../lib/operations";
import { adapterIdOf, adapterLabel } from "../lib/sources";
import { Drawer } from "./ui/Drawer";
import { SHEET_BUTTON } from "./ui/Dialog";
import { ScrollArea } from "./ui/ScrollArea";
import { ToolAvatar } from "./ToolAvatar";
import { OutcomeIcon } from "./OutcomeIcon";
import { SpinnerIcon } from "./icons";

const NEAR_BOTTOM_PX = 32;

/**
 * The words for one of Canager's own log notes, in the user's language.
 * Each `LogNote` variant needs a case here: the log is the one place the
 * app shows text it did not write, and a remark of Canager's that arrived
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
 * One operation's log, in a panel that comes in from the window's right
 * edge over the page, like a tool's details (`Drawer`): what it is
 * (「更新 ffmpeg」) and where it stands or how it ended in its header;
 * what to do next about an outcome that needs it under that; then
 * everything the tool printed, in its own words, in a softly tinted panel
 * that keeps to its end while more arrives -- with Canager's own notes
 * among the lines as plain sentences. While the operation can still be
 * stopped, the one button at its foot stops it: the page under the panel
 * is out of reach while it is open, the operation bar's Cancel with it.
 *
 * A modal dialog, as the details drawer is: Escape, the close button or a
 * click beside it closes it, Tab stays inside, and the focus goes back to
 * what opened it. It opens by itself when an uninstall starts, so the
 * focus lands on the panel, not on its close button.
 */
export function LogDrawer() {
  const { t } = useTranslation();
  const drawerOpen = useUiStore((s) => s.drawerOpen);
  const setDrawerOpen = useUiStore((s) => s.setDrawerOpen);
  const focusedOpId = useUiStore((s) => s.focusedOpId);
  const logs = useUiStore((s) => s.logs);
  const { data: operations } = useOperations();
  const { data: snapshot } = useSnapshot();
  const nameOf = useOperationName(operations);
  const cancelMutation = useCancelOperation();
  const viewportRef = useRef<HTMLDivElement>(null);
  const [stickToBottom, setStickToBottom] = useState(true);

  const visibleLogs = logs.filter((l) => l.opId === focusedOpId);
  const operation = (operations ?? []).find((op) => op.id === focusedOpId);

  // Opening counts too: the log may already be long when the drawer opens.
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

  /** The header, the next step and the foot for one operation. */
  function partsOf(op: OpSummary) {
    // The avatar the tool's row has -- its app's icon, or its source's:
    // the adapter from the snapshot, or from the id for a source the
    // snapshot no longer has.
    const adapterId =
      snapshot?.instances?.find((instance) => instance.id === op.instance_id)?.adapter_id ??
      adapterIdOf(op.instance_id);
    const status = statusKey(op, logs);
    const detailKey = op.status === "Done" && op.outcome !== null ? outcomeDetailKey(op.outcome) : null;
    const cancel = cancelState(op);
    return {
      title: t("operations.title", { kind: t(OP_KIND_KEYS[op.kind]), name: nameOf(op) }),
      leading: (
        <ToolAvatar
          adapterId={adapterId}
          sourceLabel={adapterLabel(t, adapterId)}
          iconKey={{ instance_id: op.instance_id, kind: op.artifact_kind, name: op.name }}
        />
      ),
      // Where it stands while under way; once done, how it ended.
      subtitle:
        status !== null ? (
          <span className="inline-flex items-center gap-1.5">
            <SpinnerIcon size={13} className="shrink-0 text-accent-text" />
            {t(status)}
          </span>
        ) : (
          <span className="inline-flex items-start gap-1.5">
            <OutcomeIcon tone={outcomeTone(op.outcome)} size={14} className="mt-px" />
            <span className="min-w-0 break-words">{outcomeSentence(t, op.outcome)}</span>
          </span>
        ),
      detail: detailKey === null ? null : t(detailKey),
      footer:
        cancel === "none" ? undefined : (
          <button
            type="button"
            onClick={() => cancelMutation.mutate(op.id)}
            disabled={cancel === "disabled"}
            className={SHEET_BUTTON.secondary}
          >
            {t(OP_CANCEL_KEYS[op.kind])}
          </button>
        ),
    };
  }

  const parts = operation === undefined ? null : partsOf(operation);

  return (
    <Drawer
      open={drawerOpen}
      onOpenChange={(open) => {
        if (!open) setDrawerOpen(false);
      }}
      // Before the list of operations has it -- a moment after an
      // operation starts -- the drawer is simply the operation log.
      title={parts?.title ?? t("operations.logDrawerTitle")}
      subtitle={parts?.subtitle}
      leading={parts?.leading}
      closeLabel={t("common.close")}
      footer={parts?.footer}
      focusPanelOnOpen
      fillBody
    >
      {parts?.detail ? <p className="mb-3 break-words text-body text-foreground">{parts.detail}</p> : null}
      <ScrollArea
        className="min-h-0 flex-1 overflow-hidden rounded-row bg-hover/60"
        ref={viewportRef}
        onViewportScroll={handleScroll}
      >
        <div
          role="log"
          aria-label={t("operations.logDrawerTitle")}
          className="flex flex-col gap-0.5 px-3 py-2.5 font-mono text-small text-foreground"
        >
          {visibleLogs.map((line) =>
            "note" in line ? (
              // Canager's own voice, set apart from the tool's output so
              // nobody mistakes it for something the tool said: a sentence
              // in the window's own type, marked at its side.
              <p
                key={line.seq}
                className="my-1 break-words border-l-2 border-accent/50 pl-2 font-sans text-body text-foreground"
              >
                {noteText(t, line.note)}
              </p>
            ) : (
              <p
                key={line.seq}
                className={`whitespace-pre-wrap break-words ${line.stream === "Stderr" ? "text-danger" : ""}`}
              >
                {line.line}
              </p>
            ),
          )}
        </div>
      </ScrollArea>
    </Drawer>
  );
}
