import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import React from "react";
import { act, renderHook, waitFor } from "@testing-library/react";
import { focusManager, QueryClient, QueryClientProvider, QueryObserver } from "@tanstack/react-query";
import { invoke, Channel, type InvokeArgs } from "@tauri-apps/api/core";
import { listOperations } from "./api";
import {
  OPERATIONS_FETCH_GIVE_UP_MS,
  OPERATIONS_REFETCH_EVERY_MS,
  OPERATIONS_REFETCH_WAIT_MS,
  refetchOperations,
} from "./operationsRefetch";
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
/** Every `list_operations` asked for, in order: when, and how to let it answer if held. */
let fetches: Array<{ at: number; release: () => void }> = [];
/** Whether a fetch waits for `release` before it answers. */
let holdFetches = false;
/** How many fetches from now fail, as a dropped IPC call would. */
let failFetches = 0;

beforeEach(() => {
  listed = [];
  fetches = [];
  holdFetches = false;
  failFetches = 0;
  mockInvoke.mockReset();
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "list_operations") {
      if (failFetches > 0) {
        failFetches -= 1;
        fetches.push({ at: Date.now(), release: () => {} });
        return Promise.reject("ipc dropped");
      }
      // What the backend lists when the call reaches it, not when it answers.
      const answer = [...listed];
      if (!holdFetches) {
        fetches.push({ at: Date.now(), release: () => {} });
        return Promise.resolve(answer);
      }
      return new Promise((resolve) => fetches.push({ at: Date.now(), release: () => resolve(answer) }));
    }
    return Promise.resolve(undefined);
  });
});

afterEach(() => {
  vi.useRealTimers();
  focusManager.setFocused(undefined);
});

