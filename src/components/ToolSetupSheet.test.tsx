import { act, fireEvent, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "../App";
import type { InstalledArtifact, ManagerInstance, Settings, Snapshot, SystemFacts, UnknownScan } from "../lib/types";
import { NO_FACTS } from "../lib/types";
import { fakeMenuBar } from "../test/menuBar";
import { renderWithProviders } from "../test/setup";
import { useUiStore } from "../store/ui";
import { useToolSetupSheet } from "../lib/toolSetupCheck";

const mockInvoke = vi.mocked(invoke);

const BREW = "brew:/opt/homebrew";
const NPM = "npm:/opt/homebrew";
const UV = "uv:/Users/alice/.local/bin/uv";
const CLAUDE = "standalone-claude:/Users/alice/.local/bin/claude";

function instance(id: string, more: Partial<ManagerInstance> = {}): ManagerInstance {
  return {
    id,
    adapter_id: id.split(":")[0],
    exe_path: "/opt/homebrew/bin/x",
    prefix: "/opt/homebrew",
    scope: "User",
    version: "1.0",
    unverified_version: null,
    read_only_reason: null,
    status: { unavailable: null, notes: [] },
    ...more,
  };
}

function artifact(instanceId: string, name: string, facts: Partial<InstalledArtifact["facts"]> = {}): InstalledArtifact {
  return {
    key: { instance_id: instanceId, kind: instanceId.startsWith("brew") ? "Formula" : "Package", name },
    display_name: name,
    version: "1.0",
    reason: "Requested",
    description: null,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
    facts: { ...NO_FACTS, ...facts },
  };
}

/** A Mac with one of everything the sheet can point at: uv not answering, a tool Terminal can't find, two copies of Claude Code, a formula Homebrew disabled. */
const SNAPSHOT: Snapshot = {
  generation: 2,
  round: 2,
  detect: "Found",
  instances: [instance(BREW), instance(NPM), instance(UV, { status: { unavailable: "NotResponding", notes: [] } }), instance(CLAUDE)],
  artifacts: [
    artifact(BREW, "jq", { commands: [{ name: "jq", state: "Runs" }] }),
    artifact(BREW, "youtube-dl", {
      homebrew: {
        deprecated: null,
        disabled: { date: "2024-10-24", reason: "unmaintained", replacement: "yt-dlp" },
        caveats: null,
        other_versions: [],
      },
      commands: [{ name: "youtube-dl", state: "Runs" }],
    }),
    artifact(NPM, "@anthropic-ai/claude-code", {
      family: "claude-code",
      commands: [{ name: "claude", state: { NotOnPath: { dir: "~/.npm-global/bin" } } }],
    }),
    artifact(CLAUDE, "claude", { family: "claude-code", commands: [{ name: "claude", state: "Runs" }] }),
  ],
  updates: [],
  refreshed_at: 1_790_000_000,
  stale: false,
  errors: [],
};

const FACTS: SystemFacts = {
  macos_version: "27.0",
  chip: "Apple M2 Pro",
  arch: "aarch64",
  login_path: true,
  path_dirs: ["/opt/homebrew/bin", "~/.local/bin", "/usr/bin"],
  sources: [],
  path_folders: { read: 3, unread: [] },
};

const SETTINGS: Settings = {
  language: "System",
  show_technical_details: false,
  ignored_updates: [],
  skipped_versions: [],
  include_self_updating: false,
  auto_check: false,
  notify_updates: false,
};

const EMPTY_SCAN: UnknownScan = { scanned: [], entries: [], attributed: 0, stopped: null };

beforeEach(() => {
  mockInvoke.mockReset();
  mockInvoke.mockImplementation(async (cmd: string) => {
    if (cmd === "get_snapshot" || cmd === "refresh") return SNAPSHOT;
    if (cmd === "get_settings") return SETTINGS;
    if (cmd === "get_system_facts") return FACTS;
    if (cmd === "list_operations") return [];
    if (cmd === "scan_unknown") return EMPTY_SCAN;
    return undefined;
  });
  useToolSetupSheet.setState({ open: false });
});

/** The App on the Overview, its first check in, and the sheet opened from Help's Check Tool Setup…. */
async function openedFromHelp() {
  const menu = fakeMenuBar();
  const view = renderWithProviders(<App />);
  await view.findByRole("heading", { level: 1, name: "Overview" });
  await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("get_system_facts"));
  menu.choose("checkToolSetup");
  const dialog = await view.findByRole("dialog", { name: "Tool Setup" });
  // The facts are in: the terminal section says what they say.
  await within(dialog).findByText(/Terminal's login settings were read/);
  return { ...view, dialog };
}

describe("ToolSetupSheet", () => {
  it("opens from Help's Check Tool Setup… over the page that is showing, its sections in order", async () => {
    const { dialog, getByRole } = await openedFromHelp();
    // Under it, out of reach while it is open.
    expect(getByRole("heading", { level: 1, name: "Overview", hidden: true })).toBeInTheDocument();
    expect(within(dialog).getAllByRole("heading", { level: 3 }).map((heading) => heading.textContent)).toEqual([
      "Terminal settings",
      "Sources",
      "Commands",
      "Homebrew",
      "Disk",
    ]);
    // A source that does not answer, named with its state, and the others said once.
    expect(within(dialog).getByText("uv: Not responding")).toBeInTheDocument();
    expect(within(dialog).getByText("The other sources answered normally")).toBeInTheDocument();
    // Done has the focus: the sheet asks nothing.
    expect(within(dialog).getByRole("button", { name: "Done" })).toHaveFocus();
  });

  it.each([
    ["1 tool can't be found in Terminal", "notOnPath"],
    ["1 tool is installed more than once", "twins"],
    ["1 tool was disabled or deprecated by Homebrew", "brewRetired"],
  ] as const)("closes on 查看 of “%s” and opens Installed on every source showing that choice", async (line, show) => {
    const { dialog, findByRole, queryByRole } = await openedFromHelp();
    fireEvent.click(within(dialog).getByRole("button", { name: `Show: ${line}` }));

    expect(await findByRole("heading", { level: 1, name: "Installed" })).toBeInTheDocument();
    await waitFor(() => expect(queryByRole("dialog")).toBeNull());
    expect(useUiStore.getState()).toMatchObject({ page: "installed", installedFilter: null, installedShow: show });
  });

  it("opens a source's own page from its line, and Other Programs from its own", async () => {
    const first = await openedFromHelp();
    fireEvent.click(within(first.dialog).getByRole("button", { name: "Show: uv: Not responding" }));
    expect(await first.findByRole("heading", { level: 1, name: "uv" })).toBeInTheDocument();
    expect(useUiStore.getState()).toMatchObject({ page: "installed", installedFilter: UV, installedShow: "all" });
    await waitFor(() => expect(first.queryByRole("dialog")).toBeNull());

    act(() => useToolSetupSheet.setState({ open: true }));
    const dialog = await first.findByRole("dialog", { name: "Tool Setup" });
    fireEvent.click(
      within(dialog).getByRole("button", {
        name: "Show: Command-line programs from none of these sources are in Other Programs",
      }),
    );
    expect(await first.findByRole("heading", { level: 1, name: "Other Programs" })).toBeInTheDocument();
  });

  it("opens from Settings' Check… and gives the focus back to it on Done, having changed nothing", async () => {
    useUiStore.setState({ page: "settings" });
    const { findByRole, queryByRole } = renderWithProviders(<App />);
    const open = await findByRole("button", { name: "Check Tool Setup…" });
    expect(open).toHaveTextContent(/^Check…$/);
    open.focus();
    fireEvent.click(open);
    const dialog = await findByRole("dialog", { name: "Tool Setup" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Done" }));
    await waitFor(() => expect(queryByRole("dialog")).toBeNull());
    expect(open).toHaveFocus();
    expect(useUiStore.getState().page).toBe("settings");
    // It only reads: nothing planned, submitted, saved or started.
    const asked = [...new Set(mockInvoke.mock.calls.map(([cmd]) => cmd))];
    expect(asked.filter((cmd) => /plan|submit|save|cancel|open_|clear|scan/.test(cmd))).toEqual([]);
  });

  it("closes on Escape, where it was opened", async () => {
    const { dialog, queryByRole, getByRole } = await openedFromHelp();
    fireEvent.keyDown(dialog, { key: "Escape" });
    await waitFor(() => expect(queryByRole("dialog")).toBeNull());
    expect(getByRole("heading", { level: 1, name: "Overview" })).toBeInTheDocument();
    expect(useToolSetupSheet.getState().open).toBe(false);
  });

  it("says the first check has not finished while it runs, over what is known", async () => {
    const startup: Snapshot = { ...SNAPSHOT, generation: 0, round: 0, detect: "Missing", instances: [], artifacts: [], refreshed_at: null };
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_snapshot") return startup;
      // The first check never answers here.
      if (cmd === "refresh") return new Promise(() => {});
      if (cmd === "get_settings") return SETTINGS;
      if (cmd === "get_system_facts") return { ...FACTS, path_folders: null };
      if (cmd === "list_operations") return [];
      return undefined;
    });
    const menu = fakeMenuBar();
    const { findByRole } = renderWithProviders(<App />);
    await findByRole("heading", { level: 1, name: "Overview" });
    menu.choose("checkToolSetup");
    const dialog = await findByRole("dialog", { name: "Tool Setup" });
    expect(within(dialog).getByRole("status")).toHaveTextContent("The check hasn't finished. Here's what's known so far.");
    expect(within(dialog).getByText("Checking…", { selector: "[data-setup-line] p" })).toBeInTheDocument();
  });
});
