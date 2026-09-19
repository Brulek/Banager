import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { fireEvent, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { InstalledPage } from "./InstalledPage";
import type { Settings, Snapshot } from "../lib/types";

const mockInvoke = vi.mocked(invoke);

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
    {
      key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "glib" },
      display_name: "glib",
      version: "2.88.3",
      reason: "Dependency",
      description: "Core application library for C",
      homepage: "https://docs.gtk.org/glib/",
      size_bytes: null,
      installed_at: 1788244409,
      path: null,
      auto_updates: false,
    },
  ],
  updates: [
    {
      key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "glib" },
      current: "2.88.3",
      target: "2.90.0",
      channel: "Native",
      checkable: true,
      warnings: [],
    },
  ],
  refreshed_at: 1789700000,
  stale: false,
  errors: [],
};

const settings: Settings = {
  language: "System",
  show_technical_details: false,
  ignored_updates: [],
};

beforeEach(() => {
  mockInvoke.mockReset();
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    width: 800,
    height: 600,
    top: 0,
    left: 0,
    bottom: 600,
    right: 800,
    x: 0,
    y: 0,
    toJSON: () => {},
  } as DOMRect);
  // @tanstack/react-virtual measures its scroll container via offsetWidth /
  // offsetHeight (see virtual-core's `getRect`), not getBoundingClientRect.
  // jsdom hardcodes both offset properties to 0 with no layout engine behind
  // them, so without this the virtualizer sees a zero-size viewport and
  // renders no rows at all, regardless of the getBoundingClientRect stub
  // above. Deviation from the brief's transcribed test, recorded in the task
  // report.
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "get_snapshot") return Promise.resolve(snapshot);
    if (cmd === "get_settings") return Promise.resolve(settings);
    return Promise.resolve(undefined);
  });
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("InstalledPage", () => {
  it("shows the requested artifact and collapses the dependency behind a toggle", async () => {
    const { findByText, queryByText, getByRole } = renderWithProviders(<InstalledPage />);

    await findByText("jq");
    expect(queryByText("glib")).not.toBeInTheDocument();
    expect(
      getByRole("button", { name: "1 component installed by other software" }),
    ).toBeInTheDocument();
  });

  it("reveals the dependency once the toggle is clicked", async () => {
    const { findByText, getByRole } = renderWithProviders(<InstalledPage />);

    await findByText("jq");
    fireEvent.click(getByRole("button", { name: "1 component installed by other software" }));

    await findByText("glib");
  });

  it("filters rows by the query box", async () => {
    const { findByText, queryByText, getByLabelText } = renderWithProviders(<InstalledPage />);

    await findByText("jq");
    fireEvent.change(getByLabelText("Filter installed items"), {
      target: { value: "nonexistent" },
    });

    // Controller's binding note: interactions that assert on DOM changes use
    // fireEvent + findBy/waitFor, never a synchronous assertion right after
    // the interaction. The state path (Zustand setQuery -> synchronous
    // re-render) makes this unlikely to flake today, but wrapping it in
    // waitFor keeps the test robust if the query update ever becomes async
    // (e.g. a debounced filter).
    await waitFor(() => expect(queryByText("jq")).not.toBeInTheDocument());
  });

  it("plans an uninstall when the row's primary button is clicked", async () => {
    const { findByText, getByRole } = renderWithProviders(<InstalledPage />);

    await findByText("jq");
    fireEvent.click(getByRole("button", { name: "Uninstall" }));

    // TanStack Query v5 awaits `onMutate` before calling `mutationFn`, so
    // `invoke` is not called synchronously inside the click.
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith("plan_operation", {
        request: {
          kind: "Uninstall",
          instance_id: "brew:/opt/homebrew",
          artifact_kind: "Formula",
          name: "jq",
        },
      }),
    );
  });
});
