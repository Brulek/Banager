import { useCallback, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { useOperations } from "../lib/queries";
import { artifactKeyId, useUiStore } from "../store/ui";
import type { OpSummary, Outcome, UpdateCandidate } from "../lib/types";
import { CheckIcon, SpinnerIcon } from "./icons";

/**
 * What a row shows in place of its Update button while an update of it is
 * under way or has just finished (`UpdateProgress`). `failed` and `check`
 * keep the operation's id, for the log they offer.
 */
export type RowProgress =
  | { kind: "queued" }
  | { kind: "running" }
  | { kind: "cancelling" }
  | { kind: "succeeded" }
  | { kind: "cancelled" }
  | { kind: "failed"; opId: number }
  | { kind: "check"; opId: number };

/**
 * How a finished update ended, for its row. "Check" -- look at the log --
 * for every outcome that is neither a plain success nor a plain failure:
 * the tool said it worked and Canager could not confirm it (`Unconfirmed`)
 * or found the opposite (`NeedsAttention`). A finished operation with no
 * outcome, which the backend never sends, claims nothing either way.
 */
function outcomeProgress(outcome: Outcome | null, opId: number): RowProgress {
  if (outcome === null) return { kind: "check", opId };
  if (typeof outcome === "string") {
    switch (outcome) {
      case "Succeeded":
        return { kind: "succeeded" };
      case "Cancelled":
        return { kind: "cancelled" };
      case "Unconfirmed":
        return { kind: "check", opId };
      default: {
        const unhandled: never = outcome;
        return unhandled;
      }
    }
  }
  if ("NeedsAttention" in outcome) return { kind: "check", opId };
  if ("Failed" in outcome || "CanagerFailed" in outcome) return { kind: "failed", opId };
  const unhandled: never = outcome;
  return unhandled;
}

/**
 * Whether an update ended without updating -- it failed, was cancelled, or
 * asks to be checked -- so that its row, where it still offers Update,
 * offers Retry beside how it ended. A tick asks for nothing, and one still
 * under way has its own Cancel in the operation bar.
 */
export function isRetryable(progress: RowProgress): boolean {
  return progress.kind === "failed" || progress.kind === "cancelled" || progress.kind === "check";
}

/** Where an update stands, from its operation. A `switch` with no default, so a new status fails `tsc`. */
export function progressOf(op: OpSummary): RowProgress {
  switch (op.status) {
    case "Queued":
      return { kind: "queued" };
    // Verifying is the update's own last step: the command has ended and
    // Canager is reading the result back.
    case "Running":
    case "Verifying":
      return { kind: "running" };
    case "CancelRequested":
    case "Cancelling":
      return { kind: "cancelling" };
    case "Done":
      return outcomeProgress(op.outcome, op.id);
  }
}

/**
 * The operation an update's row shows in place of its Update button, or
 * null -- on the Updates page's row and in the Installed page's detail
 * alike.
 *
 * The backend keeps a finished operation in its list, so an operation is
 * matched to a row by its key -- instance, kind and name, the newest one
 * -- and, once it has finished, by the version the row offers too. One
 * still under way, always: a second click could only queue the same
 * update behind it. A finished one only while the row still offers the
 * version it was started for (`updateTargets`, remembered when the update
 * was submitted): "Updated" or "Failed" is about that version, and a
 * newer one the source offers later gets its button back. An operation
 * this window has no record of -- one from before it was reloaded -- is
 * not shown once it has finished.
 */
export function useUpdateOperationFor(): (candidate: UpdateCandidate) => OpSummary | null {
  const { data: operations } = useOperations();
  const updateTargets = useUiStore((s) => s.updateTargets);
  const latestUpdateOp = useMemo(() => {
    const byKey = new Map<string, OpSummary>();
    for (const op of operations ?? []) {
      if (op.kind !== "Upgrade") continue;
      const id = artifactKeyId({ instance_id: op.instance_id, kind: op.artifact_kind, name: op.name });
      const seen = byKey.get(id);
      if (seen === undefined || op.id > seen.id) byKey.set(id, op);
    }
    return byKey;
  }, [operations]);
  return useCallback(
    (candidate: UpdateCandidate) => {
      const op = latestUpdateOp.get(artifactKeyId(candidate.key));
      if (op === undefined) return null;
      if (op.status !== "Done") return op;
      return updateTargets[op.id] === candidate.target ? op : null;
    },
    [latestUpdateOp, updateTargets],
  );
}

export interface UpdateProgressProps {
  progress: RowProgress;
  /** The row's name, for "View log"'s accessible name. */
  name: string;
  onViewLog: (opId: number) => void;
}

/**
 * The row's own progress, where its Update button was: 360's "the progress
 * is in the row". Waiting, updating with a spinner, a tick when it is
 * done; a failure, or an outcome to check, with the way to its log. An
 * ending the row can retry (`isRetryable`) stands beside the row's Retry,
 * which takes the button's place.
 */
export function UpdateProgress({ progress, name, onViewLog }: UpdateProgressProps) {
  const { t } = useTranslation();
  const viewLog = (opId: number) => (
    <button
      type="button"
      onClick={() => onViewLog(opId)}
      aria-label={t("updates.progress.viewLogLabel", { name })}
      className="rounded-sm text-small font-medium text-accent-text outline-none hover:underline focus-visible:ring-2 focus-visible:ring-accent"
    >
      {t("common.viewLog")}
    </button>
  );
  switch (progress.kind) {
    case "queued":
      return <span className="text-small text-muted">{t("updates.progress.queued")}</span>;
    case "running":
      return (
        <span className="inline-flex items-center gap-1.5 whitespace-nowrap text-small font-medium text-accent-text">
          <SpinnerIcon size={14} />
          {t("updates.progress.running")}
        </span>
      );
    case "cancelling":
      return (
        <span className="inline-flex items-center gap-1.5 whitespace-nowrap text-small text-muted">
          <SpinnerIcon size={14} />
          {t("updates.progress.cancelling")}
        </span>
      );
    case "succeeded":
      return (
        <span className="inline-flex items-center gap-1 whitespace-nowrap text-small font-medium text-success">
          <CheckIcon size={15} />
          {t("updates.progress.succeeded")}
        </span>
      );
    case "cancelled":
      return <span className="text-small text-muted">{t("updates.progress.cancelled")}</span>;
    case "failed":
      return (
        <span className="flex flex-col items-end leading-tight">
          <span className="text-small font-medium text-danger">{t("updates.progress.failed")}</span>
          {viewLog(progress.opId)}
        </span>
      );
    case "check":
      return (
        <span className="flex flex-col items-end leading-tight">
          <span className="text-small font-medium text-warning">{t("updates.progress.check")}</span>
          {viewLog(progress.opId)}
        </span>
      );
  }
}
