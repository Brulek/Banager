import { useCallback, useEffect, useMemo } from "react";
import { useSnapshot } from "./queries";
import { FAILURE_CAUSE_KEYS, outcomeCause } from "./failureCause";
import { outcomeArgs, outcomeKey } from "./format";
import { artifactKeyId, useUiStore, type LogLine } from "../store/ui";
import type { AlreadyUpdated, OpKind, OpStatus, OpSummary, Outcome } from "./types";

type Translate = (key: string, options?: Record<string, unknown>) => string;

/**
 * What the operation bar and the log drawer share about an operation: its
 * words, whether it can still be cancelled, how it ended, and which
 * operations the bar is about right now.
 */

/**
 * How an update already at its new version when its turn came is said, in
 * place of 「已更新」 (r6 y3-batch): `word` where the row's ending stands --
 * 「已由前面的更新一并完成」, "Done by an earlier update" -- and `why` behind
 * the ⓘ of 「最近的更新记录」. A `Record` over `AlreadyUpdated`, so a variant
 * added to the mirror without words fails `tsc`; spelled out, so the
 * reachability test finds every key.
 */
export const ALREADY_UPDATED_KEYS: Record<AlreadyUpdated, { word: string; why: string }> = {
  ByEarlierUpdate: {
    word: "batchResult.already.ByEarlierUpdate",
    why: "batchResult.alreadyWhy.ByEarlierUpdate",
  },
  BeforeItsTurn: {
    word: "batchResult.already.BeforeItsTurn",
    why: "batchResult.alreadyWhy.BeforeItsTurn",
  },
};

/** Whether an operation is still under way: anything but Done. */
export function isActive(op: OpSummary): boolean {
  return op.status !== "Done";
}

/**
 * What an operation does, as a noun -- 「更新」, "Update" -- said before
 * where it stands wherever those words do not say it themselves:
 * 「git：更新 · 排队中」, "git: Update · Connection failed"
 * (`operationWords`; walk-3 review 1.1). A `Record` over `OpKind`, so a
 * kind added to the mirror without a word here fails `tsc`.
 */
export const OP_KIND_KEYS: Record<OpKind, string> = {
  Install: "operations.kind.Install",
  Uninstall: "operations.kind.Uninstall",
  Upgrade: "operations.kind.Upgrade",
  // `brew link --formula --force`, a source notice's Fix… (src/lib/noAnswer.ts).
  Link: "noAnswer.op.kind",
};

/**
 * What an operation is doing while it runs, said of the tool it acts on
 * -- 「ffmpeg：正在更新…」, "htop: Uninstalling…" -- never a bare verb in
 * front of the name, which in English reads as a command ("Update
 * ffmpeg: Running"; walk-3 W3-3). An update's are the Updates row's own
 * words (`updates.progress.*`), so the row and the bar say one thing
 * (W3-19). A `Record` over `OpKind`, so a kind added to the mirror
 * without a word here fails `tsc`.
 */
export const OP_RUNNING_KEYS: Record<OpKind, string> = {
  Install: "operations.running.Install",
  Uninstall: "operations.running.Uninstall",
  Upgrade: "updates.progress.running",
  Link: "noAnswer.op.running",
};

/** How an operation that worked ended, in the same words: 「已更新」, "Uninstalled". */
export const OP_SUCCEEDED_KEYS: Record<OpKind, string> = {
  Install: "operations.succeeded.Install",
  Uninstall: "operations.succeeded.Uninstall",
  Upgrade: "updates.progress.succeeded",
  Link: "noAnswer.op.succeeded",
};

/**
 * How one that failed ended, where the tool's words give no cause: 「未能
 * 更新」 -- what the row says -- not a 「未能完成」 that names no kind
 * (walk-3 W3-10).
 */
export const OP_FAILED_KEYS: Record<OpKind, string> = {
  Install: "operations.failedShort.Install",
  Uninstall: "operations.failedShort.Uninstall",
  Upgrade: "updates.progress.failed",
  Link: "noAnswer.op.failed",
};

/**
 * The log drawer's Cancel, named for what it stops: at the foot of a
 * drawer, a bare 「取消」 would read as "close this".
 */
export const OP_CANCEL_KEYS: Record<OpKind, string> = {
  Install: "operations.cancelKind.Install",
  Uninstall: "operations.cancelKind.Uninstall",
  Upgrade: "operations.cancelKind.Upgrade",
  Link: "noAnswer.op.cancel",
};

/**
 * Where an operation under way stands. `Done` has no word of its own: once
 * an operation is done, its outcome takes the status's place (C2), so the
 * bar never says 「ffmpeg：已完成」 and then how it went. `Running` says
 * what it does (`OP_RUNNING_KEYS`).
 */
