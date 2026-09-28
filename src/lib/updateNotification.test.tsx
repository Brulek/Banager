import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { renderWithProviders } from "../test/setup";
import { OPEN_UPDATES_EVENT } from "./api";
import { queryKeys } from "./queries";
import { updatePairOf, useUpdateNotification } from "./updateNotification";
import { useUiStore } from "../store/ui";
import type {
  ArtifactKey,
  ManagerInstance,
  OpSummary,
  Settings,
  Snapshot,
  UpdateCandidate,
  UpdatePair,
} from "./types";

const mockInvoke = vi.mocked(invoke);

const brew: ManagerInstance = {
  id: "brew:/opt/homebrew",
  adapter_id: "brew",
  exe_path: "/opt/homebrew/bin/brew",
  prefix: "/opt/homebrew",
  scope: "User",
  version: "7.0.3",
  status: { unavailable: null, notes: [] },
  unverified_version: null,
  read_only_reason: null,
};

// pip: read-only by design, so its updates are listed and never offered.
const pip: ManagerInstance = {
  ...brew,
  id: "pip:/usr/bin/python3",
  adapter_id: "pip",
  exe_path: "/usr/bin/python3",
  prefix: "/usr",
  read_only_reason: "ByDesign",
};

function formula(name: string): ArtifactKey {
  return { instance_id: brew.id, kind: "Formula", name };
}

function update(key: ArtifactKey, target: string, overrides: Partial<UpdateCandidate> = {}): UpdateCandidate {
  return {
    key,
    current: "1.0.0",
    target,
    channel: "Native",
    checkable: true,
    warnings: [],
    blocked: null,
    ...overrides,
  };
}

const glib = update(formula("glib"), "2.86.1");
const wget = update(formula("wget"), "1.25.0");
const git = update(formula("git"), "2.52.0");

// Six updates listed; three of them Update all would take. jq is pinned,
// the user asked never to be reminded about ffmpeg, and pip's source is
// read-only.
const snapshot: Snapshot = {
  generation: 3,
  round: 5,
  detect: "Found",
  instances: [brew, pip],
  artifacts: [],
  updates: [
    glib,
    wget,
    git,
    update(formula("jq"), "1.8.1", { blocked: "Pinned" }),
    update(formula("ffmpeg"), "8.0"),
    update({ instance_id: pip.id, kind: "Package", name: "requests" }, "2.33.0"),
  ],
  refreshed_at: 1790586000,
  stale: false,
  errors: [],
};

const settings: Settings = {
  language: "System",
  show_technical_details: false,
  ignored_updates: [formula("ffmpeg")],
  skipped_versions: [],
  include_self_updating: false,
  auto_check: true,
  notify_updates: true,
};

/** What the backend has before its first check has answered (`Snapshot::empty()`). */
const startup: Snapshot = {
  generation: 0,
  round: 0,
  detect: "Missing",
  instances: [],
  artifacts: [],
  updates: [],
  refreshed_at: null,
  stale: false,
  errors: [],
};

let served: Snapshot;
let operations: OpSummary[];

beforeEach(() => {
  served = snapshot;
  operations = [];
  mockInvoke.mockReset();
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "get_snapshot") return Promise.resolve(served);
    if (cmd === "get_settings") return Promise.resolve(settings);
    if (cmd === "list_operations") return Promise.resolve(operations);
    return Promise.resolve(undefined);
  });
});

/** Every report the page sent, in order. */
function reports(): Array<{ round: number; updates: UpdatePair[] }> {
  return mockInvoke.mock.calls
    .filter(([cmd]) => cmd === "report_update_set")
    .map(([, args]) => args as { round: number; updates: UpdatePair[] });
}

function Shell() {
  useUpdateNotification();
  return null;
}

describe("the update notification's report", () => {
  it("names an update by its row's key and the version it offers", () => {
    expect(updatePairOf(glib)).toEqual({ key_id: "brew:/opt/homebrew|Formula|glib", target: "2.86.1" });
  });

  it("sends, after a snapshot, the rows Update all would take, with the snapshot's round", async () => {
    renderWithProviders(<Shell />);

    await waitFor(() => expect(reports()).toHaveLength(1));
    expect(reports()[0]).toEqual({ round: 5, updates: [glib, wget, git].map(updatePairOf) });
  });

  it("leaves out a row an update is taking, as Update all does", async () => {
    operations = [
      {
        id: 4,
        kind: "Upgrade",
        instance_id: brew.id,
        artifact_kind: "Formula",
        name: "wget",
        status: "Running",
        outcome: null,
        argv_preview: [],
        cancel_policy: "KillThenReconcile",
      },
    ];
    renderWithProviders(<Shell />);

    await waitFor(() => expect(reports()).toHaveLength(1));
    expect(reports()[0]).toEqual({ round: 5, updates: [glib, git].map(updatePairOf) });
  });

  it("sends nothing for the backend's startup snapshot, and the first round once it is in", async () => {
    served = startup;
    const { queryClient } = renderWithProviders(<Shell />);
    await waitFor(() => {
      expect(queryClient.getQueryData(queryKeys.snapshot)).toBe(startup);
      expect(queryClient.getQueryData(queryKeys.settings)).toBe(settings);
      expect(queryClient.getQueryData(queryKeys.operations)).toBe(operations);
    });
    expect(reports()).toEqual([]);

    act(() => {
      queryClient.setQueryData(queryKeys.snapshot, snapshot);
    });

    await waitFor(() => expect(reports()).toHaveLength(1));
    expect(reports()[0].round).toBe(5);
  });

  it("sends each round once: nothing more for the same round, and the next round's set when it comes", async () => {
    const { queryClient } = renderWithProviders(<Shell />);
    await waitFor(() => expect(reports()).toHaveLength(1));

    // Never remind me, on git: the same round, not reported again.
    act(() => {
      queryClient.setQueryData(queryKeys.settings, {
        ...settings,
        ignored_updates: [...settings.ignored_updates, formula("git")],
      });
    });
    // The next round -- the next day's check, finding the same updates.
    act(() => {
      queryClient.setQueryData(queryKeys.snapshot, { ...snapshot, round: 6 });
    });

    await waitFor(() => expect(reports()).toHaveLength(2));
    expect(reports()[1]).toEqual({ round: 6, updates: [glib, wget].map(updatePairOf) });
  });

  it("sends an empty set when no update can be started", async () => {
    served = { ...snapshot, updates: [update(formula("jq"), "1.8.1", { blocked: "Pinned" })] };
    renderWithProviders(<Shell />);

    await waitFor(() => expect(reports()).toHaveLength(1));
    expect(reports()[0]).toEqual({ round: 5, updates: [] });
  });
});

describe("a click on the update notification", () => {
  it("opens the Updates page", async () => {
    const handlers = new Map<string, EventCallback<unknown>>();
    vi.mocked(listen).mockImplementation(async (event, handler) => {
      handlers.set(event, handler as EventCallback<unknown>);
      return () => {
        handlers.delete(event);
      };
    });
    const { unmount } = renderWithProviders(<Shell />);
    await waitFor(() => expect(handlers.has(OPEN_UPDATES_EVENT)).toBe(true));
    expect(useUiStore.getState().page).toBe("overview");

    act(() => handlers.get(OPEN_UPDATES_EVENT)?.({ event: OPEN_UPDATES_EVENT, id: 1, payload: null }));

    expect(useUiStore.getState().page).toBe("updates");
    unmount();
    expect(handlers.has(OPEN_UPDATES_EVENT)).toBe(false);
  });
});
