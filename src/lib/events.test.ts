import { describe, expect, it, vi, beforeEach } from "vitest";
import React from "react";
import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { invoke, Channel } from "@tauri-apps/api/core";
import { isNewerSnapshot, refreshIntoCache, useOperationEvents, useStartupRefresh } from "./events";
import { useUiStore } from "../store/ui";
import { queryKeys, useOperations, useSnapshot } from "./queries";
import { OPERATIONS_REFETCH_EVERY_MS, OPERATIONS_REFETCH_WAIT_MS } from "./operationsRefetch";
import type { Snapshot } from "./types";

const mockInvoke = vi.mocked(invoke);

function wrapper(queryClient: QueryClient) {
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return React.createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

const refreshedSnapshot: Snapshot = {
  generation: 1,
  round: 1,
  detect: "Found",
  instances: [],
  artifacts: [],
  updates: [],
  refreshed_at: 1789700000,
  stale: false,
  errors: [],
};

beforeEach(() => {
  mockInvoke.mockReset();
  useUiStore.setState({ logs: [] });
});

// `capturedChannel` is declared as `null as … | null` rather than with a type
// annotation and a bare `null` initializer: tsc narrows the latter to `null`
// (the assignment inside the mock callback is invisible to control-flow
// analysis), which makes the later `capturedChannel!` collapse to `never`
// and fails `pnpm build` with "Property 'onmessage' does not exist on type
// 'never'".
describe("useOperationEvents", () => {
  it("appends streamed Log events to the ui store", async () => {
    let capturedChannel = null as InstanceType<typeof Channel> | null;
    mockInvoke.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "subscribe_events") {
        capturedChannel = (args as { channel: InstanceType<typeof Channel> }).channel;
      }
      return Promise.resolve(undefined);
    });
    const queryClient = new QueryClient();

    renderHook(() => useOperationEvents(), { wrapper: wrapper(queryClient) });

    await waitFor(() => expect(capturedChannel).not.toBeNull());
    capturedChannel!.onmessage({
      Operation: { Log: { op_id: 1, stream: "Stdout", line: "Installing jq" } },
    });

    expect(useUiStore.getState().logs).toMatchObject([
      { opId: 1, stream: "Stdout", line: "Installing jq" },
    ]);
  });

  it("appends Banager's own Note events to the log without refetching operations", async () => {
    let capturedChannel = null as InstanceType<typeof Channel> | null;
    mockInvoke.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "subscribe_events") {
        capturedChannel = (args as { channel: InstanceType<typeof Channel> }).channel;
      }
      return Promise.resolve(undefined);
    });
    const queryClient = new QueryClient();
    const invalidateSpy = vi.spyOn(queryClient, "invalidateQueries");

    renderHook(() => useOperationEvents(), { wrapper: wrapper(queryClient) });

    await waitFor(() => expect(capturedChannel).not.toBeNull());
    capturedChannel!.onmessage({
      Operation: { Note: { op_id: 3, note: { WaitingForBrewUpdate: { minutes: 10 } } } },
    });

    expect(useUiStore.getState().logs).toMatchObject([
      { opId: 3, note: { WaitingForBrewUpdate: { minutes: 10 } } },
    ]);
    // A note is a log line, not a status change: it must not fall through
    // to the branch that treats every non-Log event as one.
    expect(invalidateSpy).not.toHaveBeenCalled();
  });

  it("fetches the operations again a frame after a Status or a Finished event, and not after a Log or a Note", async () => {
    let capturedChannel = null as InstanceType<typeof Channel> | null;
    mockInvoke.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "subscribe_events") {
        capturedChannel = (args as { channel: InstanceType<typeof Channel> }).channel;
      }
      if (cmd === "list_operations") return Promise.resolve([]);
      if (cmd === "refresh") return Promise.resolve(refreshedSnapshot);
      return Promise.resolve(undefined);
    });
    const listCalls = () => mockInvoke.mock.calls.filter(([cmd]) => cmd === "list_operations").length;
    const queryClient = new QueryClient();

    // The operations on screen, as the operation bar always has them.
    renderHook(
      () => {
        useOperationEvents();
        return useOperations().data;
      },
      { wrapper: wrapper(queryClient) },
    );
    await waitFor(() => expect(capturedChannel).not.toBeNull());
    await waitFor(() => expect(listCalls()).toBe(1));

    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "Date"] });
    try {
      capturedChannel!.onmessage({ Operation: { Log: { op_id: 1, stream: "Stdout", line: "Upgrading jq" } } });
      capturedChannel!.onmessage({ Operation: { Note: { op_id: 1, note: { WaitingForBrewUpdate: { minutes: 10 } } } } });
      await vi.advanceTimersByTimeAsync(OPERATIONS_REFETCH_EVERY_MS * 2);
      expect(listCalls()).toBe(1);

      capturedChannel!.onmessage({ Operation: { Status: { op_id: 1, status: "Running" } } });
      await vi.advanceTimersByTimeAsync(OPERATIONS_REFETCH_WAIT_MS);
      expect(listCalls()).toBe(2);

      await vi.advanceTimersByTimeAsync(OPERATIONS_REFETCH_EVERY_MS);
      capturedChannel!.onmessage({ Operation: { Finished: { op_id: 1, outcome: "Succeeded" } } });
      await vi.advanceTimersByTimeAsync(OPERATIONS_REFETCH_WAIT_MS);
      expect(listCalls()).toBe(3);
    } finally {
      vi.useRealTimers();
    }
  });

  it("invalidates the snapshot query on SnapshotChanged", async () => {
    let capturedChannel = null as InstanceType<typeof Channel> | null;
    mockInvoke.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "subscribe_events") {
        capturedChannel = (args as { channel: InstanceType<typeof Channel> }).channel;
      }
      return Promise.resolve(undefined);
    });
    const queryClient = new QueryClient();
    const invalidateSpy = vi.spyOn(queryClient, "invalidateQueries");

    renderHook(() => useOperationEvents(), { wrapper: wrapper(queryClient) });

    await waitFor(() => expect(capturedChannel).not.toBeNull());
    capturedChannel!.onmessage({ SnapshotChanged: { generation: 4 } });

    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: queryKeys.snapshot });
  });

  it("invalidates the sizes query, and only it, on SizesChanged", async () => {
    let capturedChannel = null as InstanceType<typeof Channel> | null;
    mockInvoke.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "subscribe_events") {
        capturedChannel = (args as { channel: InstanceType<typeof Channel> }).channel;
      }
      return Promise.resolve(undefined);
    });
    const queryClient = new QueryClient();
    const invalidateSpy = vi.spyOn(queryClient, "invalidateQueries");

    renderHook(() => useOperationEvents(), { wrapper: wrapper(queryClient) });

    await waitFor(() => expect(capturedChannel).not.toBeNull());
    capturedChannel!.onmessage({ SizesChanged: { round: 4 } });

    expect(invalidateSpy.mock.calls).toEqual([[{ queryKey: queryKeys.sizes }]]);
    expect(mockInvoke).not.toHaveBeenCalledWith("refresh");
  });

  it("refreshes the snapshot into the cache after a Finished event", async () => {
    let capturedChannel = null as InstanceType<typeof Channel> | null;
    mockInvoke.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "subscribe_events") {
        capturedChannel = (args as { channel: InstanceType<typeof Channel> }).channel;
      }
      if (cmd === "refresh") return Promise.resolve(refreshedSnapshot);
      return Promise.resolve(undefined);
    });
    const queryClient = new QueryClient();

    renderHook(() => useOperationEvents(), { wrapper: wrapper(queryClient) });

    await waitFor(() => expect(capturedChannel).not.toBeNull());
    expect(mockInvoke).not.toHaveBeenCalledWith("refresh");
    capturedChannel!.onmessage({ Operation: { Finished: { op_id: 1, outcome: "Succeeded" } } });

    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("refresh"));
    await waitFor(() =>
      expect(queryClient.getQueryData(queryKeys.snapshot)).toEqual(refreshedSnapshot),
    );
  });

  it("remembers when each operation finished, as it hears so, and only the first time", async () => {
    let capturedChannel = null as InstanceType<typeof Channel> | null;
    mockInvoke.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "subscribe_events") {
        capturedChannel = (args as { channel: InstanceType<typeof Channel> }).channel;
      }
      if (cmd === "refresh") return Promise.resolve(refreshedSnapshot);
      return Promise.resolve(undefined);
    });
    const queryClient = new QueryClient();
    const refreshCalls = () => mockInvoke.mock.calls.filter(([cmd]) => cmd === "refresh").length;
    // Only the clock is fake.
    vi.useFakeTimers({ toFake: ["Date"] });
    try {
      vi.setSystemTime(1_790_000_000_000);
      renderHook(() => useOperationEvents(), { wrapper: wrapper(queryClient) });

      await waitFor(() => expect(capturedChannel).not.toBeNull());
      capturedChannel!.onmessage({ Operation: { Status: { op_id: 4, status: "Running" } } });
      expect(useUiStore.getState().opFinishedAt).toEqual({});

      capturedChannel!.onmessage({ Operation: { Finished: { op_id: 4, outcome: "Succeeded" } } });
      expect(useUiStore.getState().opFinishedAt).toEqual({ 4: 1_790_000_000_000 });

      vi.setSystemTime(1_790_000_060_000);
      capturedChannel!.onmessage({ Operation: { Finished: { op_id: 4, outcome: "Succeeded" } } });
      expect(useUiStore.getState().opFinishedAt).toEqual({ 4: 1_790_000_000_000 });

      // Both finishes asked for a refresh, the second folded into one
      // follow-up: settled here, so neither reaches the next test.
      await waitFor(() => expect(refreshCalls()).toBe(2));
      await new Promise((resolve) => setTimeout(resolve, 0));
    } finally {
      vi.useRealTimers();
    }
  });

  it("asks for the history again on Finished, whose record is kept before the event is sent", async () => {
    let capturedChannel = null as InstanceType<typeof Channel> | null;
    mockInvoke.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "subscribe_events") {
        capturedChannel = (args as { channel: InstanceType<typeof Channel> }).channel;
      }
      if (cmd === "refresh") return Promise.resolve(refreshedSnapshot);
      return Promise.resolve(undefined);
    });
    const queryClient = new QueryClient();
    queryClient.setQueryData(queryKeys.history, { run: "r", cleared_before: null, records: [] });
    renderHook(() => useOperationEvents(), { wrapper: wrapper(queryClient) });

    await waitFor(() => expect(capturedChannel).not.toBeNull());
    capturedChannel!.onmessage({ Operation: { Status: { op_id: 4, status: "Running" } } });
    expect(queryClient.getQueryState(queryKeys.history)?.isInvalidated).toBe(false);
    capturedChannel!.onmessage({ Operation: { Finished: { op_id: 4, outcome: "Succeeded" } } });
    expect(queryClient.getQueryState(queryKeys.history)?.isInvalidated).toBe(true);
    await waitFor(() => expect(mockInvoke.mock.calls.some(([cmd]) => cmd === "refresh")).toBe(true));
    await new Promise((resolve) => setTimeout(resolve, 0));
  });

  it("ignores events that arrive after unmount while the subscription is still pending", async () => {
    let capturedChannel = null as InstanceType<typeof Channel> | null;
    let resolveSubscribe: () => void = () => {};
    mockInvoke.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "subscribe_events") {
        capturedChannel = (args as { channel: InstanceType<typeof Channel> }).channel;
        return new Promise<void>((resolve) => {
          resolveSubscribe = resolve;
        });
      }
      if (cmd === "refresh") return Promise.resolve(refreshedSnapshot);
      return Promise.resolve(undefined);
    });
    const queryClient = new QueryClient();

    const { unmount } = renderHook(() => useOperationEvents(), { wrapper: wrapper(queryClient) });

    await waitFor(() => expect(capturedChannel).not.toBeNull());
    unmount();

    // The registration has not been acknowledged, so the cleanup had no
    // `detach` to call: the Channel still delivers to the old handler.
    capturedChannel!.onmessage({
      Operation: { Log: { op_id: 1, stream: "Stdout", line: "late line" } },
    });
    capturedChannel!.onmessage({ Operation: { Finished: { op_id: 1, outcome: "Succeeded" } } });

    expect(useUiStore.getState().logs).toEqual([]);
    expect(mockInvoke).not.toHaveBeenCalledWith("refresh");

    // Settle the pending registration so it cannot leak into the next test.
    resolveSubscribe();
    await Promise.resolve();
  });

  it("coalesces a refresh requested while one is in flight into a single follow-up", async () => {
    // The refresh coordinator in events.ts is module-level state, so every
    // deferred `refresh` here is resolved before the test ends; a refresh
    // left pending would be coalesced into the next test's calls.
    let capturedChannel = null as InstanceType<typeof Channel> | null;
    const pendingRefreshes: Array<(snapshot: Snapshot) => void> = [];
    mockInvoke.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "subscribe_events") {
        capturedChannel = (args as { channel: InstanceType<typeof Channel> }).channel;
      }
      if (cmd === "refresh") {
        return new Promise<Snapshot>((resolve) => {
          pendingRefreshes.push(resolve);
        });
      }
      return Promise.resolve(undefined);
    });
    const queryClient = new QueryClient();
    const refreshCalls = () => mockInvoke.mock.calls.filter(([cmd]) => cmd === "refresh").length;

    renderHook(() => useOperationEvents(), { wrapper: wrapper(queryClient) });
    await waitFor(() => expect(capturedChannel).not.toBeNull());

    // Two operations finish while the first refresh is still scanning. The
    // backend would merge the second request into the first scan, so the
    // front end must not fire it yet — it only notes that one is owed.
    capturedChannel!.onmessage({ Operation: { Finished: { op_id: 1, outcome: "Succeeded" } } });
    capturedChannel!.onmessage({ Operation: { Finished: { op_id: 2, outcome: "Succeeded" } } });
    expect(refreshCalls()).toBe(1);

    // Once the first settles, exactly one follow-up goes out.
    pendingRefreshes[0]({ ...refreshedSnapshot, generation: 1 });
    await waitFor(() => expect(refreshCalls()).toBe(2));

    pendingRefreshes[1]({ ...refreshedSnapshot, generation: 2, round: 2 });
    await waitFor(() =>
      expect((queryClient.getQueryData(queryKeys.snapshot) as Snapshot).generation).toBe(2),
    );
    expect(refreshCalls()).toBe(2);
  });
});

