import { describe, expect, it } from "vitest";
import type { ReactNode } from "react";
import { act, renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { useHistory } from "./history";
import { usePasswordRecoveryKeys } from "./passwordRecovery";
import { queryKeys } from "./queryKeys";
import { artifactKeyId } from "../store/ui";
import type { ArtifactKey, HistoryRecord, HistoryView, OpSummary, Snapshot } from "./types";

const BREW = "brew:/opt/homebrew";
const onyx: ArtifactKey = { instance_id: BREW, kind: "Cask", name: "onyx" };

const snapshot: Snapshot = {
  generation: 1,
  round: 1,
  detect: "Found",
  instances: [],
  artifacts: [],
  updates: [{ key: onyx, current: "5.0.2", target: "5.1.0", channel: "Native", checkable: true, warnings: [], blocked: null }],
  refreshed_at: 1789700000,
  stale: false,
  errors: [],
};

/** The stop as Rust's `get_history` sends it: every record carries `dismissed`. */
function stop(fields: Partial<HistoryRecord> = {}): HistoryRecord {
  return {
    run: "earlier",
    op_id: 4,
    finished_at: Date.now() - 60_000,
    key: onyx,
    display_name: "onyx",
    adapter_id: "brew",
    kind: "Update",
    from_version: "5.0.2",
    to_version: null,
    result: { Failed: { cause: "needsPassword" } },
    verified: false,
    dismissed: false,
    ...fields,
  };
}

function render(history: HistoryView, operations: OpSummary[] = [], offering: Snapshot = snapshot) {
  const client = new QueryClient({ defaultOptions: { queries: { staleTime: Infinity, retry: false } } });
  client.setQueryData(queryKeys.snapshot, offering);
  client.setQueryData(queryKeys.operations, operations);
  client.setQueryData(queryKeys.history, history);
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
  // The history the hook read beside its answer, to wait for Clear's.
  return { client, ...renderHook(() => ({ keys: usePasswordRecoveryKeys(), history: useHistory().data }), { wrapper }) };
}

describe("usePasswordRecoveryKeys", () => {
  it("keeps a recorded password stop after Clear marks every record dismissed (r22 W1)", async () => {
    const { client, result } = render({ run: "now", cleared_before: null, records: [stop()] });
    expect([...result.current.keys]).toEqual([artifactKeyId(onyx)]);

    // What `clear_history` answers, from Rust's `HistoryStore::clear`.
    act(() => {
      client.setQueryData<HistoryView>(queryKeys.history, {
        run: "now",
        cleared_before: Date.now(),
        records: [stop({ dismissed: true })],
      });
    });
    await waitFor(() => expect(result.current.history?.records[0].dismissed).toBe(true));
    expect([...result.current.keys]).toEqual([artifactKeyId(onyx)]);
  });

  it("keeps it after a restart that reads the dismissed record back", () => {
    const { result } = render({ run: "later", cleared_before: Date.now() - 1_000, records: [stop({ dismissed: true })] });
    expect([...result.current.keys]).toEqual([artifactKeyId(onyx)]);
  });

  it("lets a later update that worked end it, even one Clear dismissed too", () => {
    const { result } = render({
      run: "now",
      cleared_before: Date.now(),
      records: [
        stop({ op_id: 5, finished_at: Date.now() - 1_000, result: "Succeeded", to_version: "5.1.0", verified: true, dismissed: true }),
        stop({ dismissed: true }),
      ],
    });
    expect(result.current.keys.size).toBe(0);
  });

  it("is kept per tool, not per version: a newer version offered within the window still gets View Steps", () => {
    // Updated in Terminal from 5.0.2 to 5.1.0 since; the source now offers
    // 5.2.0. The step that asked for the password is the cask's own, and
    // View Steps plans whichever update is offered.
    const newer: Snapshot = {
      ...snapshot,
      updates: [{ ...snapshot.updates[0], current: "5.1.0", target: "5.2.0" }],
    };
    for (const dismissed of [false, true]) {
      const { result, unmount } = render(
        { run: "later", cleared_before: null, records: [stop({ finished_at: Date.now() - 10 * 24 * 60 * 60 * 1_000, dismissed })] },
        [],
        newer,
      );
      expect([...result.current.keys]).toEqual([artifactKeyId(onyx)]);
      unmount();
    }
  });
});
