import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { UpdatesToolbar } from "../test/updatesToolbar";
import { WithToolbarSlot } from "../test/toolbarSlot";
import { UpdatesPage } from "./UpdatesPage";
import { InstalledPage } from "./InstalledPage";
import { useUiStore } from "../store/ui";
import i18n from "../i18n";
import type { ArtifactKey, InstalledArtifact, OpRequest, Settings, Snapshot, UpdateCandidate } from "../lib/types";
import { NO_FACTS } from "../lib/types";

/**
 * The 「显示」 popup's 「AI工具」 on the Installed and Updates pages: the
 * rows Rust tagged with a family (`facts.family`), and on the Updates page
 * Select All and Update All acting on those rows alone.
 */

const mockInvoke = vi.mocked(invoke);

const BREW = "brew:/opt/homebrew";
const NPM = "npm:/opt/homebrew";
const CLAUDE = "standalone-claude";

function instance(adapterId: string, id: string): Snapshot["instances"][number] {
  return {
    id,
    adapter_id: adapterId,
    exe_path: `/opt/homebrew/bin/${adapterId}`,
    prefix: "/opt/homebrew",
    scope: "User",
    version: "1.0.0",
    status: { unavailable: null, notes: [] },
    unverified_version: null,
    read_only_reason: null,
  };
}

function artifact(key: ArtifactKey, family: string | null, displayName = key.name): InstalledArtifact {
  return {
    key,
    display_name: displayName,
    version: "1.0.0",
    reason: "Requested",
    description: null,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
    facts: family === null ? NO_FACTS : { family },
  };
}

function candidate(key: ArtifactKey): UpdateCandidate {
  return { key, current: "1.0.0", target: "1.1.0", channel: "Native", checkable: true, warnings: [], blocked: null };
}

const glib: ArtifactKey = { instance_id: BREW, kind: "Formula", name: "glib" };
const wget: ArtifactKey = { instance_id: BREW, kind: "Formula", name: "wget" };
const ollama: ArtifactKey = { instance_id: BREW, kind: "Formula", name: "ollama" };
const codex: ArtifactKey = { instance_id: NPM, kind: "Package", name: "@openai/codex" };
const claude: ArtifactKey = { instance_id: CLAUDE, kind: "Binary", name: "claude" };

let instances: Snapshot["instances"];
let artifacts: InstalledArtifact[];
let updates: UpdateCandidate[];

const settings: Settings = {
  language: "System",
  show_technical_details: false,
  ignored_updates: [],
  skipped_versions: [],
  include_self_updating: false,
  auto_check: false,
  notify_updates: false,
};

function planned(): string[] {
  return mockInvoke.mock.calls
    .filter(([name]) => name === "plan_operation")
    .map(([, args]) => (args as { request: OpRequest }).request.name);
}

beforeEach(() => {
  instances = [instance("brew", BREW), instance("npm", NPM), instance("standalone-claude", CLAUDE)];
  artifacts = [
    artifact(glib, null),
    artifact(wget, null),
    artifact(ollama, "ollama"),
    artifact(codex, "codex"),
    artifact(claude, "claude-code", "Claude Code"),
  ];
  updates = [candidate(glib), candidate(wget), candidate(codex), candidate(claude)];
  mockInvoke.mockReset();
  // The virtualizer measures through offsetWidth / offsetHeight, which
  // jsdom answers 0 for: the same stubs as the pages' own tests.
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (this: HTMLElement) {
    return this.getAttribute("data-index") === null ? 600 : 56;
  });
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
  let nextPlan = 1;
  mockInvoke.mockImplementation((cmd: string, args?: unknown) => {
    if (cmd === "get_snapshot")
      return Promise.resolve({
        generation: 1,
        round: 1,
        detect: "Found",
        instances,
        artifacts,
        updates,
        refreshed_at: 1789700000,
        stale: false,
        errors: [],
      } satisfies Snapshot);
    if (cmd === "get_settings") return Promise.resolve(settings);
    if (cmd === "list_operations") return Promise.resolve([]);
    if (cmd === "plan_operation") {
      const request = (args as { request: OpRequest }).request;
      const id = String(nextPlan++);
      return Promise.resolve({
        id,
        plan: {
          request,
          action: { Command: { program: "/opt/homebrew/bin/brew", args: ["upgrade", request.name], env: [] } },
          needs_password: false,
          locks: [request.instance_id],
          cancel_policy: "KillThenReconcile",
          warnings: [],
          affected: [],
          timeout_secs: 1800,
        },
        issued_at: 1758000000,
      });
    }
    return Promise.resolve(undefined);
  });
});