export const OP_STATUS_KEYS: Record<Exclude<OpStatus, "Done" | "Running">, string> = {
  Queued: "operations.status.Queued",
  CancelRequested: "operations.status.CancelRequested",
  Cancelling: "operations.status.Cancelling",
  Verifying: "operations.status.Verifying",
};

/**
 * Whether `op` has started and nothing can stop it now: a `NoCancel` plan
 * past Queued, which `cancelState` offers no Cancel for. It goes on to its
 * end, whatever the operation bar's Cancel does to the rest of its run.
 */
export function runsToItsEnd(op: OpSummary): boolean {
  return isActive(op) && op.cancel_policy === "NoCancel" && op.status !== "Queued";
}

/**
 * Whether Cancel is offered for `op`, and whether it can be pressed.
 *
 * `OperationManager::cancel` (ops/mod.rs) refuses a `NoCancel` op once it
 * is Running, so a Cancel button for one would promise something the
 * backend will not do. While it is still Queued nothing has started and
 * the backend accepts the cancel, so the button stays: without it the user
 * could not drop a NoCancel op waiting behind another op's lock. rustup's
 * `self update` and `self uninstall` produce `NoCancel`
 * (crates/banager-core/src/adapters/standalone/recipes.rs); the
 * confirmation said so before the click.
 *
 * Offered but not pressable while a cancel is already on its way, and
 * while Verifying: the command has already ended, and `cancel()` answers
 * `NotPending`, which the IPC reports as a silent Ok.
 */
export function cancelState(op: OpSummary): "none" | "enabled" | "disabled" {
  if (!isActive(op) || runsToItsEnd(op)) return "none";
  if (op.status === "CancelRequested" || op.status === "Cancelling" || op.status === "Verifying") {
    return "disabled";
  }
  return "enabled";
}

/**
 * Whether `op` is waiting for a `brew update` a refresh left running
 * (`BrewAdapter::wait_for_update`, up to `OP_UPDATE_WAIT`) -- which the
 * operation bar says instead of "running", since it is the only thing
 * visible before the log is opened, and the wait can last minutes.
 *
 * There is no separate `OpStatus` for this: `set_status` in `ops/mod.rs`
 * leaves the record's status at `Running` for the whole of `execute`,
 * including any time it spends inside `wait_for_update`. So this reads the
 * log the drawer renders: the note this operation's log most recently
 * carried is the wait starting, and nothing (no further `Log` or `Note`
 * event) has arrived since to say it ended.
 *
 * Only while `Running`: `cancel()` (`ops/mod.rs`) sets the record's status
 * straight to `CancelRequested` as soon as the user presses Cancel,
 * independent of and before `execute`/`wait_for_update` notice, so a
 * cancel pressed during the wait leaves this operation `CancelRequested`
 * (then briefly `Cancelling`) while the last log line is still the same
 * note. Without this guard that combination read as "waiting for
 * Homebrew" even though a cancel was in flight -- which told the user
 * their Cancel had not registered.
 */
export function isWaitingForBrewUpdate(op: OpSummary, logs: LogLine[]): boolean {
  if (op.status !== "Running") return false;
  for (let index = logs.length - 1; index >= 0; index -= 1) {
    const line = logs[index];
    if (line.opId !== op.id) continue;
    return "note" in line && "WaitingForBrewUpdate" in line.note;
  }
  return false;
}

/**
 * Words, and whether they say what the operation does themselves:
 * running (「正在卸载…」), a success (「已卸载」), a plain failure
 * (「未能卸载」), and what did not add up after it ("Reported removed",
 * "Update reported success"). The branch that picks the words says so,
 * so `operationWords` never works out again which branch it was.
 */
interface Kinded {
  words: string;
  namesKind: boolean;
}

/** `statusKey`, and whether its words name the kind: only running does. */
function statusChoice(op: OpSummary, logs: LogLine[]): Kinded | null {
  if (op.status === "Done") return null;
  if (op.status === "Running") {
    return isWaitingForBrewUpdate(op, logs)
      ? { words: "operations.status.waitingForBrewUpdate", namesKind: false }
      : { words: OP_RUNNING_KEYS[op.kind], namesKind: true };
  }
  return { words: OP_STATUS_KEYS[op.status], namesKind: false };
}

/**
 * The key for where an operation under way stands, the wait for Homebrew
 * included; null once it is done, when its outcome says how it went.
 */
export function statusKey(op: OpSummary, logs: LogLine[]): string | null {
  return statusChoice(op, logs)?.words ?? null;
}

