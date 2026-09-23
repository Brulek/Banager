import { useEffect } from "react";
import { useQueryClient, type QueryClient } from "@tanstack/react-query";
import { refresh, subscribeEvents } from "./api";
import { queryKeys } from "./queryKeys";
import { useUiStore } from "../store/ui";
import type { Snapshot, UiEvent } from "./types";

/**
 * Whether `incoming` is at least as recent as what is already cached, and
 * so may replace it.
 *
 * Two things write the snapshot cache and neither can see the other's
 * request in flight: `refreshIntoCache` below, with what its own
 * `refresh` returned, and the `useSnapshot` query, refetching because a
 * `SnapshotChanged` event invalidated it. A refresh whose reply is slow
 * can therefore land *after* a newer snapshot has already been cached and
 * overwrite it — packages the user just removed reappear, with no further
 * event coming to correct it, because as far as the backend is concerned
 * nothing has changed since.
 *
 * `generation` is the right thing to compare, but only with a
 * non-strict `>=`: an unchanged refresh deliberately keeps the same
 * generation (`Snapshot::same_content` in session/refresh.rs), so
 * treating an equal generation as stale would pin `refreshed_at` at the
 * first refresh that saw this content and `SnapshotStatus` would report a
 * "last checked" time that stopped advancing. Two snapshots sharing a
 * generation are the same data by construction, so the only thing that
 * can differ is that timestamp, and the later one wins — with a null
 * (`Snapshot::empty`, nothing has ever been checked) older than any
 * timestamp at all.
 */
export function isNewerSnapshot(incoming: Snapshot, cached: Snapshot | undefined): boolean {
  if (!cached) return true;
  if (incoming.generation !== cached.generation) return incoming.generation > cached.generation;
  return (incoming.refreshed_at ?? -Infinity) >= (cached.refreshed_at ?? -Infinity);
}

/**
 * The only way anything writes the snapshot cache: `isNewerSnapshot`
 * decides, inside the updater so that the read and the write cannot be
 * split by a reply arriving in between.
 */
export function writeSnapshotIfNewer(queryClient: QueryClient, snapshot: Snapshot): void {
  queryClient.setQueryData<Snapshot>(queryKeys.snapshot, (cached) =>
    isNewerSnapshot(snapshot, cached) ? snapshot : cached,
  );
}

/**
 * Runs a backend `refresh` and writes the returned Snapshot straight into the
 * query cache. This — not `get_snapshot` — is the only thing that ever makes
 * the backend go and look at Homebrew: `Session` starts from
 * `Snapshot::empty()` (`generation: 0`, `detect: Missing`, no artifacts,
 * `refreshed_at: null`) and `get_snapshot` merely returns whatever is in
 * memory. Failures are logged, never thrown: the stale/error surfaces in
 * Task 17 read the snapshot's own `stale`/`errors` fields.
 *
 * Refreshes are coordinated here, at module level, not per hook or per
 * component. The backend contract is that a `refresh` arriving while one is
 * already running does not start a second scan: it is merged into the
 * running one and returns *that* one's result. So a refresh that is already
 * past instance A and scanning instance B when an operation on A finishes
 * hands back A's old inventory, and the `Finished`-triggered refresh that
 * was merged into it never sees the change — an uninstall completes and the
 * list does not move. To close that gap, a refresh requested while one is
 * in flight only sets `refreshAgain`, and the in-flight refresh issues
 * exactly one follow-up when it settles. Startup, event-driven and any
 * later manual refresh all go through this one function, so they share the
 * coordination; under StrictMode's double mount the second startup call is
 * coalesced into one follow-up rather than running concurrently.
 */
let refreshInFlight: Promise<void> | null = null;
let refreshAgain = false;

/**
 * Exported (Task 13) so `useRefresh` (src/lib/queries.ts) shares this same
 * single-flight coordinator instead of calling `refresh()` directly --
 * previously a manual "Try again" click could run fully concurrently with
 * an in-flight startup/event-driven refresh, exactly the race this module
 * exists to prevent. A call that arrives while one is already in flight
 * gets back *that* run's own promise rather than starting a second one; it
 * still schedules the one-more-follow-up this module has always used to
 * make sure whatever changed after the in-flight run started is not lost.
 * Every internal side effect (cache write, `startupRefreshError`) is
 * unchanged; the only new thing is that a failure is now also re-thrown, so
 * a caller like `useRefresh` can `await` this and see `isError` — existing
 * fire-and-forget callers below append their own `.catch(() => {})`.
 */
