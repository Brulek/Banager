import { beforeEach, describe, expect, it, vi } from "vitest";
import React from "react";
import { act, renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { invoke, Channel, type InvokeArgs } from "@tauri-apps/api/core";
import { OPERATIONS_REFETCH_WAIT_MS, refetchOperations } from "./operationsRefetch";
import { queryKeys, useOperations, useSubmitOperation } from "./queries";
import { useOperationEvents } from "./events";
import type { OpStatus, OpSummary } from "./types";

const mockInvoke = vi.mocked(invoke);

function summary(id: number, status: OpStatus = "Queued"): OpSummary {
  return {
    id,
    kind: "Upgrade",
    instance_id: "brew-arm64",
    artifact_kind: "Formula",
    name: `tool-${id}`,
    status,
    outcome: null,
    argv_preview: ["/opt/homebrew/bin/brew", "upgrade", `tool-${id}`],
    env_preview: [],
    cancel_policy: "KillThenReconcile",
  };
}

/** The backend's list as it is now: newest first, as `list_operations` answers. */
let listed: OpSummary[] = [];
/** Every `list_operations` asked for, in order: what it answered, once it has. */
let fetches: Array<{ release: () => void }> = [];
/** Whether a fetch waits for `release` before it answers. */
let holdFetches = false;

function wrapper(queryClient: QueryClient) {
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return React.createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

/** A query cache with the operations on screen, as the app always has them. */
async function watching(): Promise<QueryClient> {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const { result } = renderHook(() => useOperations(), { wrapper: wrapper(queryClient) });
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
  return queryClient;
}

/** Long enough for any wait and fetch still to come to have shown itself. */
async function settle() {
  await act(() => new Promise((resolve) => setTimeout(resolve, OPERATIONS_REFETCH_WAIT_MS * 4)));
}

beforeEach(() => {
  listed = [];
  fetches = [];
  holdFetches = false;
  mockInvoke.mockReset();
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "list_operations") {
      // What the backend lists when the call reaches it, not when it answers.
      const answer = [...listed];
      if (!holdFetches) {
        fetches.push({ release: () => {} });
        return Promise.resolve(answer);
      }
      return new Promise((resolve) => fetches.push({ release: () => resolve(answer) }));
    }
    return Promise.resolve(undefined);
  });
});

describe("refetchOperations", () => {
  it("fetches the operations once for every ask within a frame, and keeps the list the backend has after the last", async () => {
    const queryClient = await watching();
    expect(fetches).toHaveLength(1);

    for (let id = 1; id <= 100; id += 1) {
      listed = [summary(id), ...listed];
      refetchOperations(queryClient);
    }
    // Nothing yet: the asks wait a frame for one another.
    expect(fetches).toHaveLength(1);

    await waitFor(() => expect(fetches).toHaveLength(2));
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual(listed));
    await settle();
    expect(fetches).toHaveLength(2);
    expect(queryClient.getQueryData<OpSummary[]>(queryKeys.operations)).toHaveLength(100);
  });

  it("asks once more after a fetch in flight answers, since that answer may predate what was asked about", async () => {
    const queryClient = await watching();
    holdFetches = true;

    listed = [summary(1)];
    refetchOperations(queryClient);
    await waitFor(() => expect(fetches).toHaveLength(2));

    // Operation 2 starts, and 3, while the list asked for above is on its
    // way without them.
    listed = [summary(2), ...listed];
    refetchOperations(queryClient);
    listed = [summary(3), ...listed];
    refetchOperations(queryClient);
    await settle();
    expect(fetches).toHaveLength(2);

    await act(async () => fetches[1].release());
    expect(queryClient.getQueryData(queryKeys.operations)).toEqual([summary(1)]);
    // One more fetch for both, after the answer.
    await waitFor(() => expect(fetches).toHaveLength(3));
    await act(async () => fetches[2].release());
    await settle();
    expect(fetches).toHaveLength(3);
    expect(queryClient.getQueryData(queryKeys.operations)).toEqual([summary(3), summary(2), summary(1)]);
  });

  it("fetches again for an ask made after the last fetch answered: no ask is lost", async () => {
    const queryClient = await watching();

    for (const status of ["Queued", "Running", "Verifying", "Done"] as const) {
      listed = [summary(1, status)];
      refetchOperations(queryClient);
      await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual(listed));
    }
    expect(fetches).toHaveLength(5);
  });

  it("keeps one wait for each query cache", async () => {
    const one = await watching();
    const two = await watching();
    expect(fetches).toHaveLength(2);

    listed = [summary(1)];
    refetchOperations(one);
    refetchOperations(two);
    await waitFor(() => expect(fetches).toHaveLength(4));
    expect(one.getQueryData(queryKeys.operations)).toEqual(listed);
    expect(two.getQueryData(queryKeys.operations)).toEqual(listed);
  });

  it("starting a burst of updates, each answered and announced, fetches a few lists, not two per update, and ends on the backend's", async () => {
    // Update all, as `confirmAndSubmit` runs it: one submit after the
    // other, each awaited, the backend announcing each as Queued.
    let channel = null as InstanceType<typeof Channel> | null;
    const answer = mockInvoke.getMockImplementation()!;
    let nextId = 1;
    mockInvoke.mockImplementation((cmd: string, args?: InvokeArgs) => {
      if (cmd === "subscribe_events") {
        channel = (args as unknown as { channel: InstanceType<typeof Channel> }).channel;
        return Promise.resolve(undefined);
      }
      if (cmd === "submit_operation") {
        const id = nextId;
        nextId += 1;
        listed = [summary(id), ...listed];
        setTimeout(() => channel!.onmessage({ Operation: { Status: { op_id: id, status: "Queued" } } }), 0);
        return Promise.resolve(id);
      }
      return answer(cmd, args);
    });
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const { result } = renderHook(
      () => {
        useOperationEvents();
        // Read as it is drawn, as every reader of the operations does:
        // React Query draws again only for what a drawing read.
        return { operations: useOperations().data, submit: useSubmitOperation() };
      },
      { wrapper: wrapper(queryClient) },
    );
    await waitFor(() => expect(channel).not.toBeNull());
    await waitFor(() => expect(result.current.operations).toEqual([]));
    const before = fetches.length;

    const N = 200;
    await act(async () => {
      for (let i = 0; i < N; i += 1) {
        await result.current.submit.mutateAsync(`plan-${i}`);
      }
    });
    await settle();

    // Two asks per update -- the submit's answer and its Queued -- were
    // two fetches each, 400 here.
    expect(fetches.length - before).toBeLessThanOrEqual(3);
    expect(listed).toHaveLength(N);
    expect(queryClient.getQueryData(queryKeys.operations)).toEqual(listed);
    await waitFor(() => expect(result.current.operations).toEqual(listed));
  });
});