/**
 * How an operation ended, for its icon and whether its log is offered:
 * `success` (a tick), `cancelled` (the user's own doing), `attention` --
 * the tool said it worked and Banager could not confirm it or found the
 * opposite -- and `failure`. A finished operation with no outcome, which
 * the backend never sends, claims nothing either way: `attention`, as the
 * row's progress calls it (`UpdateProgress`).
 */
export type OutcomeTone = "success" | "cancelled" | "attention" | "failure";

/**
 * How an operation of `kind` ended, in a few words: 「已更新」, 「未能完
 * 成：…」 with the tool's own words. A finished operation with no outcome
 * says the result is unconfirmed, which is all that can be said of it.
 */
export function outcomeSentence(t: Translate, outcome: Outcome | null, kind: OpKind): string {
  return sentenceChoice(t, outcome, kind).words;
}

function sentenceChoice(t: Translate, outcome: Outcome | null, kind: OpKind): Kinded {
  const shown: Outcome = outcome ?? "Unconfirmed";
  if (shown === "Succeeded") return { words: t(OP_SUCCEEDED_KEYS[kind]), namesKind: true };
  return {
    words: t(`operations.outcome.${outcomeKey(shown)}`, outcomeArgs(shown)),
    namesKind: typeof shown !== "string" && "NeedsAttention" in shown,
  };
}

/**
 * How an operation ended, in the operation bar's few words, for a person
 * (spec §3.10, R10): where it failed for a reason the tool's own words
 * give (`outcomeCause`), that reason -- 「网络连接失败」 -- and otherwise,
 * with "Show technical details" off, nothing another program wrote: a
 * failure is 「未能更新」 (`OP_FAILED_KEYS`), and a program that would not start says so
 * without macOS's own words for why. Those, and the tool's, are in the
 * log, and here too with the setting on (`outcomeSentence`).
 */
export function outcomeWords(t: Translate, outcome: Outcome | null, kind: OpKind, technical: boolean): string {
  return outcomeChoice(t, outcome, kind, technical).words;
}

/**
 * `outcomeWords`, and whether they name the kind: a failure's cause,
 * cancelled, unconfirmed, a program's own words or Banager's do not.
 */
function outcomeChoice(t: Translate, outcome: Outcome | null, kind: OpKind, technical: boolean): Kinded {
  if (outcome !== null && typeof outcome !== "string") {
    const cause = outcomeCause(outcome);
    if ("Failed" in outcome) {
      if (technical) return sentenceChoice(t, outcome, kind);
      if (cause !== null) return { words: t(FAILURE_CAUSE_KEYS[cause].word), namesKind: false };
      return outcome.Failed.summary.trim()
        ? { words: t(OP_FAILED_KEYS[kind]), namesKind: true }
        : sentenceChoice(t, outcome, kind);
    }
    if ("BanagerFailed" in outcome && !technical) {
      const fault = outcome.BanagerFailed;
      if (typeof fault !== "string" && "SpawnFailed" in fault) {
        return { words: t("operations.outcome.BanagerFailed.SpawnFailedShort"), namesKind: false };
      }
    }
  }
  return sentenceChoice(t, outcome, kind);
}

/**
 * Where `op` stands, or how it ended, in the words the operation bar and
 * the log sheet say after its name -- with what it does in front where
 * those words do not say it: 「正在卸载…」 and 「已更新」 as they are,
 * 「更新 · 排队中」, "Uninstall · Cancelled", "Update · No permission".
 * Without it the bar and the sheet, named for the tool alone (walk-3
 * W3-3), could not tell an update from an uninstall once one ended in
 * anything but its plain words (review 1.1).
 */
export function operationWords(t: Translate, op: OpSummary, logs: LogLine[], technical: boolean): string {
  // An update already at its new version when its turn came: done, and
  // said how, as 「最近的更新记录」 says it. Its words name what it did.
  if (op.status === "Done" && op.outcome === "Succeeded" && op.already_updated) {
    return t(ALREADY_UPDATED_KEYS[op.already_updated].word);
  }
  const status = statusChoice(op, logs);
  const { words, namesKind } =
    status !== null ? { ...status, words: t(status.words) } : outcomeChoice(t, op.outcome, op.kind, technical);
  return namesKind ? words : t("operations.kindStatus", { kind: t(OP_KIND_KEYS[op.kind]), status: words });
}

