import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, waitFor } from "@testing-library/react";
import type { QueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { finishedRunOf, runEnded, useOperationsNotification } from "./operationsNotification";
import { queryKeys } from "./queryKeys";
import type { FinishedRun, OpKind, OpStatus, OpSummary, Outcome } from "./types";
import { failureCause } from "./failureCause";

const mockInvoke = vi.mocked(invoke);

let operations: OpSummary[];

function op(id: number, status: OpStatus, outcome: Outcome | null = null, kind: OpKind = "Upgrade"): OpSummary {
  return {
    id,
    kind,
    instance_id: "brew:/opt/homebrew",
    artifact_kind: "Formula",
    name: `tool${id}`,
    status,
    outcome,
    argv_preview: [],
    env_preview: [],
    cancel_policy: "KillThenReconcile",
  };
}

const failed: Outcome = { Failed: { exit_code: 1, summary: "no network", cause: failureCause("no network") } };

/** A short wait for a run to settle, so the tests need no fake clock. */
const SETTLE_MS = 50;

function Watcher() {
  useOperationsNotification(SETTLE_MS);
  return null;
}

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/** The runs reported, in order. */
function reported(): FinishedRun[] {
  return mockInvoke.mock.calls
    .filter(([cmd]) => cmd === "report_finished_run")
    .map(([, args]) => (args as { run: FinishedRun }).run);
}

async function listNow(queryClient: QueryClient, next: OpSummary[]) {
  operations = next;
  await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.operations }));
}

beforeEach(() => {
  operations = [];
  mockInvoke.mockReset();
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "list_operations") return Promise.resolve(operations);
    return Promise.resolve(undefined);
  });
});

describe("finishedRunOf", () => {
  it("counts how a run's operations ended, and leaves the cancelled ones out", () => {
    expect(
      finishedRunOf([
        op(3, "Done", "Succeeded"),
        op(4, "Done", failed),
        op(5, "Done", "Unconfirmed"),
        op(6, "Done", "Cancelled"),
        op(7, "Done", "Succeeded"),
      ]),
    ).toEqual({ last_op: 7, kind: "Upgrade", succeeded: 2, failed: 1, attention: 1 });
  });

  it("names a run of uninstalls as one, and a mixed run as other", () => {
    expect(finishedRunOf([op(1, "Done", "Succeeded", "Uninstall")])?.kind).toBe("Uninstall");
    expect(finishedRunOf([op(1, "Done", "Succeeded", "Uninstall"), op(2, "Done", "Succeeded")])?.kind).toBe(
      "Other",
    );
    expect(finishedRunOf([])).toBeNull();
  });
});

describe("runEnded", () => {
  it("is a run seen under way and now done, or one started and done between two looks, never the first look", () => {
    expect(runEnded(null, { floor: 0, seen: 2, open: false })).toBe(false);
    expect(runEnded({ floor: 0, seen: 2, open: true }, { floor: 0, seen: 2, open: false })).toBe(true);
    expect(runEnded({ floor: 0, seen: 2, open: true }, { floor: 0, seen: 3, open: true })).toBe(false);
    expect(runEnded({ floor: 0, seen: 2, open: false }, { floor: 2, seen: 4, open: false })).toBe(true);
    expect(runEnded({ floor: 0, seen: 2, open: false }, { floor: 0, seen: 2, open: false })).toBe(false);
  });
});

describe("useOperationsNotification", () => {
  it("reports an Update all once, when its last operation has finished", async () => {
    const { queryClient } = renderWithProviders(<Watcher />);
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));

    await listNow(queryClient, [op(1, "Running"), op(2, "Queued"), op(3, "Queued")]);
    await listNow(queryClient, [op(1, "Done", "Succeeded"), op(2, "Running"), op(3, "Queued")]);
    expect(reported()).toEqual([]);
    await listNow(queryClient, [op(1, "Done", "Succeeded"), op(2, "Done", failed), op(3, "Done", "Succeeded")]);
    await waitFor(() =>
      expect(reported()).toEqual([{ last_op: 3, kind: "Upgrade", succeeded: 2, failed: 1, attention: 0 }]),
    );
    // Asked again, with nothing new: nothing more.
    await listNow(queryClient, [op(1, "Done", "Succeeded"), op(2, "Done", failed), op(3, "Done", "Succeeded")]);
    expect(reported()).toHaveLength(1);

    // The next run is its own.
    const done = [op(1, "Done", "Succeeded"), op(2, "Done", failed), op(3, "Done", "Succeeded")];
    await listNow(queryClient, [...done, op(4, "Running", null, "Uninstall")]);
    await listNow(queryClient, [...done, op(4, "Done", "Succeeded", "Uninstall")]);
    await waitFor(() =>
      expect(reported()[reported().length - 1]).toEqual({ last_op: 4, kind: "Uninstall", succeeded: 1, failed: 0, attention: 0 }),
    );
  });

  it("reports an Update all once when its first operation fails before the next is submitted", async () => {
    const { queryClient } = renderWithProviders(<Watcher />);
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));

    // The first fails at once: for a moment nothing is under way.
    await listNow(queryClient, [op(1, "Running")]);
    await listNow(queryClient, [op(1, "Done", failed)]);
    // The next two are submitted within the wait.
    await listNow(queryClient, [op(1, "Done", failed), op(2, "Running"), op(3, "Queued")]);
    await sleep(SETTLE_MS * 2);
    expect(reported()).toEqual([]);
    await listNow(queryClient, [op(1, "Done", failed), op(2, "Done", "Succeeded"), op(3, "Done", "Succeeded")]);
    await waitFor(() =>
      expect(reported()).toEqual([{ last_op: 3, kind: "Upgrade", succeeded: 2, failed: 1, attention: 0 }]),
    );
    await sleep(SETTLE_MS * 2);
    expect(reported()).toHaveLength(1);
  });

  it("reports nothing that had finished before the page first looked", async () => {
    operations = [op(1, "Done", "Succeeded")];
    const { queryClient } = renderWithProviders(<Watcher />);
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toHaveLength(1));
    expect(reported()).toEqual([]);
  });
});
