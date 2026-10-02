import { describe, expect, it, vi } from "vitest";
import type { ReactNode } from "react";
import { act, renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { queryKeys } from "../lib/queryKeys";
import { actionableUpdatesOf } from "../lib/updateState";
import type { OpStatus, OpSummary, Settings, Snapshot, UpdateCandidate } from "../lib/types";
import { isRetryable, passwordStepsOpId, useStartableUpdates, type RowProgress } from "./UpdateProgress";

// The real rule, watched: how often the hook works out what the page offers.
vi.mock("../lib/updateState", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../lib/updateState")>();
  return { ...actual, actionableUpdatesOf: vi.fn(actual.actionableUpdatesOf) };
});

const BREW = "brew:/opt/homebrew";

function update(name: string): UpdateCandidate {
  return {
    key: { instance_id: BREW, kind: "Formula", name },
    current: "1.0.0",
    target: "1.1.0",
    channel: "Native",
    checkable: true,
    warnings: [],
    blocked: null,
  };
}

const snapshot: Snapshot = {
  generation: 1,
  round: 1,
  detect: "Found",
  instances: [
    {
      id: BREW,
      adapter_id: "brew",
      exe_path: "/opt/homebrew/bin/brew",
      prefix: "/opt/homebrew",
      scope: "User",
      version: "7.0.3",
      status: { unavailable: null, notes: [] },
      answered_at: null,
      unverified_version: null,
      read_only_reason: null,
    },
  ],
  artifacts: [],
  updates: [update("glib"), update("wget")],
  refreshed_at: 1789700000,
  stale: false,
  errors: [],
};

const settings: Settings = {
  language: "System",
  show_technical_details: false,
  ignored_updates: [],
  skipped_versions: [],
  include_self_updating: false,
  auto_check: false,
  notify_updates: false,
};

function upgradeOf(name: string, status: OpStatus): OpSummary {
  return {
    id: 1,
    kind: "Upgrade",
    instance_id: BREW,
    artifact_kind: "Formula",
    name,
    status,
    outcome: null,
    argv_preview: [],
    cancel_policy: "KillThenReconcile",
  };
}

describe("useStartableUpdates", () => {
  it("works out what the page offers once per snapshot and settings, however often the operations move", async () => {
    // Everything in the cache already, and nothing asked of the backend.
    const client = new QueryClient({ defaultOptions: { queries: { staleTime: Infinity, retry: false } } });
    client.setQueryData(queryKeys.snapshot, snapshot);
    client.setQueryData(queryKeys.settings, settings);
    client.setQueryData(queryKeys.operations, []);
    const wrapper = ({ children }: { children: ReactNode }) => (
      <QueryClientProvider client={client}>{children}</QueryClientProvider>
    );
    const { result } = renderHook(() => useStartableUpdates(), { wrapper });
    const names = () => result.current?.map((candidate) => candidate.key.name);

    expect(names()).toEqual(["glib", "wget"]);
    const workedOut = vi.mocked(actionableUpdatesOf).mock.calls.length;

    // glib's update is queued, then runs: its row is taken either way.
    act(() => {
      client.setQueryData(queryKeys.operations, [upgradeOf("glib", "Queued")]);
    });
    await waitFor(() => expect(names()).toEqual(["wget"]));
    act(() => {
      client.setQueryData(queryKeys.operations, [upgradeOf("glib", "Running")]);
    });
    await waitFor(() => expect(result.current?.length).toBe(1));
    expect(vi.mocked(actionableUpdatesOf).mock.calls.length).toBe(workedOut);

    // A check that finds wget up to date is a new snapshot: worked out again.
    act(() => {
      client.setQueryData(queryKeys.snapshot, { ...snapshot, generation: 2, updates: [update("glib")] });
    });
    await waitFor(() => expect(names()).toEqual([]));
    expect(vi.mocked(actionableUpdatesOf).mock.calls.length).toBe(workedOut + 1);
  });
});

describe("passwordStepsOpId", () => {
  it("names the operation only of a failure that stopped at sudo's password, and that one alone is not retried", () => {
    const every: RowProgress[] = [
      { kind: "queued" },
      { kind: "running" },
      { kind: "cancelling" },
      { kind: "succeeded" },
      { kind: "cancelled" },
      { kind: "check", opId: 3 },
      { kind: "failed", opId: 4, cause: null },
      { kind: "failed", opId: 5, cause: "network" },
      { kind: "failed", opId: 6, cause: "passwordNotAccepted" },
      { kind: "failed", opId: 7, cause: "needsPassword" },
    ];
    expect(every.map(passwordStepsOpId)).toEqual([null, null, null, null, null, null, null, null, null, 7]);
    expect(passwordStepsOpId(null)).toBeNull();
    for (const progress of every.filter((each) => each.kind === "failed")) {
      expect(isRetryable(progress), JSON.stringify(progress)).toBe(passwordStepsOpId(progress) === null);
    }
  });
});
