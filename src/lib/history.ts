/**
 * The history Banager keeps across launches (`history.json`, Rust
 * `banager_core::history`), as the Updates page's 「最近的更新记录」 reads it:
 * the updates of the last 30 days -- those that worked, and those that did
 * not or ask to be checked -- still listed after Banager is quit and
 * opened again, as the Mac App Store keeps "Update History". This window's own operations stay `justUpdatedOps`'s
 * (src/components/JustUpdated.tsx); the history adds what this window did
 * not see, and never a tool twice.
 */
import { useMutation, useQuery, useQueryClient, type UseMutationResult, type UseQueryResult } from "@tanstack/react-query";
import { clearHistory, getHistory } from "./api";
import { queryKeys } from "./queryKeys";
import { artifactKeyId } from "../store/ui";
import { isUpdatedButStepFailed } from "./format";
import { NO_HISTORY, type HistoryRecord, type HistoryResult, type HistoryView, type OpSummary } from "./types";

/** How far back 「最近的更新记录」 lists: 30 days. */
export const RECENT_DAYS = 30;
const DAY_MS = 24 * 60 * 60 * 1000;

/**
 * The history, asked for as the Updates page needs it and again after each
 * `Finished` event (src/lib/events.ts), whose record is kept before the
 * event is sent. A command that answers nothing reads as no history.
 */
export function useHistory(): UseQueryResult<HistoryView> {
  return useQuery({
    queryKey: queryKeys.history,
    queryFn: async () => (await getHistory()) ?? NO_HISTORY,
  });
}

/**
 * Clear marks existing records as dismissed, including after a restart.
 * Later completions stay visible even if the wall clock moves backwards.
 */
export function useClearHistory(): UseMutationResult<HistoryView, Error, void> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: clearHistory,
    // A Clear that did not reach the backend would come back after a
    // restart: try it again before giving up.
    retry: 2,
    onSuccess: (view) => {
      if (view) queryClient.setQueryData(queryKeys.history, view);
      else queryClient.invalidateQueries({ queryKey: queryKeys.history });
    },
  });
}

/**
 * Whether 「最近的更新记录」 lists an update that ended so: one that worked, one
 * that did not (「未能更新」, with its cause where the tool's words gave
 * one), and one to check (it said it worked, but nothing changed, or
 * Banager could not confirm it). Not one the person cancelled: they know of it.
 */
export function listedResult(result: HistoryResult): boolean {
  return result !== "Cancelled";
}

/**
 * The records 「最近的更新记录」 adds to what this window saw, newest first: per
 * tool, its newest kept operation, when that is an update that ended in a
 * way to know of (`listedResult`) -- the rule `justUpdatedOps` has for this
 * window's own -- finished in the last `RECENT_DAYS` and not dismissed by
 * a Clear (`isDismissed`). Newest per tool, so an update that failed and then worked is
 * listed as the one that worked, and one uninstalled since not at all --
 * by an uninstall that worked: one that failed or was stopped does not
 * count (`recordWeighs`).
 * None for a tool whose say is this window's (`seenHere`, below): its
 * operation decides, whatever the history says, so an update this window
 * saw finish is listed once.
 *
 * One that did not work or asks to be checked only while the last check
 * still offers the tool an update (`offered`, by `artifactKeyId`): updated
 * in Terminal since, or uninstalled, it no longer is, and 「未能更新」
 * would say what Banager cannot know is still true. Not one installed
 * though a step after it failed (`Attention::UpdatedButStepFailed`, r35
 * U2): it is listed as one that worked is, since no check offers what is
 * installed, and after a restart this line is all that says a step of it
 * failed. While it is offered,
 * its row lists it too. A recorded Homebrew password stop also gives
 * that row View Steps through `usePasswordRecoveryKeys`, which reads
 * the records Clear dismissed too (`includeDismissed`): Clear tidies the
 * list, it does not resolve the stop. A record kept since the last Clear
 * is newer than every one a Clear dismissed, whatever the clock said;
 * otherwise the later finish is (`keptLater`).
 *
 * `seenHere`, by `artifactKeyId`, the tools whose say is this window's:
 * for the list, those it has an update or a finished uninstall of that
 * worked (`toolsSeenHere`); for
 * `usePasswordRecoveryKeys`, those whose row shows one, so that a stop of
 * this launch the row no longer shows -- the page reloaded -- still
 * counts from its record (r35 U3).
 */
