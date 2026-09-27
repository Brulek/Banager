import { describe, expect, it, vi, beforeEach } from "vitest";
import React from "react";
import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { invoke, Channel } from "@tauri-apps/api/core";
import { isNewerSnapshot, useOperationEvents, useStartupRefresh } from "./events";
import { useUiStore } from "../store/ui";
import { queryKeys } from "./queries";
import type { Snapshot } from "./types";

const mockInvoke = vi.mocked(invoke);

function wrapper(queryClient: QueryClient) {
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return React.createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

const refreshedSnapshot: Snapshot = {
  generation: 1,
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

  it("appends Canager's own Note events to the log without refetching operations", async () => {
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

    pendingRefreshes[1]({ ...refreshedSnapshot, generation: 2 });
    await waitFor(() =>
      expect((queryClient.getQueryData(queryKeys.snapshot) as Snapshot).generation).toBe(2),
    );
    expect(refreshCalls()).toBe(2);
  });
});

describe("isNewerSnapshot", () => {
  const cached: Snapshot = { ...refreshedSnapshot, generation: 2, refreshed_at: 200 };

  it("accepts anything when nothing is cached yet", () => {
    expect(isNewerSnapshot(refreshedSnapshot, undefined)).toBe(true);
  });

  it("rejects an older generation and accepts a newer one", () => {
    expect(isNewerSnapshot({ ...cached, generation: 1 }, cached)).toBe(false);
    expect(isNewerSnapshot({ ...cached, generation: 3 }, cached)).toBe(true);
  });

  it("does not treat an equal generation as stale", () => {
    // A refresh that found nothing new deliberately keeps the same
    // generation (`Snapshot::same_content`), and only its `refreshed_at`
    // moves. Rejecting those would freeze the "last checked" time at
    // whenever this content first appeared.
    expect(isNewerSnapshot({ ...cached, refreshed_at: 300 }, cached)).toBe(true);
    expect(isNewerSnapshot({ ...cached, refreshed_at: 200 }, cached)).toBe(true);
    expect(isNewerSnapshot({ ...cached, refreshed_at: 100 }, cached)).toBe(false);
  });

  it("treats a never-checked snapshot as older than any checked one", () => {
    const never: Snapshot = { ...cached, refreshed_at: null };
    expect(isNewerSnapshot(cached, never)).toBe(true);
    expect(isNewerSnapshot(never, cached)).toBe(false);
  });
});

describe("snapshot cache ordering", () => {
  it("does not let a slow refresh reply overwrite a newer cached snapshot", async () => {
    // The interleaving, in order:
    //   1. a refresh starts (a click, an operation finishing);
    //   2. while it is out, something else — another window's refresh,
    //      or a `SnapshotChanged` invalidating this window's snapshot
    //      query — caches generation 2;
    //   3. the refresh from step 1 replies with generation 1.
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

    const newer: Snapshot = { ...refreshedSnapshot, generation: 2, refreshed_at: 1789700200 };
    queryClient.setQueryData(queryKeys.snapshot, newer);

    resolveRefresh({ ...refreshedSnapshot, generation: 1, refreshed_at: 1789700100 });
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.snapshot)).toEqual(newer));
    // And it stays: nothing arrives later to put it right.
    await Promise.resolve();
    expect(queryClient.getQueryData(queryKeys.snapshot)).toEqual(newer);
  });

  it("still writes a refresh reply whose generation is unchanged", async () => {
    // The companion the rule above must not break: an unchanged refresh
    // keeps its generation and only moves `refreshed_at`, which is what
    // `SnapshotStatus` renders as "last checked".
    const cached: Snapshot = { ...refreshedSnapshot, generation: 2, refreshed_at: 1789700100 };
    const rechecked: Snapshot = { ...cached, refreshed_at: 1789700900 };
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "refresh") return Promise.resolve(rechecked);
      return Promise.resolve(undefined);
    });
    const queryClient = new QueryClient();
    queryClient.setQueryData(queryKeys.snapshot, cached);

    renderHook(() => useStartupRefresh(), { wrapper: wrapper(queryClient) });

    await waitFor(() => expect(queryClient.getQueryData(queryKeys.snapshot)).toEqual(rechecked));
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
