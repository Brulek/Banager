import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { renderWithProviders } from "../test/setup";
import { watchDock } from "../test/dock";
import { Sidebar } from "../components/Sidebar";
import { useDockBadge } from "./dockBadge";
import { useSnoozeExpiry } from "./snoozeExpiry";
import { queryKeys } from "./queries";
import type {
  ArtifactKey,
  InstalledArtifact,
  ManagerInstance,
  Settings,
  Snapshot,
  UpdateCandidate,
} from "./types";
import { NO_FACTS } from "./types";

const mockInvoke = vi.mocked(invoke);

const brew: ManagerInstance = {
  id: "brew:/opt/homebrew",
  adapter_id: "brew",
  exe_path: "/opt/homebrew/bin/brew",
  prefix: "/opt/homebrew",
  scope: "User",
  version: "7.0.3",
  status: { unavailable: null, notes: [] },
  answered_at: null,
  unverified_version: null,
  read_only_reason: null,
};

function formula(name: string): ArtifactKey {
  return { instance_id: brew.id, kind: "Formula", name };
}

function update(name: string, overrides: Partial<UpdateCandidate> = {}): UpdateCandidate {
  return {
    key: formula(name),
    current: "1.0.0",
    target: "1.1.0",
    channel: "Native",
    checkable: true,
    warnings: [],
    blocked: null,
    ...overrides,
  };
}

function installed(name: string): InstalledArtifact {
  return {
    key: formula(name),
    display_name: name,
    version: "1.0.0",
    reason: "Requested",
    description: null,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
    facts: NO_FACTS,
  };
}

// Four updates listed, two of them on offer: jq is pinned, and the user
// asked never to be reminded about ffmpeg.
const snapshot: Snapshot = {
  generation: 3,
  round: 3,
  detect: "Found",
  instances: [brew],
  artifacts: [installed("glib"), installed("wget"), installed("jq")],
  updates: [update("glib"), update("wget"), update("jq", { blocked: "Pinned" }), update("ffmpeg")],
  refreshed_at: 1789700000,
  stale: false,
  errors: [],
};

const settings: Settings = {
  language: "System",
  show_technical_details: false,
  ignored_updates: [formula("ffmpeg")],
  skipped_versions: [],
  include_self_updating: false,
  auto_check: false,
  notify_updates: false,
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

beforeEach(() => {
  served = snapshot;
  mockInvoke.mockReset();
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "get_snapshot") return Promise.resolve(served);
    if (cmd === "get_settings") return Promise.resolve(settings);
    return Promise.resolve(undefined);
  });
});

/** The window as far as the badge goes: the sidebar, and the hook `App` mounts. */
function Shell() {
  useDockBadge();
  return <Sidebar page="overview" onSelectPage={() => {}} />;
}

