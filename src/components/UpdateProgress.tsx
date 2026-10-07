import { useCallback, useId, useMemo, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useOperations, useSettings, useSnapshot } from "../lib/queries";
import { operationWords } from "../lib/operations";
import { actionableUpdatesOf, countedUpdatesOf } from "../lib/updateState";
import { artifactKeyId, useUiStore } from "../store/ui";
import type { OpSummary, Outcome, UpdateCandidate } from "../lib/types";
import { FAILURE_CAUSE_KEYS, outcomeCause, type FailureCause } from "../lib/failureCause";
import { CheckIcon, InfoIcon, SpinnerIcon, WarningFilledIcon } from "./icons";

/**
 * What a row shows in place of its Update button while an update of it is
 * under way or has just finished (`UpdateProgress`). `failed` and `check`
 * keep the operation's id, for the log they offer; `failed`, why, where
 * the tool's own words say (`outcomeCause`), or null.
 */
export type RowProgress =
  | { kind: "pendingAction"; operation: OpSummary }
  | { kind: "queued" }
  | { kind: "running" }
  | { kind: "cancelling" }
  | { kind: "succeeded" }
  | { kind: "cancelled" }
  | { kind: "failed"; opId: number; cause: FailureCause | null }
  | { kind: "check"; opId: number };

/**
 * How a finished update ended, for its row. "Check" -- look at the log --
 * for every outcome that is neither a plain success nor a plain failure:
 * the tool said it worked and Banager could not confirm it (`Unconfirmed`)
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
  if ("Failed" in outcome || "BanagerFailed" in outcome) return { kind: "failed", opId, cause: outcomeCause(outcome) };
  const unhandled: never = outcome;
  return unhandled;
}

/**
 * Whether an update ended without updating -- it failed, was cancelled, or
 * asks to be checked -- so that its row, where it still offers Update,
 * offers Retry beside how it ended. A tick asks for nothing, and one still
 * under way has its own Cancel in the operation bar.
 *
 * Not one that stopped where sudo wanted the Mac's password with no way
 * to ask (`needsPassword`): the same command from Banager stops there
 * again, as Homebrew resets sudo's remembered password before every
 * command and Banager has no terminal. Its row keeps how it ended, the
 * word that opens the log with the command for Terminal, in Retry's
 * place, and like a row an update holds it has no checkbox and is left
 * out of Select all and Update all, until a check finds it updated or
 * offers a newer version. One whose password window got no password
 * (`passwordNotAccepted`) can ask again, and keeps Retry.
 */
export function isRetryable(progress: RowProgress): boolean {
  if (progress.kind === "failed") return passwordStepsOpId(progress) === null;
  return progress.kind === "cancelled" || progress.kind === "check";
}

/**
 * The operation of an update that stopped where sudo wanted the Mac's
 * password with no way to ask (`needsPassword`), whose log has the steps
 * for Terminal; null for any other progress. The one test of it for a row:
 * no Retry (`isRetryable`), 「查看步骤」 in Retry's place, and the status
 * column that word stands in.
 */
export function passwordStepsOpId(progress: RowProgress | null): number | null {
  return progress !== null && progress.kind === "failed" && progress.cause === "needsPassword" ? progress.opId : null;
}

/**
 * Whether a row's update stopped where sudo wanted the Mac's password
 * (`passwordStepsOpId`): what the Overview counts as 「需要输入密码」
 * (`updatesSummary`), as the Updates page's headline does.
 */
export function waitsForPassword(op: OpSummary | null): boolean {
  return op !== null && passwordStepsOpId(progressOf(op)) !== null;
}

/**
 * Whether an update takes its row: one still under way, one that worked
 * and stands in the row as "Updated" until the next refresh drops it, or
 * one that needs a password Banager cannot ask for (`isRetryable`). Such
 * a row has no checkbox, and Select all, Invert selection, Update all and
 * the Overview's Review updates leave it out: a second update could only
 * queue the same one behind it. One that ended without updating
 * (`isRetryable`) leaves the row selectable, with Retry.
 */