export function outcomeTone(outcome: Outcome | null): OutcomeTone {
  if (outcome === null) return "attention";
  if (typeof outcome === "string") {
    switch (outcome) {
      case "Succeeded":
        return "success";
      case "Cancelled":
        return "cancelled";
      case "Unconfirmed":
        return "attention";
      default: {
        const unhandled: never = outcome;
        return unhandled;
      }
    }
  }
  if ("NeedsAttention" in outcome) return "attention";
  if ("Failed" in outcome || "BanagerFailed" in outcome) return "failure";
  const unhandled: never = outcome;
  return unhandled;
}

/**
 * Which operations the bar is about: its run. Operations started while
 * others were still under way join their run -- an Update all is one run
 * however many operations it submits -- and the first one started once
 * everything has finished begins a new one, which the bar then shows in
 * place of the last run's result.
 *
 * The backend numbers operations in the order they were submitted and
 * lists every one this session has run, so a run is every operation
 * numbered above `floor`. `seen` is the highest number looked at so far,
 * and `open` whether anything was under way then: an operation numbered
 * above `seen` that turns up while nothing was under way begins a new
 * run, even when it had already finished by the time it was seen.
 */
export interface OperationRun {
  floor: number;
  seen: number;
  open: boolean;
}

/**
 * The run after looking at `operations`, or `previous` itself when
 * nothing changed -- so a component can keep it in state and update it
 * while rendering without looping.
 *
 * The first look has no memory of how the list came to be. It takes what
 * is still under way and everything started after it; with nothing under
 * way, the newest operation alone -- what the bar showed before it had
 * runs at all.
 */
export function trackRun(previous: OperationRun | null, operations: OpSummary[]): OperationRun {
  const newest = operations.reduce((highest, op) => Math.max(highest, op.id), -1);
  const active = operations.filter(isActive);
  if (previous === null) {
    const oldestActive = active.reduce((lowest, op) => Math.min(lowest, op.id), Infinity);
    return {
      floor: active.length > 0 ? oldestActive - 1 : newest - 1,
      seen: newest,
      open: active.length > 0,
    };
  }
  const floor = !previous.open && newest > previous.seen ? previous.seen : previous.floor;
  const seen = Math.max(previous.seen, newest);
  const open = active.length > 0;
  return floor === previous.floor && seen === previous.seen && open === previous.open
    ? previous
    : { floor, seen, open };
}

/**
 * The operation a run's words and Cancel are about while it is under way:
 * the oldest one actually doing something, or, when every one left is
 * waiting its turn, the oldest of those. With one operation, that one.
 * The operation bar asks it first of the ones nothing can stop
 * (`runsToItsEnd`), which it names while one of them runs.
 */
export function currentOf(active: OpSummary[]): OpSummary | undefined {
  let current: OpSummary | undefined;
  for (const op of active) {
    if (current === undefined) {
      current = op;
      continue;
    }
    const waiting = op.status === "Queued";
    const currentWaiting = current.status === "Queued";
    if (waiting !== currentWaiting ? !waiting : op.id < current.id) current = op;
  }
  return current;
}

function keyIdOf(op: OpSummary): string {
  return artifactKeyId({ instance_id: op.instance_id, kind: op.artifact_kind, name: op.name });
}

/**
 * The name an operation's words use for what it acts on: the one its row
 * shows -- 「Claude Code」, where the operation itself carries only its
 * key's `claude` -- and, once that row is gone, the one it had (`opNames`
 * in the store): an uninstall's row leaves the snapshot when it finishes,
 * and its result should not then rename it. The key's own name for an
 * operation no list ever showed.
 */
export function useOperationName(operations: OpSummary[] | undefined): (op: OpSummary) => string {
  const { data: snapshot } = useSnapshot();
  const remembered = useUiStore((s) => s.opNames);
  const rememberOpNames = useUiStore((s) => s.rememberOpNames);
  const listed = useMemo(() => {
    const names = new Map<string, string>();
    // `?? []`: a snapshot query that has not answered yet, or answered
    // something else in a test, lists nothing.
    for (const artifact of snapshot?.artifacts ?? []) {
      names.set(artifactKeyId(artifact.key), artifact.display_name);
    }
    return names;
  }, [snapshot]);

  useEffect(() => {
    const fresh: Record<number, string> = {};
    for (const op of operations ?? []) {
      const name = listed.get(keyIdOf(op));
      if (name !== undefined && remembered[op.id] !== name) fresh[op.id] = name;
    }
    if (Object.keys(fresh).length > 0) rememberOpNames(fresh);
  }, [operations, listed, remembered, rememberOpNames]);

  return useCallback(
    (op: OpSummary) => listed.get(keyIdOf(op)) ?? remembered[op.id] ?? op.name,
    [listed, remembered],
  );
}
