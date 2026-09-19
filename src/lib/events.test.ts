import { describe, expect, it, vi, beforeEach } from "vitest";
import React from "react";
import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { invoke, Channel } from "@tauri-apps/api/core";
import { useOperationEvents, useStartupRefresh } from "./events";
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
