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
 * it was asked about, so it is never taken for the newest. It waits no
 * longer than `OPERATIONS_FETCH_GIVE_UP_MS` from the fetch's start: one
 * that has not answered by then -- failed and waiting to try again, which
 * React Query does not do while the window is hidden -- is given up on,
 * and the next fetch cancels it, as every ask used to. The last fetch
 * always starts after the last ask: the operations end as the backend
 * lists them after its last event, as when each ask had its own.
 *
 * The clock can be set back: no wait is ever longer than its own length,
 * whatever `Date.now()` says.
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

/** How long a fetch of the operations is waited for before the next may start. */
export const OPERATIONS_FETCH_GIVE_UP_MS = 2_000;

interface Refetch {
  /** The wait for the next fetch, if one is under way. */
  timer: ReturnType<typeof setTimeout> | null;
  /** When the last fetch started (`Date.now()`), or null before the first. */
  lastFetch: number | null;
  /** The fetch on its way, by its number, or null when none is. */
  fetching: number | null;
  /** How many fetches have started: the next one's number. */
  fetches: number;
  /** Whether the operations were asked for again while it was on its way. */
  again: boolean;
  /** When to stop waiting for it, if it was asked past. */
  giveUp: ReturnType<typeof setTimeout> | null;
}

/** One per query cache: the app has one, and each test its own. */
const refetches = new WeakMap<QueryClient, Refetch>();

/** How long from `since` until `length` has passed: never more than `length`, if the clock went back. */
function rest(since: number, length: number): number {
  return Math.min(length, since + length - Date.now());
}

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
    refetch = { timer: null, lastFetch: null, fetching: null, fetches: 0, again: false, giveUp: null };
    refetches.set(queryClient, refetch);
  }
  const state = refetch;
  if (state.fetching !== null && state.lastFetch !== null) {
    const left = rest(state.lastFetch, OPERATIONS_FETCH_GIVE_UP_MS);
    if (left > 0) {
      state.again = true;
      if (state.giveUp === null) {
        const waitedFor = state.fetching;
        state.giveUp = setTimeout(() => {
          state.giveUp = null;
          if (state.fetching === waitedFor) stopWaiting(queryClient, state);
        }, left);
      }
      return;
    }
    // On its way too long: the next fetch cancels it, and is for every ask so far.
    state.fetching = null;
    state.again = false;
    if (state.giveUp !== null) {
      clearTimeout(state.giveUp);
      state.giveUp = null;
    }
  }
  if (state.timer !== null) return;
  const turn = state.lastFetch === null ? 0 : rest(state.lastFetch, OPERATIONS_REFETCH_EVERY_MS);
  state.timer = setTimeout(
    () => {
      state.timer = null;
      state.fetches += 1;
      const number = state.fetches;
      state.lastFetch = Date.now();
      state.fetching = number;
      // An answer to a fetch given up on since says nothing of the one after it.
      const answered = () => {
        if (state.fetching === number) stopWaiting(queryClient, state);
      };
      queryClient.invalidateQueries({ queryKey: queryKeys.operations }).then(answered, answered);
    },
    Math.max(OPERATIONS_REFETCH_WAIT_MS, turn),
  );
}

/** The fetch on its way has answered, or is given up on: what was asked meanwhile is asked now. */
function stopWaiting(queryClient: QueryClient, state: Refetch): void {
  state.fetching = null;
  if (state.giveUp !== null) {
    clearTimeout(state.giveUp);
    state.giveUp = null;
  }
  if (state.again) {
    state.again = false;
    refetchOperations(queryClient);
  }
}
