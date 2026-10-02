/**
 * Asking for the operations again (`list_operations`), as each one's
 * status changes (`useOperationEvents`, src/lib/events.ts) and as one is
 * started or cancelled (`useSubmitOperation`, `useCancelOperation`):
 * however many ask within a frame, one fetch.
 *
 * Each used to ask for one of its own. The backend lists every operation
 * it has kept, so Update all over 754 tools -- each start answered, and
 * announced as Queued: two asks -- fetched about 1,500 lists of up to 754
 * each, and every one was drawn again: the Updates page, the operation
 * bar and the dialog, several times within one frame. Now an ask waits a frame
 * (`OPERATIONS_REFETCH_WAIT_MS`) for the others, and while the fetch it
 * ends in has not answered, a new ask waits for that answer and then
 * starts one more wait -- the answer in flight may have been listed before
 * what it was asked about, so it is never taken as the newest. The last
 * fetch always starts after the last ask: the operations end as the
 * backend lists them after its last event, as when each ask had its own.
 *
 * A timer, not `requestAnimationFrame`: the page keeps running when the
 * window is closed (src-tauri/src/window.rs), where a frame may never
 * come, and that is when the notification of finished operations
 * (src/lib/operationsNotification.ts) is for.
 */
import type { QueryClient } from "@tanstack/react-query";
import { queryKeys } from "./queryKeys";

/** How long an ask for the operations waits for others to join it: a frame. */
export const OPERATIONS_REFETCH_WAIT_MS = 16;

interface Refetch {
  /** The wait under way, if one is. */
  timer: ReturnType<typeof setTimeout> | null;
  /** Whether the fetch the last wait ended in has not answered yet. */
  fetching: boolean;
  /** Whether the operations were asked for again while it had not. */
  again: boolean;
}

/** One per query cache: the app has one, and each test its own. */
const refetches = new WeakMap<QueryClient, Refetch>();

/**
 * Asks for the operations again: fetched within a frame, once for every
 * ask made meanwhile. What the operations query held is replaced as
 * `invalidateQueries` replaces it -- this only decides when.
 */
export function refetchOperations(queryClient: QueryClient): void {
  let refetch = refetches.get(queryClient);
  if (refetch === undefined) {
    refetch = { timer: null, fetching: false, again: false };
    refetches.set(queryClient, refetch);
  }
  if (refetch.fetching) {
    refetch.again = true;
    return;
  }
  if (refetch.timer !== null) return;
  const state = refetch;
  state.timer = setTimeout(() => {
    state.timer = null;
    state.fetching = true;
    const answered = () => {
      state.fetching = false;
      if (state.again) {
        state.again = false;
        refetchOperations(queryClient);
      }
    };
    queryClient.invalidateQueries({ queryKey: queryKeys.operations }).then(answered, answered);
  }, OPERATIONS_REFETCH_WAIT_MS);
}
