import { describe, expect, it, vi, beforeEach } from "vitest";
import { screen, fireEvent, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { SnapshotStatus } from "./SnapshotStatus";
import { useUiStore } from "../store/ui";
import type { Snapshot } from "../lib/types";

function baseSnapshot(overrides: Partial<Snapshot> = {}): Snapshot {
  return {
    generation: 1,
    detect: "Found",
    instances: [],
    artifacts: [],
    updates: [],
    refreshed_at: 1700000000,
    stale: false,
    errors: [],
    ...overrides,
  };
}

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

describe("SnapshotStatus", () => {
  it("shows the no-Homebrew empty state and hides children when detect is Missing", async () => {
    vi.mocked(invoke).mockResolvedValue(baseSnapshot({ detect: "Missing" }));

    renderWithProviders(
      <SnapshotStatus>
        <p>installed list</p>
      </SnapshotStatus>,
    );

    expect(await screen.findByText("Homebrew isn't installed yet")).toBeInTheDocument();
    expect(screen.queryByText("installed list")).not.toBeInTheDocument();
  });

  it("shows loading, not the no-Homebrew state, before the first refresh has completed", async () => {
    // Session boots with Snapshot::empty(): generation 0, detect Missing,
    // refreshed_at null. Only a completed refresh ever sets refreshed_at —
    // including a refresh that finds Homebrew genuinely missing.
    vi.mocked(invoke).mockResolvedValue(
      baseSnapshot({ generation: 0, detect: "Missing", refreshed_at: null }),
    );

    renderWithProviders(
      <SnapshotStatus>
        <p>installed list</p>
      </SnapshotStatus>,
    );

    expect(await screen.findByText("Loading…")).toBeInTheDocument();
    expect(screen.queryByText("Homebrew isn't installed yet")).not.toBeInTheDocument();
  });

  it("shows the load-failure surface instead of Loading… when the startup refresh has failed", async () => {
    // get_snapshot itself succeeded with the empty startup snapshot, but the
    // startup refresh() IPC call rejected — refreshed_at will never be set.
    vi.mocked(invoke).mockResolvedValue(
      baseSnapshot({ generation: 0, detect: "Missing", refreshed_at: null }),
    );
    useUiStore.setState({ startupRefreshError: "brew: command not found" });

    renderWithProviders(
      <SnapshotStatus>
        <p>installed list</p>
      </SnapshotStatus>,
    );

    expect(await screen.findByText("Couldn't load what's installed")).toBeInTheDocument();
    expect(screen.getByText(/brew: command not found/)).toBeInTheDocument();
    expect(screen.queryByText("Loading…")).not.toBeInTheDocument();
  });

  it("shows the backend's error verbatim when the snapshot itself cannot be loaded", async () => {
    // get_snapshot rejects with a bare string (Task 10's call() turns it
    // into an Error); without this branch the page would be blank.
    vi.mocked(invoke).mockRejectedValue("brew: command not found" as never);

    renderWithProviders(
      <SnapshotStatus>
        <p>installed list</p>
      </SnapshotStatus>,
    );

    expect(await screen.findByText("Couldn't load what's installed")).toBeInTheDocument();
    expect(screen.getByText(/brew: command not found/)).toBeInTheDocument();
    expect(screen.queryByText("installed list")).not.toBeInTheDocument();
  });

  it("shows the root-refusal empty state when detect is RefusedAsRoot", async () => {
    vi.mocked(invoke).mockResolvedValue(baseSnapshot({ detect: "RefusedAsRoot" }));

    renderWithProviders(
      <SnapshotStatus>
        <p>installed list</p>
      </SnapshotStatus>,
    );

    expect(
      await screen.findByText("Canager can't run as an administrator"),
    ).toBeInTheDocument();
  });

  it("shows a stale banner above the existing data when the last refresh failed", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_snapshot") {
        return baseSnapshot({
          stale: true,
          errors: [{ instance_id: "brew:/opt/homebrew", message: "timed out" }],
        });
      }
      if (cmd === "refresh") return baseSnapshot();
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(
      <SnapshotStatus>
        <p>installed list</p>
      </SnapshotStatus>,
    );

    expect(await screen.findByText("Some data might be out of date")).toBeInTheDocument();
    expect(screen.getByText("installed list")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() =>
      expect(vi.mocked(invoke).mock.calls.some(([cmd]) => cmd === "refresh")).toBe(true),
    );
  });

  it("shows the nothing-installed empty state when there are no artifacts", async () => {
    vi.mocked(invoke).mockResolvedValue(baseSnapshot({ artifacts: [] }));

    renderWithProviders(
      <SnapshotStatus>
        <p>installed list</p>
      </SnapshotStatus>,
    );

    expect(await screen.findByText("Nothing installed yet")).toBeInTheDocument();
  });

  it("renders children unchanged once something is installed", async () => {
    vi.mocked(invoke).mockResolvedValue(
      baseSnapshot({
        artifacts: [
          {
            key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "jq" },
            display_name: "jq",
            version: "1.7",
            reason: "Requested",
            description: null,
            homepage: null,
            size_bytes: null,
            installed_at: null,
            path: null,
            auto_updates: false,
          },
        ],
      }),
    );

    renderWithProviders(
      <SnapshotStatus>
        <p>installed list</p>
      </SnapshotStatus>,
    );

    expect(await screen.findByText("installed list")).toBeInTheDocument();
  });
});
