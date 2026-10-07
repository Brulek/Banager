import { describe, expect, it } from "vitest";
import type { ReactNode } from "react";
import { act, renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { useHistory } from "./history";
import { usePasswordRecoveryKeys } from "./passwordRecovery";
import { useUpdateOperationFor } from "../components/UpdateProgress";
import { queryKeys } from "./queryKeys";
import { artifactKeyId, useUiStore } from "../store/ui";
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
  return {
    client,
    ...renderHook(() => ({ keys: usePasswordRecoveryKeys(useUpdateOperationFor()), history: useHistory().data }), { wrapper }),
  };
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

  // o3 skeptic 1: what docs/what-we-run.md says supersedes a recorded
  // stop -- a later update the history kept, or a later uninstall it kept
  // that worked; not one that failed, was stopped or is unconfirmed
  // (`recordWeighs`, r35 U4).
  describe("a later record of the same tool", () => {
    const uninstall = (result: HistoryRecord["result"]) =>
      stop({ op_id: 5, finished_at: Date.now() - 1_000, kind: "Uninstall", to_version: null, result });

    it.each([
      ["failed", { Failed: { cause: "permission" } }],
      ["was stopped", "Cancelled"],
      ["is unconfirmed", "Unconfirmed"],
    ] as const)("leaves the stop counted after an uninstall that %s", (_how, result) => {
      const { result: hook } = render({ run: "later", cleared_before: null, records: [uninstall(result), stop()] });
      expect([...hook.current.keys]).toEqual([artifactKeyId(onyx)]);
    });

    it("ends it with an uninstall that worked", () => {
      const { result } = render({ run: "later", cleared_before: null, records: [uninstall("Succeeded"), stop()] });
      expect(result.current.keys.size).toBe(0);
    });

    it.each([
      ["worked", "Succeeded"],
      ["was stopped", "Cancelled"],
      ["failed otherwise", { Failed: { cause: "network" } }],
    ] as const)("ends it with an update that %s", (_how, result) => {
      const { result: hook } = render({
        run: "later",
        cleared_before: null,
        records: [stop({ op_id: 5, finished_at: Date.now() - 1_000, result }), stop()],
      });
      expect(hook.current.keys.size).toBe(0);
    });
  });

  // o3 skeptic 1: any operation of the tool under way holds its row
  // (`useUpdateOperationFor`), a Fix… link of a formula too; once that
  // link has finished, the row shows it no longer and the stop counts.
  it("leaves the tool to its row while a Fix… link of it is under way, and counts the stop again once it has finished", () => {
    const formula: ArtifactKey = { instance_id: BREW, kind: "Formula", name: "node@22" };
    const offering: Snapshot = {
      ...snapshot,
      updates: [{ ...snapshot.updates[0], key: formula, current: "22.23.2", target: "22.23.3" }],
    };
    const history: HistoryView = {
      run: "now",
      cleared_before: null,
      records: [stop({ key: formula, display_name: "node@22", from_version: "22.23.2" })],
    };
    const link = (fields: Partial<OpSummary>): OpSummary => ({
      id: 9,
      kind: "Link",
      instance_id: BREW,
      artifact_kind: "Formula",
      name: "node@22",
      status: "Running",
      outcome: null,
      argv_preview: ["/opt/homebrew/bin/brew", "link", "--formula", "--force", "node@22"],
      cancel_policy: "KillThenReconcile",
      ...fields,
    });
    const running = render(history, [link({})], offering);
    expect(running.result.current.keys.size).toBe(0);
    running.unmount();
    const done = render(history, [link({ status: "Done", outcome: "Succeeded" })], offering);
    expect([...done.result.current.keys]).toEqual([artifactKeyId(formula)]);
  });

  describe("an operation of this launch (r35 U3)", () => {
    // Op 7, this launch's update of onyx, stopped where sudo wanted the
    // password; the history kept it before the operation said Done.
    const op7: OpSummary = {
      id: 7,
      kind: "Upgrade",
      instance_id: BREW,
      artifact_kind: "Cask",
      name: "onyx",
      status: "Done",
      outcome: { Failed: { exit_code: 1, summary: "sudo: a terminal is required to read the password", cause: "needsPassword" } },
      argv_preview: ["/opt/homebrew/bin/brew", "upgrade", "--cask", "onyx"],
      cancel_policy: "KillThenReconcile",
    };
    const record = () => stop({ run: "now", op_id: 7, finished_at: Date.now() - 1_000 });

    it("leaves the tool to its row while the row shows the operation", () => {
      useUiStore.getState().rememberUpdateTarget(7, "5.1.0");
      const { result } = render({ run: "now", cleared_before: null, records: [record()] }, [op7]);
      expect(result.current.keys.size).toBe(0);
    });

    it("counts it from its record once the row no longer shows it -- the page reloaded, the backend still lists op 7", () => {
      // `updateTargets` lives in the page's memory: empty after a reload.
      expect(useUiStore.getState().updateTargets).toEqual({});
      const { result } = render({ run: "now", cleared_before: null, records: [record()] }, [op7]);
      expect([...result.current.keys]).toEqual([artifactKeyId(onyx)]);
    });

    it("leaves it to its row while another update of it is under way there", () => {
      const running: OpSummary = { ...op7, id: 8, status: "Running", outcome: null };
      const { result } = render({ run: "now", cleared_before: null, records: [record()] }, [running, op7]);
      expect(result.current.keys.size).toBe(0);
    });
  });
});
