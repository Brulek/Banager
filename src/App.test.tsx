import { afterEach, describe, expect, it, vi, beforeEach } from "vitest";
import { fireEvent, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "./test/setup";
import { fakeMenuBar } from "./test/menuBar";
import { watchDock } from "./test/dock";
import App from "./App";
import type { InvokeArgs } from "@tauri-apps/api/core";
import type { OpRequest, Settings, Snapshot, UnknownScan } from "./lib/types";

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
      status: { unavailable: null, notes: [] },
      unverified_version: null,
      read_only_reason: null,
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
      uninstall_blocked: null,
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
  skipped_versions: [],
  include_self_updating: false,
};

const emptyScan: UnknownScan = {
  scanned: [{ path: "~/.local/bin", entries: 0 }],
  entries: [],
  attributed: 0,
  stopped: null,
};

function mockBackend(snap: Snapshot) {
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "get_snapshot" || cmd === "refresh") return Promise.resolve(snap);
    if (cmd === "get_settings") return Promise.resolve(defaultSettings);
    if (cmd === "list_operations") return Promise.resolve([]);
    if (cmd === "scan_unknown") return Promise.resolve(emptyScan);
    return Promise.resolve(undefined);
  });
}

beforeEach(() => {
  mockInvoke.mockReset();
  mockBackend(snapshot);
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllEnvs();
});