afterEach(() => {
  vi.restoreAllMocks();
  void i18n.changeLanguage("en");
});

function rowNames(): string[] {
  return [...document.querySelectorAll("[data-tool-row]")].map((row) => row.querySelector("p")?.textContent ?? "");
}

function showAiTools() {
  fireEvent.change(screen.getByRole("combobox", { name: "Show" }), { target: { value: "ai" } });
}

describe("the Show popup", () => {
  it("is named for a screen reader and offers every tool or the AI tools, every tool first", async () => {
    renderWithProviders(
      <WithToolbarSlot>
        <InstalledPage />
      </WithToolbarSlot>,
    );
    const popup = await screen.findByRole("combobox", { name: "Show" });
    expect([...popup.querySelectorAll("option")].map((option) => [option.value, option.textContent])).toEqual([
      ["all", "All Tools"],
      ["ai", "AI Tools"],
    ]);
    expect(popup).toHaveValue("all");
    expect(useUiStore.getState().installedShow).toBe("all");
  });
});

describe("the Installed page with AI Tools shown", () => {
  function renderInstalled() {
    return renderWithProviders(
      <WithToolbarSlot>
        <InstalledPage />
      </WithToolbarSlot>,
    );
  }

  it("lists only the tools tagged with a family, from every source, and keeps the choice in the store", async () => {
    renderInstalled();
    await screen.findByText("glib", { selector: "[data-tool-row] p" });
    expect(rowNames()).toEqual(["@openai/codex", "Claude Code", "glib", "ollama", "wget"]);

    showAiTools();
    await waitFor(() => expect(rowNames()).toEqual(["@openai/codex", "Claude Code", "ollama"]));
    expect(useUiStore.getState().installedShow).toBe("ai");
    expect(useUiStore.getState().updatesShow).toBe("all");
  });

  it("searches within the AI tools", async () => {
    renderInstalled();
    await screen.findByText("glib", { selector: "[data-tool-row] p" });
    showAiTools();
    // "l" would find glib too, which is not one.
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "l" } });
    await waitFor(() => expect(rowNames()).toEqual(["Claude Code", "ollama"]));
  });

  it("says no common AI coding tools were found on this Mac when none is installed, in either language", async () => {
    artifacts = artifacts.map((a) => ({ ...a, facts: NO_FACTS }));
    renderInstalled();
    await screen.findByText("glib", { selector: "[data-tool-row] p" });
    showAiTools();
    expect(await screen.findByText("No common AI coding tools were found on this Mac")).toBeInTheDocument();
    expect(screen.queryByText("Nothing installed")).not.toBeInTheDocument();

    await i18n.changeLanguage("zh-CN");
    expect(await screen.findByText("这台Mac上没有找到常见的AI编程工具")).toBeInTheDocument();
  });

  it("names the source when the page shows one source that has none", async () => {
    useUiStore.getState().openInstalled(BREW);
    useUiStore.getState().setInstalledShow("ai");
    artifacts = artifacts.filter((a) => a.key.name !== "ollama");
    renderInstalled();
    expect(await screen.findByText(/^No common AI coding tools were found in Homebrew/)).toBeInTheDocument();
  });

  it("shows every tool again when a notice's Show asks for a tool, so that tool is in the list", () => {
    useUiStore.getState().setInstalledShow("ai");
    useUiStore.getState().showInstalledTool(glib);
    expect(useUiStore.getState().installedShow).toBe("all");
  });
});

