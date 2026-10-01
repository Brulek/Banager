/**
 * The history Banager keeps across launches (`history.json`, Rust
 * `banager_core::history`), as the Updates page's 「最近更新」 reads it:
 * the updates that worked in the last 30 days, still listed after Banager
 * is quit and opened again -- as the Mac App Store keeps "Recently
 * Updated". This window's own operations stay `justUpdatedOps`'s
 * (src/components/JustUpdated.tsx); the history adds what this window did
 * not see, and never a tool twice.
 */
import { useMutation, useQuery, useQueryClient, type UseMutationResult, type UseQueryResult } from "@tanstack/react-query";
import { clearHistory, getHistory } from "./api";
import { queryKeys } from "./queryKeys";
import { artifactKeyId } from "../store/ui";
import { NO_HISTORY, type HistoryRecord, type HistoryView, type OpSummary } from "./types";

/** How far back 「最近更新」 lists: 30 days. */
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
 * The Updates page's Clear, kept: the backend notes the time, and the page
 * lists nothing that finished before it, after a restart too. No record is
 * removed from the file.
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
 * The records 「最近更新」 adds to what this window saw, newest first: per
 * tool, its newest kept operation, when that is an update that succeeded
 * -- the rule `justUpdatedOps` has for this window's own -- finished in the
 * last `RECENT_DAYS` and after the last Clear. None for a tool this window
 * has an operation of (`operations`): that operation decides, whatever the
 * history says, so an update this window saw finish is listed once.
 */
export function recentUpdates(view: HistoryView, operations: readonly OpSummary[], now: number): HistoryRecord[] {
  const seenHere = new Set(
    operations.map((op) => artifactKeyId({ instance_id: op.instance_id, kind: op.artifact_kind, name: op.name })),
  );
  const newest = new Map<string, HistoryRecord>();
  for (const record of view.records) {
    const id = artifactKeyId(record.key);
    const seen = newest.get(id);
    if (seen === undefined || record.finished_at > seen.finished_at) newest.set(id, record);
  }
  const since = now - RECENT_DAYS * DAY_MS;
  const cleared = view.cleared_before ?? Number.NEGATIVE_INFINITY;
  return [...newest.entries()]
    .filter(
      ([id, record]) =>
        !seenHere.has(id) &&
        record.kind === "Update" &&
        record.result === "Succeeded" &&
        record.finished_at >= since &&
        record.finished_at > cleared,
    )
    .map(([, record]) => record)
    .sort((a, b) => b.finished_at - a.finished_at);
}

/**
 * Whether the page's Clear, as the history keeps it (`cleared_before`),
 * came after an operation this window ran finished: its record of this
 * launch says when. The window's own note of what Clear took off
 * (`clearedJustUpdated`) lives only as long as the web view, which can
 * reload while Banager runs on; this does not. False until the history
 * has the record.
 */
export function clearedHere(view: HistoryView, opId: number): boolean {
  const cleared = view.cleared_before;
  if (cleared === null) return false;
  return view.records.some((record) => record.run === view.run && record.op_id === opId && record.finished_at <= cleared);
}

/**
 * Whether Banager read the version change of an operation this window ran
 * (`HistoryRecord.verified`): the record of this launch with its id, once
 * the history has it; false until then and for one it never kept.
 */
export function verifiedHere(view: HistoryView, opId: number): boolean {
  return view.records.some((record) => record.run === view.run && record.op_id === opId && record.verified);
}
