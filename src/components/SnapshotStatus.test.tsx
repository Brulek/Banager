import { describe, expect, it, vi, beforeEach } from "vitest";
import { screen, fireEvent, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { SnapshotStatus } from "./SnapshotStatus";
import { useSnapshot } from "../lib/queries";
import { useUiStore } from "../store/ui";
import type { Snapshot } from "../lib/types";

/**
 * Rendered as a sibling of the component under test, sharing its
 * QueryClient. Before `get_snapshot` resolves, `SnapshotStatus` has no
 * snapshot to judge and passes `children` straight through -- so an
 * assertion that children are visible passes vacuously on the very first
 * render, whatever the branch under test would do with the data. Waiting
 * for this probe's "snapshot loaded" is what makes such an assertion be
 * about the loaded snapshot.
 */
function SnapshotProbe() {
  const { data } = useSnapshot();
  return <p>{data ? "snapshot loaded" : "snapshot pending"}</p>;
}

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
  it("shows the no-sources empty state and hides children when detect is Missing", async () => {
    // `Missing` now means all seven sources found nothing, not that Homebrew
    // alone is absent.
    vi.mocked(invoke).mockResolvedValue(baseSnapshot({ detect: "Missing" }));

    renderWithProviders(
      <SnapshotStatus>
        <p>installed list</p>
      </SnapshotStatus>,
    );

    expect(await screen.findByText("Nothing for Canager to manage yet")).toBeInTheDocument();
    expect(
      screen.getByText(
        "Canager works with Homebrew, npm, pipx, uv, pip, Cargo and Ollama. None of them are set up on this Mac yet — Homebrew is the easiest place to start.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByText("installed list")).not.toBeInTheDocument();
  });

  it("shows loading, not the no-sources state, before the first refresh has completed", async () => {
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
    expect(screen.queryByText("Nothing for Canager to manage yet")).not.toBeInTheDocument();
  });

  it("shows the stale-data banner, and no separate incomplete-check one, when a source failed", async () => {
    // There used to be a second banner here for `refreshed_at === null &&
    // errors.length > 0` -- "no check has ever finished, and this one
    // didn't either". `refresh()` now stamps `refreshed_at` whenever it
    // ran (spec §2.4-1), so a snapshot with errors always has one and that
    // branch could never fire again; it and its copy are gone. A refresh
    // that failed for a source says so once, here.
    vi.mocked(invoke).mockResolvedValue(
      baseSnapshot({
        generation: 412,
        refreshed_at: 1700000500,
        stale: true,
        errors: [{ instance_id: "brew:/opt/homebrew", message: "timed out" }],
      }),
    );

    renderWithProviders(
      <SnapshotStatus>
        <p>installed list</p>
      </SnapshotStatus>,
    );

    expect(await screen.findByText("Some data might be out of date")).toBeInTheDocument();
    expect(screen.getByText("installed list")).toBeInTheDocument();
    expect(screen.queryByText("Loading…")).not.toBeInTheDocument();
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

  it("keeps showing the data it has when a later refresh fails on a Mac that has refreshed before", async () => {
    // A Mac that has refreshed 412 times and whose npm is broken. Gating
    // the full-page load-failure surface on the startup error alone let one
    // rejected refresh hide every artifact the other six sources found.
    // `generation > 0` says a refresh has committed data at least once, and
    // data in hand beats a full-page error.
    vi.mocked(invoke).mockResolvedValue(
      baseSnapshot({
        generation: 412,
        refreshed_at: 1700000500,
        stale: true,
        errors: [{ instance_id: "npm:/opt/homebrew/lib", message: "npm ls exited 1" }],
      }),
    );
    useUiStore.setState({ startupRefreshError: "refresh timed out" });

    renderWithProviders(
      <SnapshotStatus>
        <p>installed list</p>
      </SnapshotStatus>,
    );

    expect(await screen.findByText("Some data might be out of date")).toBeInTheDocument();
    expect(screen.getByText("installed list")).toBeInTheDocument();
    expect(screen.queryByText("Couldn't load what's installed")).not.toBeInTheDocument();
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
    // `baseSnapshot` carries a non-null `refreshed_at`, which is exactly what
    // the backend now returns for a root refusal: a refresh that definitively
    // answered "cannot run as root" is a completed refresh and stamps
    // `refreshed_at` (crates/canager-core/src/session/mod.rs). The test below
    // covers the same `detect` with a null `refreshed_at`, so the view does
    // not silently depend on that stamp.
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

  it("shows the root-refusal empty state, not Loading…, when refreshed_at is still null", async () => {
    // Regression: the loading branch matched on `refreshed_at === null &&
    // errors.length === 0`, which a root refusal satisfied, so the app sat on
    // "Loading…" forever — a process's euid never changes, so no later
    // refresh could ever clear it. `detect` must win over the absence of a
    // timestamp.
    vi.mocked(invoke).mockResolvedValue(
      baseSnapshot({ detect: "RefusedAsRoot", refreshed_at: null, errors: [] }),
    );

    renderWithProviders(
      <SnapshotStatus>
        <p>installed list</p>
      </SnapshotStatus>,
    );

    expect(await screen.findByText("Canager can't run as an administrator")).toBeInTheDocument();
    expect(screen.queryByText("Loading…")).not.toBeInTheDocument();
    expect(screen.queryByText("installed list")).not.toBeInTheDocument();
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
    // Not "Once you install something with Homebrew": a Mac with Node and no
    // global packages lands here too.
    expect(
      screen.getByText(
        "Anything you install with Homebrew, npm, pipx, uv, pip, Cargo or Ollama will show up here.",
      ),
    ).toBeInTheDocument();
  });

  it("renders children, not the nothing-installed state, when a source still has a notice to show", async () => {
    // A Mac with Ollama installed but not running and nothing installed
    // anywhere else. InstalledPage renders that instance's group header and
    // its "Open Ollama" notice with no artifacts under it; swallowing the
    // children here would make that notice -- and with it the whole
    // open_ollama_app affordance -- unreachable in the app.
    vi.mocked(invoke).mockResolvedValue(
      baseSnapshot({
        artifacts: [],
        instances: [
          {
            id: "ollama:http://127.0.0.1:11434",
            adapter_id: "ollama",
            exe_path: "/usr/local/bin/ollama",
            prefix: "/usr/local",
            scope: "User",
            version: null,
            unverified_version: null,
            read_only_reason: null,
            status: { unavailable: "NotRunning", notes: [] },
          },
        ],
      }),
    );

    renderWithProviders(
      <>
        <SnapshotProbe />
        <SnapshotStatus>
          <p>installed list</p>
        </SnapshotStatus>
      </>,
    );

    await screen.findByText("snapshot loaded");
    expect(screen.getByText("installed list")).toBeInTheDocument();
    expect(screen.queryByText("Nothing installed yet")).not.toBeInTheDocument();
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