describe("isNewerSnapshot", () => {
  // The backend's round 5, generation 2, stamped at 2026-09-28 09:00 UTC
  // by the Mac's clock.
  const cached: Snapshot = { ...refreshedSnapshot, generation: 2, round: 5, refreshed_at: 1790586000 };

  it("accepts anything when nothing is cached yet", () => {
    expect(isNewerSnapshot(refreshedSnapshot, undefined)).toBe(true);
  });

  it("takes a later round and rejects an earlier one", () => {
    expect(isNewerSnapshot({ ...cached, round: 6, generation: 3 }, cached)).toBe(true);
    expect(isNewerSnapshot({ ...cached, round: 4, generation: 1 }, cached)).toBe(false);
  });

  it("takes a later round that found nothing new after the clock was set back", () => {
    // The next round finds what this one did, so it keeps generation 2
    // (`Snapshot::same_content`), and the clock was put back an hour
    // before it ran. Its `refreshed_at` reads earlier than the cached
    // one's, and by that it was dropped.
    const setBack: Snapshot = { ...cached, round: 6, refreshed_at: 1790586000 - 3600 };
    expect(isNewerSnapshot(setBack, cached)).toBe(true);
    // And round 5, landing after it, stays out, however late its clock read.
    expect(isNewerSnapshot(cached, setBack)).toBe(false);
  });

  it("tells two rounds stamped within the same second apart by their numbers", () => {
    const next: Snapshot = { ...cached, round: 6 };
    expect(isNewerSnapshot(next, cached)).toBe(true);
    expect(isNewerSnapshot(cached, next)).toBe(false);
  });

  it("treats the startup snapshot, round 0, as older than any round", () => {
    // `Snapshot::empty()`, and the first check of a Mac with no source at
    // all: nothing new to it, so still generation 0 -- only the round says
    // which came first.
    const startup: Snapshot = {
      ...refreshedSnapshot,
      generation: 0,
      round: 0,
      detect: "Missing",
      refreshed_at: null,
    };
    const firstCheck: Snapshot = { ...startup, round: 1, refreshed_at: 1790586000 };
    expect(isNewerSnapshot(firstCheck, startup)).toBe(true);
    expect(isNewerSnapshot(startup, firstCheck)).toBe(false);
  });

  it("lets the same round in again, but never an earlier generation of it", () => {
    expect(isNewerSnapshot({ ...cached }, cached)).toBe(true);
    expect(isNewerSnapshot({ ...cached, generation: 3 }, cached)).toBe(true);
    expect(isNewerSnapshot({ ...cached, generation: 1 }, cached)).toBe(false);
  });
});

