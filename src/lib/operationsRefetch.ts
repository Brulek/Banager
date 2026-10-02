/**
 * Asking for the operations again (`list_operations`), as each one's
 * status changes (`useOperationEvents`, src/lib/events.ts) and as one is
 * started or cancelled (`useSubmitOperation`, `useCancelOperation`): one
 * fetch for every ask that comes while it waits.
 *
 * Each used to ask for one of its own. The backend lists every operation
 * it has kept, so Update all over 754 tools -- each start answered, and
 * announced as Queued: two asks -- fetched about 1,500 lists of up to 754
 * each. Now an ask waits a frame (`OPERATIONS_REFETCH_WAIT_MS`) for others
 * to join it, and there is at most one fetch every
 * `OPERATIONS_REFETCH_EVERY_MS`: an ask soon after a fetch waits for the
 * rest of that time, and every ask meanwhile joins it. So one status
 * change on its own shows within a frame, and hundreds of operations
 * started one after another show a few times a second as they go, not
 * once per ask -- each list taken in is drawn again by every page and bar
 * that shows the operations.
 *
 * An ask that comes while the fetch is on its way waits for its answer,
 * and then for its own turn: that answer may have been listed before what
 * it was asked about, so it is never taken for the newest. The last fetch
 * always starts after the last ask: the operations end as the backend
 * lists them after its last event, as when each ask had its own.
 *
 * Timers, not `requestAnimationFrame`: the page keeps running when the
 * window is closed (src-tauri/src/window.rs), where a frame may never
 * come, and that is when the notification of finished operations
 * (src/lib/operationsNotification.ts) is for.
 */
import type { QueryClient } from "@tanstack/react-query";
import { queryKeys } from "./queryKeys";

/** How long an ask for the operations waits for others to join it: a frame. */
export const OPERATIONS_REFETCH_WAIT_MS = 16;

/** The shortest time from one fetch of the operations to the next. */
export const OPERATIONS_REFETCH_EVERY_MS = 250;

interface Refetch {
  /** The wait under way, if one is. */
  timer: ReturnType<typeof setTimeout> | null;
  /** When the last fetch started (`Date.now()`), or null before the first. */
  lastFetch: number | null;
  /** Whether that fetch has not answered yet. */
  fetching: boolean;
  /** Whether the operations were asked for again while it had not. */
  again: boolean;
}

/** One per query cache: the app has one, and each test its own. */
const refetches = new WeakMap<QueryClient, Refetch>();

/**
 * Asks for the operations again: fetched a frame from now, or once
 * `OPERATIONS_REFETCH_EVERY_MS` has passed since the last fetch if that is
 * later -- once for every ask made meanwhile. What the operations query
 * held is replaced as `invalidateQueries` replaces it: this decides only
 * when.
 */
export function refetchOperations(queryClient: QueryClient): void {
  let refetch = refetches.get(queryClient);
  if (refetch === undefined) {
    refetch = { timer: null, lastFetch: null, fetching: false, again: false };
    refetches.set(queryClient, refetch);
  }
  if (refetch.fetching) {
    refetch.again = true;
    return;
  }
  if (refetch.timer !== null) return;
  const state = refetch;
  const turn = state.lastFetch === null ? 0 : state.lastFetch + OPERATIONS_REFETCH_EVERY_MS - Date.now();
  state.timer = setTimeout(
    () => {
      state.timer = null;
      state.lastFetch = Date.now();
      state.fetching = true;
      const answered = () => {
        state.fetching = false;
        if (state.again) {
          state.again = false;
          refetchOperations(queryClient);
        }
      };
      queryClient.invalidateQueries({ queryKey: queryKeys.operations }).then(answered, answered);
    },
    Math.max(OPERATIONS_REFETCH_WAIT_MS, turn),
  );
}
