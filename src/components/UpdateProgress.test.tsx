import { describe, expect, it, vi } from "vitest";
import type { ReactNode } from "react";
import { act, renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import i18n from "../i18n";
import { useUiStore } from "../store/ui";
import { queryKeys } from "../lib/queryKeys";
import { actionableUpdatesOf } from "../lib/updateState";
import type { OpStatus, OpSummary, Settings, Snapshot, UpdateCandidate } from "../lib/types";
import {
  isRetryable,
  passwordStepsOpId,
  progressOf,
  useStartableUpdates,
  useCountedUpdates,
  useUpdateOperationFor,
  progressWord,
  isUnderway,
  waitsForPassword,
  type RowProgress,
} from "./UpdateProgress";

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

describe("waitsForPassword", () => {
  it("offers the password steps for an update whose summary lost sudo's words to a masked login (re-check 2's N1)", () => {
    // A proxy password `pass`: the core masked it inside sudo's
    // "password", and read the cause before it did.
    const masked = "sudo: a ****word is required";
    const stopped: OpSummary = {
      ...upgradeOf("glib", "Done"),
      id: 9,
      outcome: { Failed: { exit_code: 1, summary: masked, cause: "needsPassword" } },
    };
    expect(progressOf(stopped)).toEqual({ kind: "failed", opId: 9, cause: "needsPassword" });
    expect(waitsForPassword(stopped)).toBe(true);
    expect(isRetryable(progressOf(stopped))).toBe(false);
    // The summary alone says nothing: the row reads the cause.
    const unread: OpSummary = { ...stopped, outcome: { Failed: { exit_code: 1, summary: "sudo: a password is required", cause: null } } };
    expect(waitsForPassword(unread)).toBe(false);
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

describe("updates held by another action", () => {
  it.each(["Queued", "Running", "Verifying", "CancelRequested", "Cancelling"] as const)(
    "excludes a %s uninstall by full key and names its action",
    (status) => {
      const client = new QueryClient({ defaultOptions: { queries: { staleTime: Infinity } } });
      const uninstall: OpSummary = { ...upgradeOf("glib", status), kind: "Uninstall", id: 2 };
      // A newer completed upgrade must not hide the pending uninstall.
      const completed = { ...upgradeOf("glib", "Done"), id: 3, outcome: "Succeeded" as const };
      useUiStore.getState().rememberUpdateTarget(3, "1.0.9");
      const otherKind = { ...update("glib"), key: { ...update("glib").key, kind: "Cask" as const } };
      const otherSource = { ...update("glib"), key: { ...update("glib").key, instance_id: "brew:/usr/local" } };
      const data = { ...snapshot, updates: [...snapshot.updates, otherKind, otherSource],
        instances: [...snapshot.instances, { ...snapshot.instances[0], id: otherSource.key.instance_id }] };
      client.setQueryData(queryKeys.snapshot, data);
      client.setQueryData(queryKeys.settings, settings);
      client.setQueryData(queryKeys.operations, [uninstall, completed]);
      const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
      const { result } = renderHook(() => ({ startable: useStartableUpdates(), counted: useCountedUpdates(), operationFor: useUpdateOperationFor() }), { wrapper });
      expect(result.current.startable).toEqual([update("wget"), otherKind, otherSource]);
      expect(result.current.counted).toEqual([update("wget"), otherKind, otherSource]);
      expect(result.current.operationFor(update("glib"))).toEqual(uninstall);
      expect(isUnderway(uninstall)).toBe(false); // Never counted as updating.
      for (const language of ["en", "zh-CN", "zh-Hant"]) {
        const t = i18n.getFixedT(language);
        expect(progressWord(t, progressOf(uninstall))).toContain(
          t(status === "Running" ? "operations.running.Uninstall" : "operations.kind.Uninstall"),
        );
      }
    },
  );

  it("does not show a completed uninstall as an update outcome", () => {
    const client = new QueryClient({ defaultOptions: { queries: { staleTime: Infinity } } });
    const completed = { ...upgradeOf("glib", "Done"), kind: "Uninstall" as const, outcome: "Succeeded" as const };
    useUiStore.getState().rememberUpdateTarget(completed.id, "1.1.0");
    client.setQueryData(queryKeys.operations, [completed]);
    const wrapper = ({ children }: { children: ReactNode }) => <QueryClientProvider client={client}>{children}</QueryClientProvider>;
    const { result } = renderHook(useUpdateOperationFor, { wrapper });
    expect(result.current(update("glib"))).toBeNull();
  });
});

it("keeps a successful update with a follow-up warning non-retryable and gives it a log", () => {
  const op = { ...upgradeOf("node@22", "Done"), outcome: "Succeeded" as const,
    follow_up_warnings: [{ NoLongerLinked: { name: "node@22", commands: ["node"] } }],
  };
  const progress = progressOf(JSON.parse(JSON.stringify(op)));
  expect(progress).toEqual({ kind: "succeeded", warningOpId: op.id, warnings: 1 });
  expect(isRetryable(progress)).toBe(false);
});
