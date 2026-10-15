import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactNode } from "react";
import { act, renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { HISTORY_POLL_MS, historyPollInterval, useClearHistory, useHistory, useHistoryClearState } from "./history";
import { queryKeys } from "./queryKeys";
import { useUiStore } from "../store/ui";
import type { HistoryPersistence, HistoryView } from "./types";

const mockInvoke = vi.mocked(invoke);

function view(persistence: HistoryPersistence, fields: Partial<HistoryView> = {}): HistoryView {
  return { run: "now", cleared_before: null, records: [], persistence, unsaved_clear: false, ...fields };
}

function deferred<T>() {
  let resolve: (value: T) => void = () => {};
  let reject: (error: unknown) => void = () => {};
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

function wrapperFor(client: QueryClient) {
  return ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
}

function newClient() {
  return new QueryClient({ defaultOptions: { queries: { retry: false } } });
}

const calls = (cmd: string) => mockInvoke.mock.calls.filter(([name]) => name === cmd).length;

beforeEach(() => {
  mockInvoke.mockReset();
  useHistoryClearState.setState({ unconfirmed: false });
  useUiStore.setState({ clearedJustUpdated: [] });
});

afterEach(() => {
  vi.useRealTimers();
});

describe("historyPollInterval", () => {
  it("asks again only while a write is under way or failed", () => {
    expect(historyPollInterval(view("pending"))).toBe(HISTORY_POLL_MS);
    expect(historyPollInterval(view("write_failed"))).toBe(HISTORY_POLL_MS);
    for (const done of ["saved", "unreadable", "malformed", "newer", "writer_unavailable", "unattached"] as const) {
      expect(historyPollInterval(view(done))).toBe(false);
    }
    expect(historyPollInterval(undefined)).toBe(false);
  });
});

describe("useHistory", () => {
  it("polls get_history while the write failed, never Clear, stops once saved, and keeps the last answer when an ask fails", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const answers: (() => Promise<unknown>)[] = [
      () => Promise.resolve(view("write_failed")),
      () => Promise.reject(new Error("no answer")),
      () => Promise.resolve(view("saved")),
    ];
    mockInvoke.mockImplementation((cmd: string) =>
      cmd === "get_history" ? (answers.shift() ?? (() => Promise.resolve(view("saved"))))() : Promise.resolve(undefined),
    );
    const client = newClient();
    const { result, unmount } = renderHook(() => useHistory(), { wrapper: wrapperFor(client) });
    await waitFor(() => expect(result.current.data?.persistence).toBe("write_failed"));
    await act(async () => vi.advanceTimersByTimeAsync(HISTORY_POLL_MS));
    await waitFor(() => expect(calls("get_history")).toBe(2));
    // The failed ask leaves what the notice says as it was.
    expect(result.current.data?.persistence).toBe("write_failed");
    await act(async () => vi.advanceTimersByTimeAsync(HISTORY_POLL_MS));
    await waitFor(() => expect(result.current.data?.persistence).toBe("saved"));
    await act(async () => vi.advanceTimersByTimeAsync(HISTORY_POLL_MS * 3));
    expect(calls("get_history")).toBe(3);
    expect(calls("clear_history")).toBe(0);
    unmount();
  });
});

describe("useClearHistory", () => {
  it("sends a Clear that fails once, says it is unconfirmed, hides nothing of this window's, and asks what Banager has", async () => {
    mockInvoke.mockImplementation((cmd: string) =>
      cmd === "clear_history" ? Promise.reject(new Error("lost")) : Promise.resolve(view("saved")),
    );
    const client = newClient();
    client.setQueryData(queryKeys.history, view("saved"));
    const { result } = renderHook(() => useClearHistory(), { wrapper: wrapperFor(client) });
    await act(async () => {
      result.current.mutate([7]);
    });
    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(calls("clear_history")).toBe(1);
    expect(useHistoryClearState.getState().unconfirmed).toBe(true);
    expect(useUiStore.getState().clearedJustUpdated).toEqual([]);
    await waitFor(() => expect(client.getQueryState(queryKeys.history)?.isInvalidated).toBe(true));
  });

  it("takes this window's off the list only once Banager has taken the Clear, and only those listed when it was pressed", async () => {
    const clear = deferred<HistoryView>();
    mockInvoke.mockImplementation((cmd: string) => (cmd === "clear_history" ? clear.promise : Promise.resolve(view("saved"))));
    const client = newClient();
    const { result } = renderHook(() => useClearHistory(), { wrapper: wrapperFor(client) });
    act(() => result.current.mutate([7]));
    await waitFor(() => expect(result.current.isPending).toBe(true));
    expect(useUiStore.getState().clearedJustUpdated).toEqual([]);
    useHistoryClearState.setState({ unconfirmed: true });
    await act(async () => clear.resolve(view("pending", { unsaved_clear: true })));
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(useUiStore.getState().clearedJustUpdated).toEqual([7]);
    expect(useHistoryClearState.getState().unconfirmed).toBe(false);
  });

  it("does not let an ask started during the Clear answer over the Clear's own answer", async () => {
    const stale = deferred<HistoryView>();
    const clear = deferred<HistoryView>();
    let asks = 0;
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "clear_history") return clear.promise;
      if (cmd === "get_history") {
        asks += 1;
        // The first answers; the second is the stale one; the ask after
        // the Clear never answers, so that only the Clear's own answer can
        // put 9 in the cache.
        if (asks === 1) return Promise.resolve(view("saved"));
        return asks === 2 ? stale.promise : new Promise(() => {});
      }
      return Promise.resolve(undefined);
    });
    const client = newClient();
    const { result } = renderHook(() => ({ history: useHistory(), clear: useClearHistory() }), {
      wrapper: wrapperFor(client),
    });
    await waitFor(() => expect(result.current.history.data?.persistence).toBe("saved"));
    act(() => result.current.clear.mutate([]));
    await waitFor(() => expect(result.current.clear.isPending).toBe(true));
    // A `Finished` event's ask, while the Clear is under way.
    act(() => {
      void client.invalidateQueries({ queryKey: queryKeys.history });
    });
    await waitFor(() => expect(asks).toBe(2));
    await act(async () => clear.resolve(view("saved", { cleared_before: 9 })));
    await waitFor(() => expect(result.current.clear.isSuccess).toBe(true));
    // The ask from before the Clear answers last, with what was there before it.
    expect(result.current.history.data?.cleared_before).toBe(9);
    await act(async () => {
      stale.resolve(view("saved", { cleared_before: null }));
      await new Promise((settle) => setTimeout(settle, 20));
    });
    expect(result.current.history.data?.cleared_before).toBe(9);
    expect(calls("clear_history")).toBe(1);
  });
});