export function holdsRow(op: OpSummary | null): boolean {
  return op !== null && !isRetryable(progressOf(op));
}

/** Whether an update is still going: queued, running, being cancelled or read back. */
export function isUnderway(op: OpSummary | null): boolean {
  return op !== null && op.kind === "Upgrade" && op.status !== "Done";
}

/** Where an update stands, from its operation. A `switch` with no default, so a new status fails `tsc`. */
export function progressOf(op: OpSummary): RowProgress {
  if (op.kind !== "Upgrade" && op.status !== "Done") return { kind: "pendingAction", operation: op };
  switch (op.status) {
    case "Queued":
      return { kind: "queued" };
    // Verifying is the update's own last step: the command has ended and
    // Banager is reading the result back.
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
 * Any unfinished action holds the row by full artifact key; completed
 * outcomes below are matched only for upgrades.
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
 * was submitted): "Updated" or "Update failed" is about that version, and a
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
      if (op.kind !== "Upgrade" && op.status === "Done") continue;
      const id = artifactKeyId({ instance_id: op.instance_id, kind: op.artifact_kind, name: op.name });
      const seen = byKey.get(id);
      // Any unfinished action takes priority over a completed update.
      if (seen === undefined || (seen.status === "Done" && op.status !== "Done") ||
          ((seen.status === "Done") === (op.status === "Done") && op.id > seen.id)) byKey.set(id, op);
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

/**
 * The updates that can be started now: every one the Updates page offers
 * to install (`actionableUpdatesOf`) whose row no update takes
 * (`holdsRow`) -- the rows that show a checkbox, and what its Select all
 * and Invert selection tick -- or undefined until the snapshot and the
 * settings have both arrived. What Update all takes, and every number,
 * is these less the copies Terminal does not run (`useCountedUpdates`).
 */
export function useStartableUpdates(): UpdateCandidate[] | undefined {
  const { data: snapshot } = useSnapshot();
  const { data: settings } = useSettings();
  const operationFor = useUpdateOperationFor();
  // What the page offers to install moves with the snapshot and the
  // settings only; the operations, which move at every step of every
  // update, only take rows out of it.
  const actionable = useMemo(
    () => (snapshot && settings ? actionableUpdatesOf(snapshot, settings) : undefined),
    [snapshot, settings],
  );
  return useMemo(
    () => actionable?.filter((candidate) => !holdsRow(operationFor(candidate))),
    [actionable, operationFor],
  );
}

/**
 * The updates Update all would take now: those that can be started
 * (`useStartableUpdates`) but the update of a copy Terminal does not run,
 * whose row keeps its checkbox, unticked (`countedUpdatesOf`, decision
 * U4) -- the page's "N updates", what its Update all hands on -- or
 * undefined until the snapshot and the settings have both arrived. The
 * Updates page and the update notification's report
 * (`useUpdateNotification` in src/lib/updateNotification.ts) read this
 * one hook, so the notification cannot count a row Update all would not
 * take.
 */
export function useCountedUpdates(): UpdateCandidate[] | undefined {
  const { data: snapshot } = useSnapshot();
  const { data: settings } = useSettings();
  const operationFor = useUpdateOperationFor();
  // As `useStartableUpdates`: the snapshot and the settings decide what
  // is counted, the operations only take rows out of it.
  const counted = useMemo(
    () => (snapshot && settings ? countedUpdatesOf(snapshot, settings) : undefined),
    [snapshot, settings],
  );
  return useMemo(
    () => counted?.filter((candidate) => !holdsRow(operationFor(candidate))),
    [counted, operationFor],
  );
}

/**
 * How many updates can be started now (`useCountedUpdates`), or
 * undefined until the snapshot and the settings have both arrived: the
 * number beside the sidebar's Updates and on the Dock's badge
 * (`useDockBadge`), and the Updates page's 「N 个可更新」. Those an update
 * is installing are not counted: the page says 「正在更新 N 个工具」 of them
 * in words, and a number beside it that counted them too would disagree
 * with its own. Nor is the update of a copy Terminal does not run, which
 * the page says apart (「1个终端用不到」). One rule, so no two of them can
 * show different numbers.
 */
export function useUpdateCount(): number | undefined {
  return useCountedUpdates()?.length;
}

/** Whatever `useTranslation()`'s `t` needs here. */
type Translate = (key: string, options?: Record<string, unknown>) => string;

/**
 * What a row's progress says, in words (`UpdateProgress`): 「正在更新…」,
 * 「已更新」, 「未能更新」 or why, where the tool's own words say. The row
 * names itself with it where it stands in the status word's place.
 */
export function progressWord(t: Translate, progress: RowProgress): string {
  switch (progress.kind) {
    case "pendingAction":
      return operationWords(t, progress.operation, [], false);
    case "queued":
      return t("updates.progress.queued");
    case "running":
      return t("updates.progress.running");
    case "cancelling":
      return t("updates.progress.cancelling");
    case "succeeded":
      return t("updates.progress.succeeded");
    case "cancelled":
      return t("updates.progress.cancelled");
    case "failed":
      return progress.cause === null ? t("updates.progress.failed") : t(FAILURE_CAUSE_KEYS[progress.cause].word);
    case "check":
      return t("updates.progress.check");
  }
}

export interface UpdateProgressProps {
  progress: RowProgress;
  /** The row's name, for "View log"'s accessible name. */
  name: string;
  onViewLog: (opId: number) => void;
}

/**
 * The row's own progress, where its Update button was (spec §3.10): 360's
 * "the progress is in the row", in a Mac list's words. Waiting, in the
 * muted grey; updating, a 16 spinner and 「正在更新…」, muted; done, a 12
 * green ✓ and 「已更新」. How it ended when it did not update is a word
 * that opens its log: 「未能更新」 in the red for text -- or why, where the
 * tool's own words say, 「网络连接失败」 (`failureCause`) -- and a 12
 * orange ⚠︎ and 「结果不符」 where the result is not what the tool said.
 * An ending the row can retry (`isRetryable`) stands in the status column,
 * beside the row's Retry, which takes the button's place. Such a word has
 * an ⓘ after it, as every status word with a why has (`StatusChip`): red
 * words alone did not read as something to press, and 「未能更新」 with no
 * way to see why left only Retry (walk-2 W2-4).
 */
export function UpdateProgress({ progress, name, onViewLog }: UpdateProgressProps) {
  const { t } = useTranslation();
  const wordId = useId();
  // The word is what shows; the button's name says what pressing it does,
  // and the word is its description, so a screen reader says both.
  const toLog = (opId: number, word: string, className: string, symbol?: ReactNode) => (
    <button
      type="button"
      onClick={() => onViewLog(opId)}
      aria-label={t("updates.progress.viewLogLabel", { name })}
      aria-describedby={wordId}
      title={t("common.viewLog")}
      className={`inline-flex items-center gap-1 whitespace-nowrap rounded-sm text-small ${className}`}
    >
      {symbol}
      <span id={wordId}>{word}</span>
      <InfoIcon size={12} className="shrink-0" />
    </button>
  );
  const word = progressWord(t, progress);
  switch (progress.kind) {
    case "pendingAction":
    case "queued":
      return <span className="whitespace-nowrap text-small text-muted">{word}</span>;
    case "running":
    case "cancelling":
      return (
        <span className="inline-flex items-center gap-1.5 whitespace-nowrap text-small text-muted">
          <SpinnerIcon size={16} className="shrink-0" />
          {word}
        </span>
      );
    case "succeeded":
      return (
        <span className="inline-flex items-center gap-1 whitespace-nowrap text-small text-foreground">
          <CheckIcon size={12} className="shrink-0 text-success" />
          {word}
        </span>
      );
    case "cancelled":
      return <span className="whitespace-nowrap text-small text-muted">{word}</span>;
    case "failed":
      return toLog(progress.opId, word, "text-danger-text");
    case "check":
      return toLog(
        progress.opId,
        word,
        "text-foreground",
        <WarningFilledIcon size={12} className="shrink-0 text-warning" />,
      );
  }
}
