import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { UpdatesToolbar } from "../test/updatesToolbar";
import { watchDock } from "../test/dock";
import { UpdatesPage } from "./UpdatesPage";
import { OverviewPage } from "./OverviewPage";
import { Sidebar } from "../components/Sidebar";
import { useDockBadge } from "../lib/dockBadge";
import { updatePairOf, useUpdateNotification } from "../lib/updateNotification";
import { artifactKeyId, useUiStore } from "../store/ui";
import i18n from "../i18n";
import type {
  ArtifactKey,
  CommandState,
  InstalledArtifact,
  ManagerInstance,
  OpRequest,
  Settings,
  Snapshot,
  UpdateCandidate,
  UpdatePair,
} from "../lib/types";
import { NO_FACTS } from "../lib/types";

/**
 * U4 (decisions round, 2026-10-06): Codex installed twice, its own
 * installer's copy the one `codex` runs. npm's copy has an update: its row
 * stays on the Updates page with 「终端用另一份」 and a checkbox, but Update
 * All leaves it unticked, and neither the sidebar, the Dock, the update
 * notification, the toolbar's count nor the Overview counts it.
 */

const mockInvoke = vi.mocked(invoke);

function instance(id: string, adapterId: string): ManagerInstance {
  return {
    id,
    adapter_id: adapterId,
    exe_path: `/opt/homebrew/bin/${adapterId}`,
    prefix: "/opt/homebrew",
    scope: "User",
    version: "1.0.0",
    status: { unavailable: null, notes: [] },
    answered_at: null,
    unverified_version: null,
    read_only_reason: null,
  };
}

const BREW = instance("brew:/opt/homebrew", "brew");
const NPM = instance("npm:/opt/homebrew", "npm");
const CODEX = instance("standalone-codex", "standalone-codex");

function installed(key: ArtifactKey, family: string | null, state: CommandState | null): InstalledArtifact {
  return {
    key,
    display_name: key.name,
    version: "1.0.0",
    reason: "Requested",
    description: null,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
    facts: { ...NO_FACTS, family, commands: [{ name: family ?? key.name, state }] },
  };
}

const glibKey: ArtifactKey = { instance_id: BREW.id, kind: "Formula", name: "glib" };
const ownCodexKey: ArtifactKey = { instance_id: CODEX.id, kind: "Binary", name: "codex" };
const npmCodexKey: ArtifactKey = { instance_id: NPM.id, kind: "Package", name: "@openai/codex" };

const artifacts = [
  installed(glibKey, null, "Runs"),
  installed(ownCodexKey, "codex", "Runs"),
  installed(npmCodexKey, "codex", { ShadowedBy: { by: ownCodexKey } }),
];

function update(key: ArtifactKey, target = "1.1.0"): UpdateCandidate {
  return { key, current: "1.0.0", target, channel: "Native", checkable: true, warnings: [], blocked: null };
}

const glib = update(glibKey);
const npmCodex = update(npmCodexKey, "0.160.0");

let updates: UpdateCandidate[];

const settings: Settings = {
  language: "System",
  show_technical_details: false,
  ignored_updates: [],
  skipped_versions: [],
  include_self_updating: false,
  auto_check: true,
  notify_updates: true,
};

beforeEach(() => {
  updates = [glib, npmCodex];
  mockInvoke.mockReset();
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (this: HTMLElement) {
    return this.getAttribute("data-index") === null ? 600 : 56;
  });
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "get_snapshot")
      return Promise.resolve({
        generation: 4,
        round: 4,
        detect: "Found",
        instances: [BREW, NPM, CODEX],
        artifacts,
        updates,
        refreshed_at: 1789700000,
        stale: false,
        errors: [],
      } satisfies Snapshot);
    if (cmd === "get_settings") return Promise.resolve(settings);
    if (cmd === "list_operations") return Promise.resolve([]);
    // The confirmation stays preparing: what it was asked to plan is enough.
    if (cmd === "plan_operation") return new Promise(() => {});
    return Promise.resolve(undefined);
  });
});

afterEach(() => {
  vi.restoreAllMocks();
  void i18n.changeLanguage("en");
});

function plannedNames(): string[] {
  return mockInvoke.mock.calls
    .filter(([cmd]) => cmd === "plan_operation")
    .map(([, args]) => (args as { request: OpRequest }).request.name);
}