describe("refetchOperations", () => {
  /**
   * A query cache with the operations watched, as the app always has them,
   * and its first fetch done; React Query's own retries off unless asked
   * for (`src/main.tsx` leaves them on).
   */
  async function watched(retry = false): Promise<QueryClient> {
    const queryClient = new QueryClient(retry ? {} : { defaultOptions: { queries: { retry: false } } });
    new QueryObserver(queryClient, { queryKey: queryKeys.operations, queryFn: listOperations }).subscribe(() => {});
    await vi.advanceTimersByTimeAsync(0);
    expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]);
    return queryClient;
  }

  beforeEach(() => {
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "Date"] });
  });

  it("fetches the operations once for every ask within a frame, and keeps the list the backend has after the last", async () => {
    const queryClient = await watched();
    expect(fetches).toHaveLength(1);

    for (let id = 1; id <= 100; id += 1) {
      listed = [summary(id), ...listed];
      refetchOperations(queryClient);
    }
    // Nothing yet: the asks wait a frame for one another.
    await vi.advanceTimersByTimeAsync(OPERATIONS_REFETCH_WAIT_MS - 1);
    expect(fetches).toHaveLength(1);

    await vi.advanceTimersByTimeAsync(1);
    expect(fetches).toHaveLength(2);
    expect(queryClient.getQueryData(queryKeys.operations)).toEqual(listed);
    await vi.advanceTimersByTimeAsync(OPERATIONS_REFETCH_EVERY_MS * 4);
    expect(fetches).toHaveLength(2);
    expect(queryClient.getQueryData<OpSummary[]>(queryKeys.operations)).toHaveLength(100);
  });

  it("fetches a status change on its own within a frame, however many came before", async () => {
    const queryClient = await watched();

    for (const status of ["Queued", "Running", "Verifying", "Done"] as const) {
      listed = [summary(1, status)];
      refetchOperations(queryClient);
      await vi.advanceTimersByTimeAsync(OPERATIONS_REFETCH_WAIT_MS);
      expect(queryClient.getQueryData(queryKeys.operations)).toEqual(listed);
      // The next one comes later than a fetch's turn.
      await vi.advanceTimersByTimeAsync(OPERATIONS_REFETCH_EVERY_MS);
    }
    expect(fetches).toHaveLength(5);
  });

  it("fetches operations started one after another a few times a second as they go, and once more after the last", async () => {
    const queryClient = await watched();

    // An ask every 10 ms for two seconds: two hundred operations started.
    const shown: number[] = [];
    for (let id = 1; id <= 200; id += 1) {
      listed = [summary(id), ...listed];
      refetchOperations(queryClient);
      await vi.advanceTimersByTimeAsync(10);
      shown.push(queryClient.getQueryData<OpSummary[]>(queryKeys.operations)?.length ?? 0);
    }
    await vi.advanceTimersByTimeAsync(OPERATIONS_REFETCH_EVERY_MS);

    const during = fetches.slice(1);
    // Never two fetches closer than a turn; one every turn while the asks go on.
    for (let i = 1; i < during.length; i += 1) {
      expect(during[i].at - during[i - 1].at).toBeGreaterThanOrEqual(OPERATIONS_REFETCH_EVERY_MS);
    }
    expect(during.length).toBeGreaterThanOrEqual(Math.floor(2000 / OPERATIONS_REFETCH_EVERY_MS));
    expect(during.length).toBeLessThanOrEqual(Math.ceil(2000 / OPERATIONS_REFETCH_EVERY_MS) + 1);
    // What was shown kept up as they went, not only at the end.
    expect(shown[Math.floor(shown.length / 2)]).toBeGreaterThan(50);
    expect(queryClient.getQueryData(queryKeys.operations)).toEqual(listed);
  });

  it("asks once more after a fetch on its way answers, since that answer may predate what was asked about", async () => {
    const queryClient = await watched();
    holdFetches = true;

    listed = [summary(1)];
    refetchOperations(queryClient);
    await vi.advanceTimersByTimeAsync(OPERATIONS_REFETCH_WAIT_MS);
    expect(fetches).toHaveLength(2);

    // Operation 2 starts, and 3, while the list asked for above is on its
    // way without them.
    listed = [summary(2), ...listed];
    refetchOperations(queryClient);
    listed = [summary(3), ...listed];
    refetchOperations(queryClient);
    await vi.advanceTimersByTimeAsync(OPERATIONS_REFETCH_EVERY_MS * 4);
    expect(fetches).toHaveLength(2);

    fetches[1].release();
    await vi.advanceTimersByTimeAsync(0);
    expect(queryClient.getQueryData(queryKeys.operations)).toEqual([summary(1)]);
    // One more fetch for both, after the answer.
    await vi.advanceTimersByTimeAsync(OPERATIONS_REFETCH_WAIT_MS);
    expect(fetches).toHaveLength(3);
    fetches[2].release();
    await vi.advanceTimersByTimeAsync(OPERATIONS_REFETCH_EVERY_MS * 4);
    expect(fetches).toHaveLength(3);
    expect(queryClient.getQueryData(queryKeys.operations)).toEqual([summary(3), summary(2), summary(1)]);
  });

  it("keeps to its waits when the clock is set back", async () => {
    const queryClient = await watched();
    listed = [summary(1)];
    refetchOperations(queryClient);
    await vi.advanceTimersByTimeAsync(OPERATIONS_REFETCH_WAIT_MS);
    expect(fetches).toHaveLength(2);

    // Ten minutes back, by hand or by a time sync.
    vi.setSystemTime(Date.now() - 10 * 60 * 1000);
    listed = [summary(1, "Running")];
    refetchOperations(queryClient);
    await vi.advanceTimersByTimeAsync(OPERATIONS_REFETCH_EVERY_MS);
    expect(fetches).toHaveLength(3);
    expect(queryClient.getQueryData(queryKeys.operations)).toEqual(listed);
  });

  it("stops waiting for a fetch that does not answer, and fetches what was asked since", async () => {
    const queryClient = await watched();
    holdFetches = true;
    listed = [summary(1)];
    refetchOperations(queryClient);
    await vi.advanceTimersByTimeAsync(OPERATIONS_REFETCH_WAIT_MS);
    expect(fetches).toHaveLength(2);

    // Asked again while that one hangs: no fetch until it is given up on.
    holdFetches = false;
    listed = [summary(2), ...listed];
    refetchOperations(queryClient);
    // Given up on as long after it started, then a frame's wait.
    await vi.advanceTimersByTimeAsync(OPERATIONS_FETCH_GIVE_UP_MS - 1);
    expect(fetches).toHaveLength(2);
    await vi.advanceTimersByTimeAsync(1 + OPERATIONS_REFETCH_WAIT_MS);
    expect(fetches).toHaveLength(3);
    expect(queryClient.getQueryData(queryKeys.operations)).toEqual(listed);

    // The hung one answering late changes nothing: it was cancelled.
    fetches[1].release();
    await vi.advanceTimersByTimeAsync(OPERATIONS_REFETCH_EVERY_MS * 4);
    expect(queryClient.getQueryData(queryKeys.operations)).toEqual(listed);
    expect(fetches).toHaveLength(3);
  });

  it("fetches again after a failed fetch that React Query waits to retry while the window is hidden", async () => {
    const queryClient = await watched(true);
    focusManager.setFocused(false);
    failFetches = 1;
    listed = [summary(1)];
    refetchOperations(queryClient);
    await vi.advanceTimersByTimeAsync(OPERATIONS_REFETCH_WAIT_MS);
    expect(fetches).toHaveLength(2);

    listed = [summary(2), ...listed];
    refetchOperations(queryClient);
    // Its retry waits for the window to come back; the ask does not.
    await vi.advanceTimersByTimeAsync(OPERATIONS_FETCH_GIVE_UP_MS + OPERATIONS_REFETCH_WAIT_MS);
    expect(fetches).toHaveLength(3);
    expect(queryClient.getQueryData(queryKeys.operations)).toEqual(listed);
  });

  it("keeps each query cache's asks apart", async () => {
    const one = await watched();
    const two = await watched();
    expect(fetches).toHaveLength(2);

    listed = [summary(1)];
    refetchOperations(one);
    refetchOperations(two);
    await vi.advanceTimersByTimeAsync(OPERATIONS_REFETCH_WAIT_MS);
    expect(fetches).toHaveLength(4);
    expect(one.getQueryData(queryKeys.operations)).toEqual(listed);
    expect(two.getQueryData(queryKeys.operations)).toEqual(listed);
  });
});

describe("starting many operations, as Update all does", () => {
  function wrapper(queryClient: QueryClient) {
    return function Wrapper({ children }: { children: React.ReactNode }) {
      return React.createElement(QueryClientProvider, { client: queryClient }, children);
    };
  }

  it("fetches a few lists, not two per update, and ends on the backend's", async () => {
    // As `confirmAndSubmit` runs it: one submit after the other, each
    // awaited, the backend announcing each as Queued.
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
    const started = Date.now();
    await act(async () => {
      for (let i = 0; i < N; i += 1) {
        await result.current.submit.mutateAsync(`plan-${i}`);
      }
    });
    const took = Date.now() - started;
    await waitFor(() => expect(result.current.operations).toEqual(listed), {
      timeout: OPERATIONS_REFETCH_EVERY_MS * 4,
    });

    // Two asks per update -- the submit's answer and its Queued -- were
    // two fetches each: 400 here.
    expect(listed).toHaveLength(N);
    expect(fetches.length - before).toBeLessThanOrEqual(2 + Math.ceil(took / OPERATIONS_REFETCH_EVERY_MS));
    expect(fetches.length - before).toBeLessThan(N / 10);
  });
});