export function refreshIntoCache(queryClient: QueryClient, why: string): Promise<void> {
  if (refreshInFlight) {
    refreshAgain = true;
    return refreshInFlight;
  }
  const run: Promise<void> = refresh()
    .then((snapshot) => {
      // Not `setQueryData` outright: this reply can be older than what a
      // `SnapshotChanged`-driven refetch has already cached. See
      // `isNewerSnapshot`.
      writeSnapshotIfNewer(queryClient, snapshot);
      useUiStore.getState().setStartupRefreshError(null);
    })
    .catch((e: unknown) => {
      console.error(`${why} refresh failed`, e);
      useUiStore.getState().setStartupRefreshError(e instanceof Error ? e.message : String(e));
      throw e;
    })
    .finally(() => {
      refreshInFlight = null;
      if (refreshAgain) {
        refreshAgain = false;
        refreshIntoCache(queryClient, `${why} (follow-up)`).catch(() => {});
      }
    });
  refreshInFlight = run;
  return run;
}

/**
 * Mounted once by `App` (Task 13), next to `useOperationEvents`: triggers the
 * first real refresh when the window opens. Without it the UI would sit on
 * the empty startup snapshot forever and report "Homebrew isn't installed".
 * Until the call resolves the cached snapshot has `refreshed_at: null`,
 * which Task 17's `SnapshotStatus` renders as loading, not as an empty state.
 */
export function useStartupRefresh(): void {
  const queryClient = useQueryClient();

  useEffect(() => {
    refreshIntoCache(queryClient, "initial").catch(() => {});
  }, [queryClient]);
}

/**
 * Mounted once (by `App`, in Task 13) to bridge the backend's Channel into
 * React state: `Operation.Log` and `Operation.Note` events are appended to
 * the Zustand log ring buffer, `Operation.Status`/`Operation.Finished`
 * invalidate the operations query, and `SnapshotChanged` invalidates the snapshot query. A `Finished`
 * event additionally triggers a `refresh`: that is the only way the
 * installed/updates lists learn that an uninstall or update changed
 * anything, because nothing on the backend refreshes on its own. Not part of
 * the skeleton's Core Interfaces — introduced here because `events.ts` needs
 * a concrete hook shape and none was specified.
 */
export function useOperationEvents(): void {
  const queryClient = useQueryClient();

  useEffect(() => {
    let detach: (() => void) | undefined;
    let cancelled = false;

    function handle(event: UiEvent) {
      // The subscription can still be pending when this hook unmounts
      // (StrictMode's first mount, a window that closes at once): the cleanup
      // below has no `detach` to call yet, so the Channel keeps delivering.
      // This check is what makes an unmounted hook inert until then.
      if (cancelled) return;
      if ("Operation" in event) {
        const opEvent = event.Operation;
        if ("Log" in opEvent) {
          useUiStore.getState().appendLog({
            opId: opEvent.Log.op_id,
            stream: opEvent.Log.stream,
            line: opEvent.Log.line,
          });
        } else if ("Note" in opEvent) {
          useUiStore.getState().appendLog({
            opId: opEvent.Note.op_id,
            note: opEvent.Note.note,
          });
        } else {
          queryClient.invalidateQueries({ queryKey: queryKeys.operations });
          if ("Finished" in opEvent) {
            refreshIntoCache(queryClient, "post-operation").catch(() => {});
          }
        }
      } else {
        queryClient.invalidateQueries({ queryKey: queryKeys.snapshot });
      }
    }

    subscribeEvents(handle)
      .then((unsubscribe) => {
        if (cancelled) {
          unsubscribe();
        } else {
          detach = unsubscribe;
        }
      })
      .catch((e: unknown) => {
        // Without this the rejection is unhandled: the app would silently
        // lose its subscription (backend not ready, command not registered)
        // and vitest would fail the whole run on the stray rejection.
        console.error("subscribe_events failed", e);
      });

    // Under React StrictMode the effect mounts, unmounts and mounts again.
    // The first Channel is detached client-side but stays in the backend's
    // ChannelSink registry as a ghost until a send to it fails; it receives
    // events and drops them. There is no other side effect.
    return () => {
      cancelled = true;
      detach?.();
    };
  }, [queryClient]);
}