describe("the Dock's badge", () => {
  it("shows the sidebar's Updates count: the updates the Updates page offers, not every one listed", async () => {
    const dock = watchDock();
    const { getByRole } = renderWithProviders(<Shell />);

    await waitFor(() => expect(dock.badge()).toBe(2));
    expect(getByRole("button", { name: "Updates" })).toHaveAccessibleDescription("2 can be updated");
  });

  it("shows none before the first check has answered, and the count once it has", async () => {
    served = startup;
    const dock = watchDock();
    const { getByRole, queryClient } = renderWithProviders(<Shell />);
    // The placeholder and the settings have both arrived.
    await waitFor(() => {
      expect(queryClient.getQueryData(queryKeys.snapshot)).toBe(startup);
      expect(queryClient.getQueryData(queryKeys.settings)).toBe(settings);
    });

    // Taken away as the page came up, never set: as the sidebar shows none.
    expect(dock.counts()).toEqual([undefined]);
    expect(getByRole("button", { name: "Updates" })).not.toHaveAttribute("aria-describedby");

    act(() => {
      queryClient.setQueryData(queryKeys.snapshot, snapshot);
    });

    await waitFor(() => expect(dock.badge()).toBe(2));
    expect(getByRole("button", { name: "Updates" })).toHaveAccessibleDescription("2 can be updated");
  });

  it("follows the count up and down, and is taken away at zero", async () => {
    const dock = watchDock();
    const { queryClient } = renderWithProviders(<Shell />);
    await waitFor(() => expect(dock.badge()).toBe(2));

    // A check finds one more.
    act(() => {
      queryClient.setQueryData(queryKeys.snapshot, {
        ...snapshot,
        generation: 4,
        updates: [...snapshot.updates, update("git")],
      });
    });
    await waitFor(() => expect(dock.badge()).toBe(3));

    // Never remind me, on glib.
    act(() => {
      queryClient.setQueryData(queryKeys.settings, {
        ...settings,
        ignored_updates: [...settings.ignored_updates, formula("glib")],
      });
    });
    await waitFor(() => expect(dock.badge()).toBe(2));

    // Everything updated.
    act(() => {
      queryClient.setQueryData(queryKeys.snapshot, { ...snapshot, generation: 5, updates: [] });
    });
    await waitFor(() => expect(dock.counts()).toEqual([undefined, 2, 3, 2, undefined]));
  });

  it("is left alone by a check that finds as many updates as before", async () => {
    const dock = watchDock();
    const { getByRole, queryClient } = renderWithProviders(<Shell />);
    await waitFor(() => expect(dock.badge()).toBe(2));

    // The same updates, and one more thing installed.
    act(() => {
      queryClient.setQueryData(queryKeys.snapshot, {
        ...snapshot,
        generation: 4,
        artifacts: [...snapshot.artifacts, installed("git")],
      });
    });
    await waitFor(() =>
      expect(getByRole("button", { name: "Installed" })).toHaveAccessibleDescription("4 installed"),
    );

    expect(dock.counts()).toEqual([undefined, 2]);
  });

  it("counts a snoozed tool again once its snooze runs out, with Banager left open", async () => {
    const dock = watchDock();
    // wget snoozed until a moment from now; the page left alone after.
    const until = Math.ceil(Date.now() / 1000) + 1;
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_snapshot") return Promise.resolve(served);
      if (cmd === "get_settings")
        return Promise.resolve({ ...settings, snoozed_updates: [{ key: formula("wget"), until }] });
      return Promise.resolve(undefined);
    });
    function Watched() {
      useDockBadge();
      useSnoozeExpiry();
      return <Sidebar page="overview" onSelectPage={() => {}} />;
    }
    const { getByRole } = renderWithProviders(<Watched />);

    await waitFor(() => expect(dock.badge()).toBe(1));
    expect(getByRole("button", { name: "Updates" })).toHaveAccessibleDescription("1 can be updated");
    // Real time: the snooze runs out about a second from now. The wait is
    // generous because a busy machine (several builds at once) has made a
    // 3 s wait time out before the page's own timer fired.
    await waitFor(() => expect(dock.badge()).toBe(2), { timeout: 10_000 });
    expect(getByRole("button", { name: "Updates" })).toHaveAccessibleDescription("2 can be updated");
  }, 15_000);

  it("leaves the page as it is when the Dock cannot be badged", async () => {
    const logged = vi.spyOn(console, "error").mockImplementation(() => {});
    watchDock();
    vi.mocked(getCurrentWindow().setBadgeCount).mockRejectedValue("window.set_badge_count not allowed");
    const { getByRole } = renderWithProviders(<Shell />);

    await waitFor(() =>
      expect(logged).toHaveBeenCalledWith(
        "set_badge_count failed",
        new Error("window.set_badge_count not allowed"),
      ),
    );
    await waitFor(() =>
      expect(getByRole("button", { name: "Updates" })).toHaveAccessibleDescription("2 can be updated"),
    );
    logged.mockRestore();
  });
});
