import { describe, expect, it, vi, beforeEach } from "vitest";
import React from "react";
import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import {
  useSnapshot,
  useRefresh,
  usePlanOperation,
  useSubmitOperation,
  useOpenOllamaApp,
} from "./queries";
import { refreshIntoCache } from "./events";
import type { IssuedPlan, ManagerInstance, Snapshot } from "./types";

function ollamaInstance(running: boolean): ManagerInstance {
  return {
    id: "ollama:http://127.0.0.1:11434",
    adapter_id: "ollama",
    exe_path: "/usr/local/bin/ollama",
    prefix: "/usr/local",
    scope: "User",
    version: null,
    unverified_version: null,
    read_only_reason: null,
    status: { unavailable: running ? null : "NotRunning", notes: [] },
  };
}

const mockInvoke = vi.mocked(invoke);

function wrapper(queryClient: QueryClient) {
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return React.createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

function newClient() {
  return new QueryClient({ defaultOptions: { queries: { retry: false }, mutations: { retry: false } } });
}

const snapshot: Snapshot = {
  generation: 1,
  detect: "Found",
  instances: [],
  artifacts: [],
  updates: [],
  refreshed_at: null,
  stale: false,
  errors: [],
};

beforeEach(() => {
  mockInvoke.mockReset();
});

describe("queries", () => {
  it("useSnapshot fetches through getSnapshot", async () => {
    mockInvoke.mockResolvedValueOnce(snapshot as never);
    const queryClient = newClient();
    const { result } = renderHook(() => useSnapshot(), { wrapper: wrapper(queryClient) });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual(snapshot);
    expect(mockInvoke).toHaveBeenCalledWith("get_snapshot");
  });

  it("useRefresh writes its result into the snapshot cache", async () => {
    mockInvoke.mockResolvedValueOnce({ ...snapshot, generation: 2 } as never);
    const queryClient = newClient();
    const { result } = renderHook(() => useRefresh(), { wrapper: wrapper(queryClient) });

    result.current.mutate();
    await waitFor(() => expect(result.current.isSuccess).toBe(true));

    expect((queryClient.getQueryData(["snapshot"]) as Snapshot).generation).toBe(2);
  });

  it("useRefresh coalesces with a refresh already started through refreshIntoCache instead of firing a second one", async () => {
    let resolveFirst: (s: Snapshot) => void = () => {};
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "refresh") {
        return new Promise<Snapshot>((resolve) => {
          resolveFirst = resolve;
        });
      }
      return Promise.resolve(undefined);
    });
    const queryClient = newClient();
    const refreshCalls = () => mockInvoke.mock.calls.filter(([cmd]) => cmd === "refresh").length;

    // Something else (e.g. the Finished-event handler in events.ts)
    // already started a refresh through the shared coordinator.
    const inFlight = refreshIntoCache(queryClient, "test-setup");
    await waitFor(() => expect(refreshCalls()).toBe(1));

    const { result } = renderHook(() => useRefresh(), { wrapper: wrapper(queryClient) });
    result.current.mutate();

    // If useRefresh still called `refresh` directly (bypassing the
    // coordinator), this would now be 2.
    expect(refreshCalls()).toBe(1);

    resolveFirst({ ...snapshot, generation: 5 });
    await inFlight;
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect((queryClient.getQueryData(["snapshot"]) as Snapshot).generation).toBe(5);

    // The coordinator always issues one more refresh after an in-flight one
    // settles, and this mock hands every `refresh` a promise that only the
    // test resolves. Left pending, that follow-up stays in events.ts's
    // module-level `refreshInFlight` slot for the rest of the file, and
    // every later test's refresh is silently coalesced into a refresh that
    // never finishes. `resolveFirst` now points at the follow-up's resolver.
    await waitFor(() => expect(refreshCalls()).toBe(2));
    resolveFirst({ ...snapshot, generation: 5 });
    await waitFor(() =>
      expect((queryClient.getQueryData(["snapshot"]) as Snapshot).generation).toBe(5),
    );
  });

  it("usePlanOperation calls planOperation and returns its IssuedPlan", async () => {
    const issued: IssuedPlan = {
      id: 1,
      plan: {
        request: { kind: "Uninstall", instance_id: "brew:/opt/homebrew", artifact_kind: "Formula", name: "jq" },
        program: "/opt/homebrew/bin/brew",
        args: ["uninstall", "--formula", "jq"],
        env: [],
        needs_password: false,
        locks: ["brew:/opt/homebrew"],
        cancel_policy: "KillThenReconcile",
        warnings: [],
        affected: [],
        timeout_secs: 1800,
      },
      issued_at: 1758000000,
    };
    mockInvoke.mockResolvedValueOnce(issued as never);
    const queryClient = newClient();
    const { result } = renderHook(() => usePlanOperation(), { wrapper: wrapper(queryClient) });

    result.current.mutate(issued.plan.request);
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual(issued);
  });

  it("useOpenOllamaApp refreshes once the daemon has had a moment to come up", async () => {
    // open_ollama_app only asks macOS to launch the app. The notice the
    // button sits in says "Start the Ollama app to see its models and check
    // for updates", and until this mutation refreshed, it stayed on screen
    // until the next restart.
    vi.useFakeTimers();
    try {
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "refresh") {
          return Promise.resolve({ ...snapshot, generation: 9, instances: [ollamaInstance(true)] });
        }
        return Promise.resolve(undefined);
      });
      const queryClient = newClient();
      const { result } = renderHook(() => useOpenOllamaApp(), { wrapper: wrapper(queryClient) });

      result.current.mutate();
      await vi.advanceTimersByTimeAsync(2000);

      expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "refresh")).toHaveLength(1);
      expect((queryClient.getQueryData(["snapshot"]) as Snapshot).generation).toBe(9);
    } finally {
      vi.useRealTimers();
    }
  });

  it("useOpenOllamaApp tries once more when the first look still finds Ollama down", async () => {
    // A cold start can take longer than the first grace period, and a single
    // attempt that lands too early leaves exactly the stale notice this fix
    // exists to clear. Bounded, though: two looks, then the notice stays and
    // the button can be pressed again.
    vi.useFakeTimers();
    try {
      let refreshes = 0;
      mockInvoke.mockImplementation((cmd: string) => {
        if (cmd === "refresh") {
          refreshes += 1;
          return Promise.resolve({
            ...snapshot,
            instances: [ollamaInstance(refreshes > 1)],
          });
        }
        return Promise.resolve(undefined);
      });
      const queryClient = newClient();
      const { result } = renderHook(() => useOpenOllamaApp(), { wrapper: wrapper(queryClient) });

      result.current.mutate();
      await vi.advanceTimersByTimeAsync(30_000);

      expect(refreshes).toBe(2);
    } finally {
      vi.useRealTimers();
    }
  });

  it("useSubmitOperation invalidates the operations query on success", async () => {
    mockInvoke.mockResolvedValueOnce(9 as never);
    const queryClient = newClient();
    const invalidateSpy = vi.spyOn(queryClient, "invalidateQueries");
    const { result } = renderHook(() => useSubmitOperation(), { wrapper: wrapper(queryClient) });

    result.current.mutate(1);

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ["operations"] });
  });
});
