import { describe, expect, it, vi, beforeEach } from "vitest";
import { fireEvent } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "./test/setup";
import App from "./App";
import type { Settings, Snapshot } from "./lib/types";

const mockInvoke = vi.mocked(invoke);

// A *refreshed* snapshot with one instance and one requested artifact — the
// shape a real launch reaches after useStartupRefresh resolves. `updates: []`
// keeps the Updates page on "Everything is up to date".
const snapshot: Snapshot = {
  generation: 1,
  detect: "Found",
  instances: [
    {
      id: "brew:/opt/homebrew",
      adapter_id: "brew",
      exe_path: "/opt/homebrew/bin/brew",
      prefix: "/opt/homebrew",
      scope: "User",
      version: "7.0.3",
      healthy: true,
      unverified_version: null,
    },
  ],
  artifacts: [
    {
      key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "jq" },
      display_name: "jq",
      version: "1.8.2",
      reason: "Requested",
      description: "Lightweight and flexible command-line JSON processor",
      homepage: "https://jqlang.github.io/jq/",
      size_bytes: null,
      installed_at: 1783762037,
      path: null,
      auto_updates: false,
    },
  ],
  updates: [],
  refreshed_at: 1789700000,
  stale: false,
  errors: [],
};

const defaultSettings: Settings = {
  language: "System",
  show_technical_details: false,
  ignored_updates: [],
};

function mockBackend(snap: Snapshot) {
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "get_snapshot" || cmd === "refresh") return Promise.resolve(snap);
    if (cmd === "get_settings") return Promise.resolve(defaultSettings);
    if (cmd === "list_operations") return Promise.resolve([]);
    return Promise.resolve(undefined);
  });
}

beforeEach(() => {
  mockInvoke.mockReset();
  mockBackend(snapshot);
});

describe("App", () => {
  it("shows the Installed page's filter box by default", async () => {
    const { findByLabelText } = renderWithProviders(<App />);
    await findByLabelText("Filter installed items");
  });

  it("switches the content area when a sidebar link is clicked", async () => {
    const { getByRole, findByLabelText, findByText } = renderWithProviders(<App />);
    await findByLabelText("Filter installed items");

    fireEvent.click(getByRole("button", { name: "Updates" }));

    expect(await findByText("Everything is up to date")).toBeInTheDocument();
  });

  it("keeps Settings reachable when Homebrew is missing", async () => {
    mockBackend({ ...snapshot, detect: "Missing", instances: [], artifacts: [] });
    const { getByRole, findByText, findByRole } = renderWithProviders(<App />);
    await findByText("Homebrew isn't installed yet");

    fireEvent.click(getByRole("button", { name: "Settings" }));

    expect(await findByRole("heading", { name: "Settings" })).toBeInTheDocument();
  });
});
