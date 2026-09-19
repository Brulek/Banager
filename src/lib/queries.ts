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
      queryClient.setQueryData(queryKeys.settings, settings);
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

export function useOpenOllamaApp(): UseMutationResult<void, Error, void> {
  return useMutation({ mutationFn: openOllamaApp });
}
