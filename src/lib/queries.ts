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
} from "./api";
import { refreshIntoCache } from "./events";
import { queryKeys } from "./queryKeys";
import type { IssuedPlan, OpRequest, OpSummary, Settings, Snapshot } from "./types";

export { queryKeys };

export function useSnapshot(): UseQueryResult<Snapshot> {
  return useQuery({ queryKey: queryKeys.snapshot, queryFn: getSnapshot });
}

export function useSettings(): UseQueryResult<Settings> {
  return useQuery({ queryKey: queryKeys.settings, queryFn: getSettings });
}

export function useOperations(): UseQueryResult<OpSummary[]> {
  return useQuery({ queryKey: queryKeys.operations, queryFn: listOperations });
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
      // rendered from the settings themselves, and `ignored_updates` is
      // filtered client-side, so refreshing on every save would put a full
      // seven-source scan behind each Ignore click on the Updates page
      // (which shares this mutation). A failed refresh is swallowed: the
      // save itself did succeed, and the snapshot's own stale/errors fields
      // are what report a bad refresh.
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

export function useSubmitOperation(): UseMutationResult<number, Error, number> {
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
 * refresh reliably finds the instance still unhealthy and leaves the very
 * notice the button was pressed to clear. One look after a grace period
 * would usually do, but a cold start can outlast it -- so there is a second,
 * later one, and then it stops. Bounded on purpose: a poll that kept going
 * would scan all seven sources over and over behind a machine where Ollama
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
        const up = snapshot?.instances.some(
          (instance) => instance.adapter_id === "ollama" && instance.healthy,
        );
        if (up) return;
      }
    },
  });
}