describe("App", () => {
  it("opens on the Overview", async () => {
    const { findByRole, getByRole } = renderWithProviders(<App />);

    expect(
      await findByRole("heading", { level: 2, name: "Everything is up to date" }),
    ).toBeInTheDocument();
    expect(getByRole("heading", { level: 1, name: "Overview" })).toBeInTheDocument();
    expect(getByRole("button", { name: "Overview" })).toHaveAttribute("aria-current", "page");
  });

  it("switches the content area when a sidebar link is clicked", async () => {
    const { getByRole, findByLabelText, findByText, queryByText } = renderWithProviders(<App />);
    await findByText("Everything is up to date");

    fireEvent.click(getByRole("button", { name: "Installed" }));
    await findByLabelText("Search installed items");
    expect(queryByText("Everything is up to date")).not.toBeInTheDocument();

    fireEvent.click(getByRole("button", { name: "Updates" }));

    expect(await findByText("Everything is up to date")).toBeInTheDocument();
  });

  it("opens Installed on one source from an Overview tile, and on everything from the sidebar", async () => {
    // The Installed list is virtualized: the virtualizer needs a viewport
    // and row heights, which jsdom does not lay out.
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (
      this: HTMLElement,
    ) {
      return this.getAttribute("data-index") === null ? 600 : 56;
    });
    vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
    const brew = snapshot.instances[0];
    mockBackend({
      ...snapshot,
      instances: [brew, { ...brew, id: "npm:/opt/homebrew", adapter_id: "npm", exe_path: "/opt/homebrew/bin/npm" }],
      artifacts: [
        ...snapshot.artifacts,
        {
          ...snapshot.artifacts[0],
          key: { instance_id: "npm:/opt/homebrew", kind: "Package", name: "typescript" },
          display_name: "typescript",
        },
      ],
    });
    const { findByRole, getByRole, queryByText, findByText } = renderWithProviders(<App />);

    // The tile's count is what the page then shows.
    fireEvent.click(await findByRole("button", { name: "npm 1 item" }));
    expect(await findByRole("button", { name: "npm 1", pressed: true })).toBeInTheDocument();
    expect(await findByText("typescript", { selector: "[data-tool-row] p" })).toBeInTheDocument();
    expect(queryByText("jq", { selector: "[data-tool-row] p" })).toBeNull();

    // The sidebar's count is of everything installed.
    fireEvent.click(getByRole("button", { name: "Installed" }));
    expect(await findByRole("button", { name: "All 2", pressed: true })).toBeInTheDocument();
    expect(await findByText("jq", { selector: "[data-tool-row] p" })).toBeInTheDocument();
  });

  it("titles every page in one header, with that page's own way to look again beside it", async () => {
    const { getByRole, findByText, getAllByRole, queryAllByRole } = renderWithProviders(<App />);
    await findByText("Everything is up to date");

    // The pages about the sources check them again; the Unknown page
    // scans again, and only that; Settings has nothing to look again at.
    const actions: Array<[string, string[]]> = [
      ["Overview", ["Check again"]],
      ["Updates", ["Check again"]],
      ["Installed", ["Check again"]],
      ["Unknown", ["Scan again"]],
      ["Settings", []],
    ];
    for (const [name, expected] of actions) {
      fireEvent.click(getByRole("button", { name }));
      // One page title, and it is this page's.
      const titles = getAllByRole("heading", { level: 1 });
      expect(titles.map((title) => title.textContent)).toEqual([name]);
      const header = titles[0].closest("header") as HTMLElement;
      expect(within(header).queryAllByRole("button").map((button) => button.textContent)).toEqual(expected);
      // Never a second one stacked under the header.
      expect(queryAllByRole("button", { name: /^(Check|Scan) again$/ })).toHaveLength(expected.length);
    }
  });

  it("says in the Unknown page's header when its scan answered, and never when the sources were checked", async () => {
    const { getByRole, findByText, queryByText } = renderWithProviders(<App />);
    await findByText("Everything is up to date");
    expect(queryByText(/^Checked /)).not.toBeNull();

    fireEvent.click(getByRole("button", { name: "Unknown" }));

    expect(await findByText("Scanned just now")).toBeInTheDocument();
    expect(queryByText(/^Checked /)).toBeNull();
    expect(getByRole("button", { name: "Scan again" })).toBeEnabled();
  });

  it("opens the Updates page from Review updates with every row it can update ticked", async () => {
    // Two updates the Updates page offers, and one it lists without a
    // checkbox (pinned). Rows need a height to be drawn in jsdom.
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (
      this: HTMLElement,
    ) {
      return this.getAttribute("data-index") === null ? 600 : 56;
    });
    vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
    const brew = snapshot.instances[0];
    const update = (name: string, blocked: "Pinned" | null = null) => ({
      key: { instance_id: brew.id, kind: "Formula" as const, name },
      current: "1.0.0",
      target: "1.1.0",
      channel: "Native" as const,
      checkable: true,
      warnings: [],
      blocked,
    });
    mockBackend({
      ...snapshot,
      updates: [update("glib"), update("jq", "Pinned"), update("wget")],
    });
    const { findByRole, getByRole, findByText } = renderWithProviders(<App />);

    fireEvent.click(await findByRole("button", { name: "Review updates" }));

    expect(await findByText("2 updates")).toBeInTheDocument();
    expect(getByRole("button", { name: "Updates" })).toHaveAttribute("aria-current", "page");
    expect(await findByRole("checkbox", { name: "Select glib for update" })).toBeChecked();
    expect(getByRole("checkbox", { name: "Select wget for update" })).toBeChecked();
    expect(getByRole("button", { name: "Update selected (2)" })).toBeEnabled();
  });

  it("hands the focus from an uninstall's confirmation to its log, and back to the row's Uninstall when the log closes", async () => {
    // The log drawer gives the focus back to what had it as it opened.
    // The Installed page opens it only once the confirmation has closed and
    // given the focus back to the row's Uninstall, so that is where the
    // user lands when the log closes -- not at the top of the window.
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (
      this: HTMLElement,
    ) {
      return this.getAttribute("data-index") === null ? 600 : 56;
    });
    vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
    const answer = mockInvoke.getMockImplementation();
    mockInvoke.mockImplementation((cmd: string, args?: InvokeArgs) => {
      if (cmd === "plan_operation") {
        return Promise.resolve({
          id: "1",
          plan: {
            request: (args as { request: OpRequest }).request,
            action: { Command: { program: "/opt/homebrew/bin/brew", args: ["uninstall", "--formula", "jq"], env: [] } },
            needs_password: false,
            locks: ["brew:/opt/homebrew"],
            cancel_policy: "KillThenReconcile",
            warnings: [],
            affected: [],
            timeout_secs: 1800,
          },
          issued_at: 1758000000,
        });
      }
      if (cmd === "submit_operation") return Promise.resolve(7);
      return answer === undefined ? Promise.resolve(undefined) : answer(cmd, args);
    });
    const { getByRole, findByRole, findByText } = renderWithProviders(<App />);
    await findByText("Everything is up to date");
    fireEvent.click(getByRole("button", { name: "Installed" }));

    const row = (await findByText("jq", { selector: "[data-tool-row] p" })).closest("[data-tool-row]");
    const uninstall = within(row as HTMLElement).getByRole("button", { name: "Uninstall" });
    fireEvent.click(uninstall);
    const sheet = await findByRole("dialog", { name: "Uninstall jq?" });
    const confirm = within(sheet).getByRole("button", { name: "Uninstall" });
    await waitFor(() => expect(confirm).toBeEnabled());
    fireEvent.click(confirm);

    const log = await findByRole("dialog", { name: "Operation log" });
    await waitFor(() => expect(document.activeElement).toBe(log));
    fireEvent.keyDown(log, { key: "Escape" });

    await waitFor(() => expect(document.activeElement).toBe(uninstall));
  });

  it("keeps Settings reachable when no source is installed", async () => {
    mockBackend({ ...snapshot, detect: "Missing", instances: [], artifacts: [] });
    const { getByRole, findByText, findByRole } = renderWithProviders(<App />);
    await findByText("Canager found nothing it can manage");

    fireEvent.click(getByRole("button", { name: "Settings" }));

    expect(await findByRole("heading", { name: "Settings" })).toBeInTheDocument();
  });

  it("switches to the Unknown page, which lives outside the snapshot's empty states", async () => {
    // A Mac with no source at all: SnapshotStatus shows "Canager found
    // nothing it can manage" for the Installed and Updates pages. That is
    // exactly where everything on the machine is unknown, so this page
    // must not be behind that gate.
    mockBackend({ ...snapshot, detect: "Missing", instances: [], artifacts: [] });
    const { getByRole, findByText, findByRole } = renderWithProviders(<App />);
    await findByText("Canager found nothing it can manage");

    fireEvent.click(getByRole("button", { name: "Unknown" }));

    // Its title is the page header's; the page adds no second one.
    expect(await findByRole("heading", { level: 1, name: "Unknown" })).toBeInTheDocument();
    expect(await findByText("No programs of unknown origin")).toBeInTheDocument();
  });

  it("shows no browser menu on a right-click in a build, and leaves it to developers in development", async () => {
    // Which right-clicks keep a menu in a build is src/lib/contextMenu.ts's.
    vi.stubEnv("PROD", true);
    const built = renderWithProviders(<App />);
    expect(fireEvent.contextMenu(await built.findByRole("heading", { level: 1, name: "Overview" }))).toBe(false);
    built.unmount();

    vi.unstubAllEnvs();
    const dev = renderWithProviders(<App />);
    expect(fireEvent.contextMenu(await dev.findByRole("heading", { level: 1, name: "Overview" }))).toBe(true);
  });

  it("puts the sidebar's Updates count on the Dock's badge", async () => {
    // Two updates the Updates page offers, and one pinned, which it lists
    // without offering.
    const brew = snapshot.instances[0];
    const update = (name: string, blocked: "Pinned" | null = null) => ({
      key: { instance_id: brew.id, kind: "Formula" as const, name },
      current: "1.0.0",
      target: "1.1.0",
      channel: "Native" as const,
      checkable: true,
      warnings: [],
      blocked,
    });
    mockBackend({ ...snapshot, updates: [update("glib"), update("jq", "Pinned"), update("wget")] });
    const dock = watchDock();
    const { getByRole } = renderWithProviders(<App />);

    await waitFor(() => expect(dock.badge()).toBe(2));
    expect(getByRole("button", { name: "Updates" })).toHaveAccessibleDescription("2 can be updated");
  });
});

