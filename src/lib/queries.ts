import { useCallback } from "react";
import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import {
  getSnapshot,
  planOperation,
  submitOperation,
  cancelOperation,
  listOperations,
  getSettings,
  setSettings,
  openOllamaApp,
  scanUnknown,
  revealInFinder,
  artifactIcon,
} from "./api";
import { isNewerSnapshot, isRefreshInFlight, refreshIntoCache, useRefreshInFlight } from "./events";
import { queryKeys } from "./queryKeys";
import { isAvailable } from "./sources";
import type {
  ArtifactKey,
  IssuedPlan,
  OpRequest,
  OpSummary,
  PlanId,
  Settings,
  Snapshot,
  UnknownScan,
} from "./types";

export { queryKeys };

export function useSnapshot(): UseQueryResult<Snapshot> {
  const queryClient = useQueryClient();
  return useQuery({
    queryKey: queryKeys.snapshot,
    // The other half of the rule in `isNewerSnapshot`: this refetch (a
    // `SnapshotChanged` invalidation, a remount) can also come back with
    // an older snapshot than a concurrent `refresh` has already cached,
    // and whatever a queryFn returns is what React Query stores. Keeping
    // the cached one is how this write obeys the same rule as
    // `writeSnapshotIfNewer`, which cannot reach inside a query's own
    // fetch to enforce it.
    queryFn: async () => {
      const fetched = await getSnapshot();
      const cached = queryClient.getQueryData<Snapshot>(queryKeys.snapshot);
      return isNewerSnapshot(fetched, cached) ? fetched : (cached ?? fetched);
    },
  });
}

export function useSettings(): UseQueryResult<Settings> {
  return useQuery({ queryKey: queryKeys.settings, queryFn: getSettings });
}

export function useOperations(): UseQueryResult<OpSummary[]> {
  return useQuery({ queryKey: queryKeys.operations, queryFn: listOperations });
}

/**
 * The unknown-source scan. `enabled: false`: nothing runs until asked. The
 * Unknown page asks through `refetch` -- once per snapshot generation
 * while it is open, and on "Scan again" -- and it is the only reader. This
 * is not the snapshot: `refresh` never writes it, and `SnapshotChanged`
 * invalidates only the snapshot query (src/lib/events.ts), because it is
 * not about the managed sources. Each scan judges against whatever
 * snapshot is committed when it runs (spec §8.1, Q11), which is why the
 * page re-asks when that snapshot's `generation` moves: a scan made before
 * the startup refresh committed would otherwise stand until pressed.
 */
export function useUnknownScan(): UseQueryResult<UnknownScan> {
  return useQuery({ queryKey: queryKeys.unknown, queryFn: scanUnknown, enabled: false });
}

/**
 * The Unknown page's Show in Finder, handed the path to show
 * (`revealInFinder`). Nothing is cached and nothing refreshed after it:
 * Finder shows the file, and nothing Canager knows has changed. A mutation
 * for its error, which the page says.
 */
export function useRevealInFinder(): UseMutationResult<void, Error, string> {
  return useMutation({ mutationFn: revealInFinder });
}

export function useRefresh(): UseMutationResult<Snapshot, Error, void> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: async () => {
      // refreshIntoCache already writes the result into the cache; reading
      // it back here is what makes a coalesced call (one that arrived while
      // another refresh was already in flight) report the *served* result
      // instead of racing a second, redundant `invoke("refresh")`.
      await refreshIntoCache(queryClient, "manual");
      const result = queryClient.getQueryData<Snapshot>(queryKeys.snapshot);
      if (!result) {
        throw new Error("refresh did not produce a snapshot");
      }
      return result;
    },
  });
}

/**
 * Check again, wherever it is asked for: the page header's button
 * (`CheckAgain`), the menu bar's Check Again (⌘R, src/lib/menu.ts), and
 * the buttons called what the header's is -- a page's that could not load
 * (`SnapshotStatus`), and the one on a Homebrew list that could not be
 * updated (`SourceNotices`). It is the refresh every other trigger runs
 * -- the one at startup, the one after an operation -- through
 * `useRefresh`, and it does nothing while a refresh runs, whoever started
 * it: one asked for then would be folded into the running one and then run
 * once more after it (`refreshIntoCache`'s follow-up), a second check
 * nobody asked for. Every such button is off meanwhile; the menu bar's
 * item, which stays on, does nothing. `checking` says whether one runs;
 * `error`, why the last one this caller started failed, until it starts
 * another.
 */
export function useCheckAgain(): { checkAgain: () => void; checking: boolean; error: Error | null } {
  const { mutate, error } = useRefresh();
  const checking = useRefreshInFlight();
  const checkAgain = useCallback(() => {
    if (!isRefreshInFlight()) mutate();
  }, [mutate]);
  return { checkAgain, checking, error };
}