describe("snapshot cache ordering", () => {
  it("does not let a slow refresh reply overwrite a newer cached snapshot", async () => {
    // The interleaving, in order:
    //   1. a refresh starts (a click, an operation finishing);
    //   2. while it is out, something else — another window's refresh,
    //      or a `SnapshotChanged` invalidating this window's snapshot
    //      query — caches round 3, generation 2;
    //   3. the refresh from step 1 replies with round 2, generation 1.
    // Written into the cache unconditionally, step 3 rolls the UI back
    // to data the backend has already superseded, and no further event
    // is coming: as far as the backend is concerned nothing has changed
    // since. Packages the user just removed reappear and stay.
    let resolveRefresh: (snapshot: Snapshot) => void = () => {};
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "refresh") {
        return new Promise<Snapshot>((resolve) => {
          resolveRefresh = resolve;
        });
      }
      return Promise.resolve(undefined);
    });
    const queryClient = new QueryClient();

    renderHook(() => useStartupRefresh(), { wrapper: wrapper(queryClient) });
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("refresh"));

    const newer: Snapshot = { ...refreshedSnapshot, generation: 2, round: 3, refreshed_at: 1789700200 };
    queryClient.setQueryData(queryKeys.snapshot, newer);

    resolveRefresh({ ...refreshedSnapshot, generation: 1, round: 2, refreshed_at: 1789700100 });
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.snapshot)).toEqual(newer));
    // And it stays: nothing arrives later to put it right.
    await Promise.resolve();
    expect(queryClient.getQueryData(queryKeys.snapshot)).toEqual(newer);
  });

  it("still writes a refresh reply whose generation is unchanged", async () => {
    // The companion the rule above must not break: an unchanged refresh
    // keeps its generation, and it is a later round, whose `refreshed_at`
    // is what `SnapshotStatus` renders as "last checked".
    const cached: Snapshot = { ...refreshedSnapshot, generation: 2, round: 3, refreshed_at: 1789700100 };
    const rechecked: Snapshot = { ...cached, round: 4, refreshed_at: 1789700900 };
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "refresh") return Promise.resolve(rechecked);
      return Promise.resolve(undefined);
    });
    const queryClient = new QueryClient();
    queryClient.setQueryData(queryKeys.snapshot, cached);

    renderHook(() => useStartupRefresh(), { wrapper: wrapper(queryClient) });

    await waitFor(() => expect(queryClient.getQueryData(queryKeys.snapshot)).toEqual(rechecked));
  });

  it("writes a later round with nothing new whose clock was set back", async () => {
    // The clock was put back an hour between two checks that found the
    // same things: one generation, and the later round stamped earlier.
    // Judged by the clock, the reply was the older of the two and never
    // written, so "Checked … ago" stayed on the round before it.
    const cached: Snapshot = { ...refreshedSnapshot, generation: 2, round: 3, refreshed_at: 1789700900 };
    const rechecked: Snapshot = { ...cached, round: 4, refreshed_at: 1789700900 - 3600 };
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "refresh") return Promise.resolve(rechecked);
      return Promise.resolve(undefined);
    });
    const queryClient = new QueryClient();
    queryClient.setQueryData(queryKeys.snapshot, cached);

    await refreshIntoCache(queryClient, "test");

    expect(queryClient.getQueryData(queryKeys.snapshot)).toEqual(rechecked);
  });

  it("keeps the later of two rounds stamped within the same second when the earlier one lands last", async () => {
    // A refresh is out for round 3 when round 4 -- nothing new, the same
    // second on the clock -- is fetched by the snapshot query, as a
    // `SnapshotChanged` has it do, and cached first. Equal generations
    // and equal times let whichever landed last win, and round 3's late
    // reply took the cache back.
    const earlier: Snapshot = { ...refreshedSnapshot, generation: 2, round: 3, refreshed_at: 1789700100 };
    const later: Snapshot = { ...earlier, round: 4 };
    let resolveRefresh: (snapshot: Snapshot) => void = () => {};
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "refresh") {
        return new Promise<Snapshot>((resolve) => {
          resolveRefresh = resolve;
        });
      }
      if (cmd === "get_snapshot") return Promise.resolve(later);
      return Promise.resolve(undefined);
    });
    const queryClient = new QueryClient();

    const run = refreshIntoCache(queryClient, "test");
    const { result } = renderHook(() => useSnapshot(), { wrapper: wrapper(queryClient) });
    await waitFor(() => expect(result.current.data).toEqual(later));

    resolveRefresh(earlier);
    await run;

    expect(queryClient.getQueryData(queryKeys.snapshot)).toEqual(later);
    expect(result.current.data).toEqual(later);
  });
});

describe("useStartupRefresh", () => {
  it("calls refresh on mount and writes the result into the snapshot cache", async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "refresh") return Promise.resolve(refreshedSnapshot);
      return Promise.resolve(undefined);
    });
    const queryClient = new QueryClient();

    renderHook(() => useStartupRefresh(), { wrapper: wrapper(queryClient) });

    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("refresh"));
    await waitFor(() =>
      expect(queryClient.getQueryData(queryKeys.snapshot)).toEqual(refreshedSnapshot),
    );
  });

  it("sets startupRefreshError in the ui store when the startup refresh rejects", async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "refresh") return Promise.reject("brew: command not found");
      return Promise.resolve(undefined);
    });
    const queryClient = new QueryClient();

    renderHook(() => useStartupRefresh(), { wrapper: wrapper(queryClient) });

    await waitFor(() =>
      expect(useUiStore.getState().startupRefreshError).toBe("brew: command not found"),
    );
  });
});