describe("the menu bar's items that act in the page", () => {
  /** How many times the page has asked for a refresh. */
  function refreshes(): number {
    return mockInvoke.mock.calls.filter(([cmd]) => cmd === "refresh").length;
  }

  it("are listened for from the start: Settings…, Check Again and Search", async () => {
    const menu = fakeMenuBar();
    const { findByText } = renderWithProviders(<App />);
    await findByText("Everything is up to date");

    expect(menu.listening()).toEqual(["menu://check-again", "menu://search", "menu://settings"]);
  });

  it("open Settings on Settings…, as the sidebar's Settings does", async () => {
    const menu = fakeMenuBar();
    const { findByText, findByRole, getByRole } = renderWithProviders(<App />);
    await findByText("Everything is up to date");

    menu.choose("settings");

    expect(await findByRole("heading", { level: 1, name: "Settings" })).toBeInTheDocument();
    expect(getByRole("button", { name: "Settings" })).toHaveAttribute("aria-current", "page");
  });

  it("run the header's Check again on Check Again, and nothing more while a check runs", async () => {
    // The startup check answers at once; every one after it waits.
    const menu = fakeMenuBar();
    const answer = mockInvoke.getMockImplementation();
    const waiting: Array<(snapshot: Snapshot) => void> = [];
    mockInvoke.mockImplementation((cmd: string, args?: InvokeArgs) => {
      if (cmd === "refresh" && refreshes() > 1) {
        return new Promise((resolve) => waiting.push(resolve as (snapshot: Snapshot) => void));
      }
      return answer === undefined ? Promise.resolve(undefined) : answer(cmd, args);
    });
    const { findByText, getByRole } = renderWithProviders(<App />);
    await findByText("Everything is up to date");
    const checkAgain = getByRole("button", { name: "Check again" });
    await waitFor(() => expect(checkAgain).toBeEnabled());
    expect(refreshes()).toBe(1);

    menu.choose("checkAgain");

    await waitFor(() => expect(refreshes()).toBe(2));
    // The header says so, as for its own button.
    expect(checkAgain).toBeDisabled();
    expect(checkAgain.closest("header")).toHaveTextContent("Checking…");

    // Chosen again while it runs: no second check, now or after it.
    menu.choose("checkAgain");
    menu.choose("checkAgain");
    expect(waiting).toHaveLength(1);
    waiting[0](snapshot);

    await waitFor(() => expect(checkAgain).toBeEnabled());
    expect(refreshes()).toBe(2);
  });

  it("go to the Installed page and focus its search on Search, with the focus in the sidebar", async () => {
    const menu = fakeMenuBar();
    const { findByText, findByLabelText, getByRole } = renderWithProviders(<App />);
    await findByText("Everything is up to date");
    const updates = getByRole("button", { name: "Updates" });
    fireEvent.click(updates);
    updates.focus();
    expect(document.activeElement).toBe(updates);

    menu.choose("search");

    const box = await findByLabelText("Search installed items");
    await waitFor(() => expect(document.activeElement).toBe(box));
    expect(getByRole("heading", { level: 1, name: "Installed" })).toBeInTheDocument();
    expect(getByRole("button", { name: "Installed" })).toHaveAttribute("aria-current", "page");
  });

  it("keep the Installed page's search on Search there, its text selected to be typed over", async () => {
    const menu = fakeMenuBar();
    const { findByText, findByLabelText, getByRole } = renderWithProviders(<App />);
    await findByText("Everything is up to date");
    fireEvent.click(getByRole("button", { name: "Installed" }));
    const box = (await findByLabelText("Search installed items")) as HTMLInputElement;
    fireEvent.change(box, { target: { value: "jq" } });
    getByRole("button", { name: "Installed" }).focus();

    menu.choose("search");

    await waitFor(() => expect(document.activeElement).toBe(box));
    expect(box.value).toBe("jq");
    expect([box.selectionStart, box.selectionEnd]).toEqual([0, 2]);
  });

  it("focus the search once the Installed page has loaded, when Search comes before the first check is done", async () => {
    // The backend's startup snapshot, until the first refresh answers.
    const menu = fakeMenuBar();
    const answer = mockInvoke.getMockImplementation();
    let finishFirstCheck: ((snapshot: Snapshot) => void) | undefined;
    mockInvoke.mockImplementation((cmd: string, args?: InvokeArgs) => {
      if (cmd === "get_snapshot") {
        return Promise.resolve({
          ...snapshot,
          generation: 0,
          detect: "Missing",
          instances: [],
          artifacts: [],
          refreshed_at: null,
        });
      }
      if (cmd === "refresh") {
        return new Promise((resolve) => {
          finishFirstCheck = resolve as (snapshot: Snapshot) => void;
        });
      }
      return answer === undefined ? Promise.resolve(undefined) : answer(cmd, args);
    });
    const { findByRole, findByLabelText, queryByLabelText } = renderWithProviders(<App />);
    await findByRole("heading", { level: 1, name: "Overview" });

    menu.choose("search");

    expect(await findByRole("heading", { level: 1, name: "Installed" })).toBeInTheDocument();
    expect(queryByLabelText("Search installed items")).toBeNull();
    await waitFor(() => expect(finishFirstCheck).toBeDefined());
    finishFirstCheck?.(snapshot);

    const box = await findByLabelText("Search installed items");
    await waitFor(() => expect(document.activeElement).toBe(box));
  });

  it("focus the search once the Installed page has its snapshot, when Search comes before it has", async () => {
    // The page is up, still asking the backend for what it last found.
    const menu = fakeMenuBar();
    const answer = mockInvoke.getMockImplementation();
    const answers = new Map<string, (snapshot: Snapshot) => void>();
    mockInvoke.mockImplementation((cmd: string, args?: InvokeArgs) => {
      if (cmd === "get_snapshot" || cmd === "refresh") {
        return new Promise((resolve) => answers.set(cmd, resolve as (snapshot: Snapshot) => void));
      }
      return answer === undefined ? Promise.resolve(undefined) : answer(cmd, args);
    });
    const { findByRole, findByLabelText, getByRole, queryByLabelText } = renderWithProviders(<App />);
    await findByRole("heading", { level: 1, name: "Overview" });

    menu.choose("search");

    expect(await findByRole("heading", { level: 1, name: "Installed" })).toBeInTheDocument();
    expect(queryByLabelText("Search installed items")).toBeNull();
    await waitFor(() => expect(answers.has("get_snapshot")).toBe(true));
    answers.get("get_snapshot")?.(snapshot);

    const box = await findByLabelText("Search installed items");
    await waitFor(() => expect(document.activeElement).toBe(box));
    // The startup check, which nothing here waited on, finishes too.
    answers.get("refresh")?.(snapshot);
    await waitFor(() => expect(getByRole("button", { name: "Check again" })).toBeEnabled());
  });
});
