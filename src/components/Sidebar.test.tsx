import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { dragsWindow } from "../test/dragRegion";
import { Sidebar } from "./Sidebar";
import { UpdatesPage } from "../pages/UpdatesPage";
import { queryKeys } from "../lib/queries";
import type {
  ArtifactKey,
  InstalledArtifact,
  ManagerInstance,
  Settings,
  Snapshot,
  UnknownScan,
  UpdateCandidate,
} from "../lib/types";

const mockInvoke = vi.mocked(invoke);

function instance(
  id: string,
  adapterId: string,
  overrides: Partial<ManagerInstance> = {},
): ManagerInstance {
  return {
    id,
    adapter_id: adapterId,
    exe_path: `/opt/${adapterId}/bin/${adapterId}`,
    prefix: `/opt/${adapterId}`,
    scope: "User",
    version: "1.0.0",
    status: { unavailable: null, notes: [] },
    unverified_version: null,
    read_only_reason: null,
    ...overrides,
  };
}

const brew = instance("brew:/opt/homebrew", "brew");
const pip = instance("pip:/usr/bin/python3", "pip", { read_only_reason: "ByDesign" });
const ollama = instance("ollama:http://127.0.0.1:11434", "ollama", {
  status: { unavailable: "NotRunning", notes: [] },
});

function candidate(
  key: ArtifactKey,
  overrides: Partial<UpdateCandidate> = {},
): UpdateCandidate {
  return {
    key,
    current: "1.0.0",
    target: "1.1.0",
    channel: "Native",
    checkable: true,
    warnings: [],
    blocked: null,
    ...overrides,
  };
}

function formula(name: string): ArtifactKey {
  return { instance_id: brew.id, kind: "Formula", name };
}

function artifact(key: ArtifactKey, reason: InstalledArtifact["reason"]): InstalledArtifact {
  return {
    key,
    display_name: key.name,
    version: "1.0.0",
    reason,
    description: null,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
  };
}

// Every kind of row the Updates page lists without offering it, beside the
// two it offers: pinned, could not be checked, from a read-only source, from
// a source that is not answering, and two the user hid -- one never to be
// reminded about, one whose version was skipped.
const snapshot: Snapshot = {
  generation: 3,
  round: 3,
  detect: "Found",
  instances: [brew, pip, ollama],
  artifacts: [
    artifact(formula("glib"), "Requested"),
    artifact(formula("wget"), "Requested"),
    artifact(formula("jq"), "Requested"),
    artifact(formula("pcre2"), "Dependency"),
    artifact({ instance_id: pip.id, kind: "Package", name: "urllib3" }, "Unknown"),
  ],
  updates: [
    candidate(formula("glib")),
    candidate(formula("wget")),
    candidate(formula("jq"), { blocked: "Pinned" }),
    candidate(formula("pcre2"), { checkable: false, target: "1.0.0" }),
    candidate(formula("ffmpeg")),
    candidate(formula("gh"), { target: "2.102.0" }),
    candidate({ instance_id: pip.id, kind: "Package", name: "urllib3" }),
    candidate({ instance_id: ollama.id, kind: "Model", name: "qwen3:8b" }, { channel: "Digest" }),
  ],
  refreshed_at: 1789700000,
  stale: false,
  errors: [],
};

const settings: Settings = {
  language: "System",
  show_technical_details: false,
  ignored_updates: [formula("ffmpeg")],
  skipped_versions: [{ key: formula("gh"), version: "2.102.0" }],
  include_self_updating: false,
  auto_check: false,
  notify_updates: false,
};

const scan: UnknownScan = {
  scanned: [{ path: "~/.local/bin", entries: 4 }],
  entries: [
    {
      path: "~/.local/bin/tool-a",
      kind: "File",
      resolved: "/Users/you/.local/bin/tool-a",
      link_target: null,
      size_bytes: 10,
      modified_at: 1789000000,
      owned_by_me: true,
      app_bundle: null,
    },
    {
      path: "~/.local/bin/tool-b",
      kind: "BrokenSymlink",
      resolved: null,
      link_target: "/Applications/Gone.app/tool-b",
      size_bytes: null,
      modified_at: null,
      owned_by_me: true,
      app_bundle: "Gone",
    },
  ],
  attributed: 2,
  stopped: null,
};