export function recentUpdates(
  view: HistoryView,
  seenHere: ReadonlySet<string>,
  now: number,
  offered: ReadonlySet<string>,
  { includeDismissed = false }: { includeDismissed?: boolean } = {},
): HistoryRecord[] {
  const newest = new Map<string, HistoryRecord>();
  for (const record of view.records) {
    if (!Number.isFinite(new Date(record.finished_at).getTime())) continue;
    if (!includeDismissed && isDismissed(view, record)) continue;
    // An uninstall that failed or was stopped leaves the update before it
    // listed, as it does in the window that ran it (`opWeighs`).
    if (!recordWeighs(record)) continue;
    const id = artifactKeyId(record.key);
    const seen = newest.get(id);
    if (seen === undefined || keptLater(view, record, seen)) newest.set(id, record);
  }
  const since = now - RECENT_DAYS * DAY_MS;
  return [...newest.entries()]
    .filter(
      ([id, record]) =>
        !seenHere.has(id) &&
        record.kind === "Update" &&
        listedResult(record.result) &&
        (record.result === "Succeeded" || isUpdatedButStepFailed(record.result) || offered.has(id)) &&
        record.finished_at >= since,
    )
    .map(([, record]) => record)
    .sort((a, b) => b.finished_at - a.finished_at);
}

/**
 * Whether an operation of this window has a say in what 「最近的更新记录」
 * lists of its tool: an update, and an uninstall that worked, which ends
 * the tool's line -- not a Fix… link (`Link`), nor an uninstall that did
 * not happen or did not work (cancelled while it waited its turn, failed,
 * or one to check): the update before it still happened (r35 U4).
 * `justUpdatedOps` weighs only these, and `toolsSeenHere` counts only
 * these, so the history can stand in for the rest; `recordWeighs` is the
 * same rule for the history's records.
 */
export function opWeighs(op: OpSummary): boolean {
  return op.kind === "Upgrade" || (op.kind === "Uninstall" && op.status === "Done" && op.outcome === "Succeeded");
}

/**
 * `opWeighs` for a record the history kept: an update, or an uninstall
 * that worked. The history keeps no link and no operation cancelled
 * before it started (`history::record_for`).
 */
export function recordWeighs(record: HistoryRecord): boolean {
  return record.kind === "Update" || record.result === "Succeeded";
}

/**
 * The tools, by `artifactKeyId`, whose line 「最近的更新记录」 takes from
 * this window's own operations (`justUpdatedOps`) rather than from the
 * history (`recentUpdates`): those with an operation that has a say
 * (`opWeighs`). A tool whose only operations here are a Fix… link or an
 * uninstall that did not happen is still the history's to list.
 */
export function toolsSeenHere(operations: readonly OpSummary[]): Set<string> {
  return new Set(
    operations
      .filter(opWeighs)
      .map((op) => artifactKeyId({ instance_id: op.instance_id, kind: op.artifact_kind, name: op.name })),
  );
}

/**
 * Whether `record` was kept after `other`, as far as the history can say:
 * Clear dismisses every record already kept, so one not dismissed came
 * after the last Clear, and so after every dismissed one. Else by finish
 * time, the only order left (`dismissed` is a boolean, and the file is
 * kept sorted by `finished_at`), which a clock set wrong can mislead:
 * between two records that two different Clears dismissed, say.
 */
function keptLater(view: HistoryView, record: HistoryRecord, other: HistoryRecord): boolean {
  const dismissed = isDismissed(view, record);
  if (dismissed !== isDismissed(view, other)) return !dismissed;
  return record.finished_at > other.finished_at;
}

/** Legacy wire data uses its cutoff; current records always carry a boolean. */
function isDismissed(view: HistoryView, record: HistoryRecord): boolean {
  return record.dismissed ?? (view.cleared_before !== null && record.finished_at <= view.cleared_before);
}

/**
 * Whether the page's Clear, as the history keeps it (each record's
 * `dismissed`, whatever its date), took off an operation this window ran:
 * its record of this launch says so. The window's own note of what Clear
 * took off (`clearedJustUpdated`) lives only as long as the web view, which
 * can reload while Banager runs on; this does not. False until the history
 * has the record.
 */
export function clearedHere(view: HistoryView, opId: number): boolean {
  return view.records.some((record) => record.run === view.run && record.op_id === opId && isDismissed(view, record));
}

/**
 * Whether Banager read the version change of an operation this window ran
 * (`HistoryRecord.verified`): the record of this launch with its id, once
 * the history has it; false until then and for one it never kept.
 */
export function verifiedHere(view: HistoryView, opId: number): boolean {
  return view.records.some((record) => record.run === view.run && record.op_id === opId && record.verified);
}
