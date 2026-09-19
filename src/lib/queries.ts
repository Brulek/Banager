import {
  useMutation,
  useQuery,
  useQueryClient,
  type UseMutationResult,
  type UseQueryResult,
} from "@tanstack/react-query";
import {
  getSnapshot,
  refresh,
  planOperation,
  submitOperation,
  cancelOperation,
  listOperations,
  getSettings,
  setSettings,
} from "./api";
import type { IssuedPlan, OpRequest, OpSummary, Settings, Snapshot } from "./types";

export const queryKeys = {
  snapshot: ["snapshot"] as const,
  operations: ["operations"] as const,
  settings: ["settings"] as const,
};

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
    mutationFn: refresh,
    onSuccess: (snapshot) => {
      queryClient.setQueryData(queryKeys.snapshot, snapshot);
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