export function useSaveSettings(): UseMutationResult<void, Error, Settings> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: setSettings,
    onSuccess: (_data, settings) => {
      const previous = queryClient.getQueryData<Settings>(queryKeys.settings);
      queryClient.setQueryData(queryKeys.settings, settings);
      // `include_self_updating` is read fresh by the backend on every
      // refresh and changes which update candidates come back -- but nothing
      // was triggering a refresh, so flipping the switch changed nothing the
      // user could see until the app was restarted. Only this one field is
      // worth re-scanning for: `show_technical_details` and `language` are
      // rendered from the settings themselves, and `ignored_updates` and
      // `skipped_versions` are filtered client-side, so refreshing on every
      // save would put a full scan of every source behind each Skip this
      // version or Never remind me click on the Updates page (which shares
      // this mutation). A failed refresh is swallowed: the save itself did
      // succeed, and the snapshot's own stale/errors fields are what report
      // a bad refresh.
      if (previous && previous.include_self_updating !== settings.include_self_updating) {
        return refreshIntoCache(queryClient, "include_self_updating changed").catch(() => {});
      }
    },
  });
}

/** Plans, shows nothing itself; callers render the IssuedPlan's Plan then submit its id. */
export function usePlanOperation(): UseMutationResult<IssuedPlan, Error, OpRequest> {
  return useMutation({ mutationFn: planOperation });
}

export function useSubmitOperation(): UseMutationResult<number, Error, PlanId> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: submitOperation,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.operations });
    },
  });
}

export function useCancelOperation(): UseMutationResult<void, Error, number> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: cancelOperation,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: queryKeys.operations });
    },
  });
}

/**
 * How long to wait before each look at whether Ollama came up, in order.
 *
 * `open_ollama_app` only asks macOS to launch the app; the daemon's HTTP
 * port is not listening by the time the command resolves, so an immediate
 * refresh reliably finds the instance still unavailable and leaves the very
 * notice the button was pressed to clear. One look after a grace period
 * would usually do, but a cold start can outlast it -- so there is a second,
 * later one, and then it stops. Bounded on purpose: a poll that kept going
 * would scan every source over and over behind a machine where Ollama
 * simply is not going to start. After two looks the notice stays put and the
 * button can be pressed again, which starts this over.
 */
const OLLAMA_START_RETRY_DELAYS_MS = [1_000, 3_000];

function afterDelay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

export function useOpenOllamaApp(): UseMutationResult<void, Error, void> {
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: openOllamaApp,
    // Awaited rather than fired and forgotten, so the mutation's own pending
    // state covers the wait and nothing is left writing into a query cache
    // after the window has gone.
    onSuccess: async () => {
      for (const ms of OLLAMA_START_RETRY_DELAYS_MS) {
        await afterDelay(ms);
        // Through the shared single-flight coordinator in events.ts, so this
        // can never race a startup or post-operation refresh; a rejection is
        // swallowed exactly as every other fire-and-forget caller does.
        await refreshIntoCache(queryClient, "ollama started").catch(() => {});
        const snapshot = queryClient.getQueryData<Snapshot>(queryKeys.snapshot);
        // `isAvailable`, not a re-spelling of it: this poll is what
        // decides whether the notice the button was pressed to clear goes
        // away, so it has to ask the same question the notice asked.
        const up = snapshot?.instances.some(
          (instance) => instance.adapter_id === "ollama" && isAvailable(instance),
        );
        if (up) return;
      }
    },
  });
}

/**
 * How long a cask's icon is kept, and trusted, before it is asked for
 * again. An app's icon changes only when the app is replaced, by an
 * upgrade, and asking again costs little: the Rust side keeps every icon it
 * drew in memory until the app's folder changes (`AppIcons`). An hour, so
 * an app upgraded while Canager stays open shows its new icon without a
 * relaunch, and a row scrolled away and back does not ask again.
 */
export const ARTIFACT_ICON_STALE_MS = 60 * 60 * 1000;

/**
 * The icon Finder shows for a cask's app, for that cask's row: a `data:`
 * URL, or null when it has none (`undefined` until the answer arrives, and
 * when not asked). Asked only when `enabled` and `key` names a cask: no
 * other kind of row ever has one, so anything else would be an IPC call
 * answered null. `retry: false`: null is an answer, and the one failure
 * there is -- a panic while drawing -- would not go away on a second try.
 */
export function useArtifactIcon(
  key: ArtifactKey,
  enabled: boolean,
): UseQueryResult<string | null> {
  return useQuery({
    queryKey: queryKeys.artifactIcon(key),
    queryFn: () => artifactIcon(key),
    enabled: enabled && key.kind === "Cask",
    staleTime: ARTIFACT_ICON_STALE_MS,
    gcTime: ARTIFACT_ICON_STALE_MS,
    retry: false,
  });
}