describe("the Updates page with AI Tools shown", () => {
  function renderUpdates() {
    return renderWithProviders(
      <UpdatesToolbar>
        <UpdatesPage />
      </UpdatesToolbar>,
    );
  }

  it("lists only the AI tools' updates, and Update All says how many it will update", async () => {
    renderUpdates();
    await screen.findByText("glib", { selector: "[data-tool-row] p" });
    expect(screen.getByRole("button", { name: "Update All" })).toBeEnabled();

    showAiTools();
    await waitFor(() => expect(rowNames()).toEqual(["@openai/codex", "Claude Code"]));
    expect(screen.getByRole("button", { name: "Update All (2)" })).toBeEnabled();
    expect(useUiStore.getState().updatesShow).toBe("ai");
  });

  it("ticks only the rows in sight with Select All, and leaves a row ticked before out of the count", async () => {
    renderUpdates();
    const glibBox = await screen.findByRole("checkbox", { name: "Select glib for update" });
    fireEvent.click(glibBox);
    expect(screen.getByRole("button", { name: "Update Selected (1)" })).toBeInTheDocument();

    showAiTools();
    // glib is out of sight: nothing in sight is ticked.
    expect(await screen.findByRole("button", { name: "Update All (2)" })).toBeInTheDocument();
    const selectAll = screen.getByRole("checkbox", { name: "Select all items that can be updated here" });
    expect(selectAll).not.toBeChecked();

    fireEvent.click(selectAll);
    expect(selectAll).toBeChecked();
    expect(screen.getByRole("button", { name: "Update Selected (2)" })).toBeInTheDocument();

    // Unticking all unticks the rows in sight, and only them.
    fireEvent.click(selectAll);
    expect(selectAll).not.toBeChecked();
    fireEvent.change(screen.getByRole("combobox", { name: "Show" }), { target: { value: "all" } });
    expect(await screen.findByRole("button", { name: "Update Selected (1)" })).toBeInTheDocument();
    expect(screen.getByRole("checkbox", { name: "Select glib for update" })).toBeChecked();
  });

  it("opens the usual confirmation with the AI tools alone and plans nothing else", async () => {
    renderUpdates();
    await screen.findByText("glib", { selector: "[data-tool-row] p" });
    showAiTools();
    fireEvent.click(await screen.findByRole("button", { name: "Update All (2)" }));

    const dialog = await screen.findByRole("dialog", { name: "Update 2 tools?" });
    await waitFor(() => expect(dialog.querySelectorAll("[data-sheet-tool]")).toHaveLength(2));
    expect([...dialog.querySelectorAll("[data-sheet-name]")].map((name) => name.textContent)).toEqual([
      "@openai/codex",
      "Claude Code",
    ]);
    expect(within(dialog).getByRole("button", { name: "Update" })).toBeInTheDocument();
    expect(planned().sort()).toEqual(["@openai/codex", "claude"]);
  });

  it("says that no AI coding tool has an update here when some are installed and none is listed", async () => {
    updates = [candidate(glib), candidate(wget)];
    renderUpdates();
    await screen.findByText("glib", { selector: "[data-tool-row] p" });
    showAiTools();
    expect(await screen.findByText("No AI coding tool updates here")).toBeInTheDocument();
    expect(rowNames()).toEqual([]);
    expect(screen.getByRole("button", { name: "Update All (0)" })).toBeDisabled();
  });

  it("says no common AI coding tools were found on this Mac when none is installed", async () => {
    artifacts = artifacts.map((a) => ({ ...a, facts: NO_FACTS }));
    renderUpdates();
    await screen.findByText("glib", { selector: "[data-tool-row] p" });
    showAiTools();
    expect(await screen.findByText("No common AI coding tools were found on this Mac")).toBeInTheDocument();
  });
});