let served: Snapshot;

beforeEach(() => {
  served = snapshot;
  mockInvoke.mockReset();
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "get_snapshot") return Promise.resolve(served);
    if (cmd === "get_settings") return Promise.resolve(settings);
    if (cmd === "scan_unknown") return Promise.resolve(scan);
    return Promise.resolve(undefined);
  });
});

describe("Sidebar", () => {
  it("renders a button for each page, in order, and marks the active one", () => {
    const onSelectPage = vi.fn();
    const { getByRole, getAllByRole } = renderWithProviders(
      <Sidebar page="installed" onSelectPage={onSelectPage} />,
    );

    const overviewButton = getByRole("button", { name: "Overview" });
    const installedButton = getByRole("button", { name: "Installed" });
    const updatesButton = getByRole("button", { name: "Updates" });
    const unknownButton = getByRole("button", { name: "Unknown" });
    const settingsButton = getByRole("button", { name: "Settings" });

    expect(installedButton).toHaveAttribute("aria-current", "page");
    expect(overviewButton).not.toHaveAttribute("aria-current");
    expect(updatesButton).not.toHaveAttribute("aria-current");
    expect(unknownButton).not.toHaveAttribute("aria-current");
    expect(settingsButton).not.toHaveAttribute("aria-current");
    // The Overview first, then the pages about the machine, Updates
    // leading; Settings last, apart from them.
    expect(getAllByRole("button").map((b) => b.textContent)).toEqual([
      "Overview",
      "Updates",
      "Installed",
      "Unknown",
      "Settings",
    ]);
  });

  it("calls onSelectPage with the clicked page", () => {
    const onSelectPage = vi.fn();
    const { getByRole } = renderWithProviders(
      <Sidebar page="installed" onSelectPage={onSelectPage} />,
    );

    getByRole("button", { name: "Updates" }).click();
    expect(onSelectPage).toHaveBeenCalledWith("updates");

    getByRole("button", { name: "Unknown" }).click();
    expect(onSelectPage).toHaveBeenCalledWith("unknown");

    getByRole("button", { name: "Settings" }).click();
    expect(onSelectPage).toHaveBeenCalledWith("settings");

    getByRole("button", { name: "Overview" }).click();
    expect(onSelectPage).toHaveBeenCalledWith("overview");
  });

  it("counts on Updates exactly the updates the Updates page says it can install", async () => {
    // Both on one snapshot and one QueryClient, as in the app. Of the eight
    // updates, only glib and wget have an Update button: the rest are
    // pinned, not checkable, read-only, from a stopped Ollama, never to be
    // reminded about, or skipped.
    const { getByRole, findByText } = renderWithProviders(
      <>
        <Sidebar page="updates" onSelectPage={vi.fn()} />
        <UpdatesPage />
      </>,
    );

    await findByText("2 can be updated", { selector: "p" });
    const updatesButton = getByRole("button", { name: "Updates" });
    expect(within(updatesButton).getByText("2")).toBeInTheDocument();
    expect(updatesButton).toHaveAccessibleDescription("2 can be updated");
  });

  it("leaves an update being installed out of its count, as the Updates page's header does", async () => {
    // glib's update is running: the page says so in words, and counts wget
    // alone among those that can be updated; the sidebar counts the same.
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_snapshot") return Promise.resolve(served);
      if (cmd === "get_settings") return Promise.resolve(settings);
      if (cmd === "list_operations") {
        return Promise.resolve([
          {
            id: 7,
            kind: "Upgrade",
            instance_id: brew.id,
            artifact_kind: "Formula",
            name: "glib",
            status: "Running",
            outcome: null,
            argv_preview: ["/opt/brew/bin/brew", "upgrade", "glib"],
            cancel_policy: "KillThenReconcile",
          },
        ]);
      }
      return Promise.resolve(undefined);
    });
    const { getByRole, findByText } = renderWithProviders(
      <>
        <Sidebar page="updates" onSelectPage={vi.fn()} />
        <UpdatesPage />
      </>,
    );

    await findByText("Updating 1 tool, 1 more can be updated");
    const updatesButton = getByRole("button", { name: "Updates" });
    expect(within(updatesButton).getByText("1")).toBeInTheDocument();
    expect(updatesButton).toHaveAccessibleDescription("1 can be updated");
  });

  it("counts everything installed, components other software brought in included", async () => {
    const { getByRole, findByText } = renderWithProviders(
      <Sidebar page="updates" onSelectPage={vi.fn()} />,
    );

    const installedButton = getByRole("button", { name: "Installed" });
    await findByText("5");
    expect(within(installedButton).getByText("5")).toBeInTheDocument();
    expect(installedButton).toHaveAccessibleDescription("5 installed");
  });

  it("counts Unknown only once a scan has run, and never starts one itself", async () => {
    const { getByRole, findByText, queryClient } = renderWithProviders(
      <Sidebar page="updates" onSelectPage={vi.fn()} />,
    );
    await findByText("5");

    const unknownButton = getByRole("button", { name: "Unknown" });
    expect(unknownButton.textContent).toBe("Unknown");
    expect(unknownButton).not.toHaveAttribute("aria-describedby");
    expect(mockInvoke).not.toHaveBeenCalledWith("scan_unknown");

    // The Unknown page's scan lands in the shared cache.
    act(() => {
      queryClient.setQueryData(queryKeys.unknown, scan);
    });

    expect(await within(unknownButton).findByText("2")).toBeInTheDocument();
    expect(unknownButton).toHaveAccessibleDescription("2 found");
  });

  it("shows no count once it drops to zero", async () => {
    const { getByRole, queryClient } = renderWithProviders(
      <Sidebar page="updates" onSelectPage={vi.fn()} />,
    );
    act(() => {
      queryClient.setQueryData(queryKeys.unknown, scan);
    });
    // Every count showing first, so their going away below is the zero
    // and not data that has not arrived yet.
    await waitFor(() => {
      for (const name of ["Updates", "Installed", "Unknown"]) {
        expect(getByRole("button", { name })).toHaveAttribute("aria-describedby");
      }
    });

    // Everything updated and uninstalled, and a scan that found nothing.
    act(() => {
      queryClient.setQueryData(queryKeys.snapshot, {
        ...snapshot,
        generation: snapshot.generation + 1,
        updates: [],
        artifacts: [],
      });
      queryClient.setQueryData(queryKeys.unknown, { ...scan, entries: [] });
    });

    await waitFor(() => {
      for (const name of ["Overview", "Updates", "Installed", "Unknown", "Settings"]) {
        const button = getByRole("button", { name });
        expect(button.textContent).toBe(name);
        expect(button).not.toHaveAttribute("aria-describedby");
      }
    });
  });

  it("keeps its first row empty for the window's traffic lights, as a drag region, above the app's name", () => {
    const { getByRole, getAllByRole, getByText } = renderWithProviders(
      <Sidebar page="overview" onSelectPage={vi.fn()} />,
    );

    const firstRow = getByRole("navigation", { name: "Navigation" }).firstElementChild as HTMLElement;
    // Nothing under the lights: no text, no control.
    expect(firstRow.childElementCount).toBe(0);
    expect(firstRow.textContent).toBe("");
    // 52px: the 14px lights with 19px above and below them, their centre
    // 26px from the window's top (src/test/windowChrome.test.ts).
    expect(firstRow.className).toContain("h-13");
    expect(firstRow.nextElementSibling).toBe(getByText("Canager"));

    // Pressing it drags the window; nothing else in the sidebar does.
    expect(dragsWindow(firstRow)).toBe(true);
    expect(dragsWindow(getByText("Canager"))).toBe(false);
    for (const button of getAllByRole("button")) {
      expect(dragsWindow(button)).toBe(false);
    }
  });
});