function rowOf(name: string): HTMLElement {
  return screen.getByText(name, { selector: "[data-tool-row] p" }).closest("[data-tool-row]") as HTMLElement;
}

function renderUpdates() {
  return renderWithProviders(
    <UpdatesToolbar>
      <UpdatesPage />
    </UpdatesToolbar>,
  );
}

describe("the update of a copy Terminal does not run", () => {
  it("is not in the sidebar's count or on the Dock's badge", async () => {
    const dock = watchDock();
    function Shell() {
      useDockBadge();
      return <Sidebar page="overview" onSelectPage={() => {}} />;
    }
    const { getByRole } = renderWithProviders(<Shell />);
    await waitFor(() => expect(dock.badge()).toBe(1));
    expect(getByRole("button", { name: "Updates" })).toHaveAccessibleDescription("1 can be updated");
  });

  it("is not in what the update notification reports", async () => {
    function Shell() {
      useUpdateNotification();
      return null;
    }
    renderWithProviders(<Shell />);
    const reports = () =>
      mockInvoke.mock.calls
        .filter(([cmd]) => cmd === "report_update_set")
        .map(([, args]) => (args as { updates: UpdatePair[] }).updates);
    await waitFor(() => expect(reports()).toHaveLength(1));
    expect(reports()[0]).toEqual([updatePairOf(glib)]);
  });

  it("keeps its row, its word and an unticked checkbox, and the toolbar counts it apart", async () => {
    renderUpdates();
    await screen.findByText("@openai/codex", { selector: "[data-tool-row] p" });
    const row = rowOf("@openai/codex");
    expect(within(row).getByText("Not used in Terminal")).toBeInTheDocument();
    expect(within(row).getByRole("checkbox", { name: "Select @openai/codex for update" })).not.toBeChecked();
    expect(within(row).getByRole("button", { name: "Update @openai/codex" })).toBeEnabled();
    expect(document.querySelector("[data-toolbar-subtitle]")?.textContent).toBe(
      "1 update available, 1 not used in Terminal",
    );
    await i18n.changeLanguage("zh-CN");
    await waitFor(() =>
      expect(document.querySelector("[data-toolbar-subtitle]")?.textContent).toBe("1个可更新，1个终端用不到"),
    );
  });

  it("is left unticked by Update All, which plans the others", async () => {
    renderUpdates();
    await screen.findByText("@openai/codex", { selector: "[data-tool-row] p" });
    fireEvent.click(screen.getByRole("button", { name: "Update All" }));
    await screen.findByRole("dialog");
    await waitFor(() => expect(plannedNames()).toEqual(["glib"]));
    expect(useUiStore.getState().selectedUpdates).toEqual([artifactKeyId(glibKey)]);
    // Behind the sheet, the list is hidden from the accessibility tree.
    expect(within(rowOf("@openai/codex")).getByRole("checkbox", { hidden: true })).not.toBeChecked();
  });

  it("is ticked by Select All, and updated with the others once ticked", async () => {
    renderUpdates();
    await screen.findByText("@openai/codex", { selector: "[data-tool-row] p" });
    fireEvent.click(screen.getByRole("checkbox", { name: "Select all items that can be updated here" }));
    expect(within(rowOf("@openai/codex")).getByRole("checkbox")).toBeChecked();
    fireEvent.click(screen.getByRole("button", { name: "Update Selected (2)" }));
    await screen.findByRole("dialog");
    await waitFor(() => expect([...plannedNames()].sort()).toEqual(["@openai/codex", "glib"]));
  });

  it("leaves Update All off when it is the only update, while its own row can still be updated", async () => {
    updates = [npmCodex];
    renderUpdates();
    await screen.findByText("@openai/codex", { selector: "[data-tool-row] p" });
    expect(screen.getByRole("button", { name: "Update All" })).toBeDisabled();
    expect(within(rowOf("@openai/codex")).getByRole("button", { name: "Update @openai/codex" })).toBeEnabled();
    expect(document.querySelector("[data-toolbar-subtitle]")?.textContent).toBe("1 not used in Terminal");
  });

  it("is not in the Overview's count, and Review Updates selects the others", async () => {
    const setPage = vi.fn();
    useUiStore.setState({ setPage });
    renderWithProviders(<OverviewPage />);
    expect(await screen.findByRole("heading", { level: 2, name: "1 tool can be updated" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Review Updates" }));
    expect(useUiStore.getState().selectedUpdates).toEqual([artifactKeyId(glibKey)]);
  });
});
