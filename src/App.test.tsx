import { afterEach, describe, expect, it, onTestFinished, vi, beforeEach } from "vitest";
import { act, fireEvent, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "./test/setup";
import { fakeMenuBar } from "./test/menuBar";
import { watchDock } from "./test/dock";
import App from "./App";
import { OPEN_UPDATES_EVENT, QUIT_REQUESTED_EVENT } from "./lib/api";
import { useNoBrowserContextMenu } from "./lib/contextMenu";
import { queryKeys } from "./lib/queryKeys";
import { useUiStore } from "./store/ui";
import type { InvokeArgs } from "@tauri-apps/api/core";
import type { OpRequest, OpSummary, Settings, Snapshot, UnknownEntry, UnknownScan } from "./lib/types";
import { NO_FACTS } from "./lib/types";

// The real hook, watched: `App` calls it once each time it draws, and
// nothing else calls it, so its calls count App's draws.
vi.mock("./lib/contextMenu", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./lib/contextMenu")>();
  return { ...actual, useNoBrowserContextMenu: vi.fn(actual.useNoBrowserContextMenu) };
});

const mockInvoke = vi.mocked(invoke);

// A *refreshed* snapshot with one instance and one requested artifact — the
// shape a real launch reaches after useStartupRefresh resolves. `updates: []`
// keeps the Updates page on "Everything is up to date".
const snapshot: Snapshot = {
  generation: 1,
  round: 1,
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
      facts: NO_FACTS,
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
  auto_check: false,
  notify_updates: false,
};

const emptyScan: UnknownScan = {
  scanned: [{ path: "~/.local/bin", entries: 0 }],
  entries: [],
  attributed: 0,
  stopped: null,
};

function mockBackend(snap: Snapshot, settings: Settings = defaultSettings) {
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "get_snapshot" || cmd === "refresh") return Promise.resolve(snap);
    if (cmd === "get_settings") return Promise.resolve(settings);
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

  it("draws the toolbar's hairline once the page has scrolled from its top, and takes it away at the top, without drawing the window again", async () => {
    const { findByText, getByRole, container } = renderWithProviders(<App />);
    await findByText("Everything is up to date");
    const header = getByRole("heading", { level: 1, name: "Overview" }).closest("header") as HTMLElement;
    // The page's own box, under the toolbar: jsdom lays nothing out, so it
    // says how tall its content and its box are.
    const box = header.nextElementSibling as HTMLElement;
    expect(box.className.split(" ")).toContain("overflow-y-auto");
    Object.defineProperty(box, "scrollHeight", { configurable: true, value: 2000 });
    Object.defineProperty(box, "clientHeight", { configurable: true, value: 500 });
    const appDraws = vi.mocked(useNoBrowserContextMenu).mock.calls.length;

    box.scrollTop = 120;
    fireEvent.scroll(box);
    expect(container.querySelector("[data-scroll-edge]")).not.toBeNull();
    box.scrollTop = 0;
    fireEvent.scroll(box);
    expect(container.querySelector("[data-scroll-edge]")).toBeNull();
    // The toolbar drew again; the window -- the page and its list's rows,
    // at the first step of every scroll -- did not.
    expect(vi.mocked(useNoBrowserContextMenu).mock.calls.length).toBe(appDraws);
  });

  it("switches the content area when a sidebar link is clicked", async () => {
    const { getByRole, findByLabelText, findByText, queryByText } = renderWithProviders(<App />);
    await findByText("Everything is up to date");

    fireEvent.click(getByRole("button", { name: "Installed" }));
    await findByLabelText("Search installed tools");
    expect(queryByText("Everything is up to date")).not.toBeInTheDocument();

    fireEvent.click(getByRole("button", { name: "Updates" }));

    expect(await findByText("Everything is up to date")).toBeInTheDocument();
  });

  it("opens Installed on one source from its row under Sources, titled and counted as that source, and on everything from Installed", async () => {
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
    await findByRole("heading", { level: 2, name: "Everything is up to date" });
    const sources = await findByRole("list", { name: "Sources" });
    const subtitle = () => getByRole("heading", { level: 1 }).nextElementSibling?.textContent ?? null;

    // A source's row, as Mail's mailbox: the page on its tools alone,
    // titled with its name and counting its own; that row selected, and
    // Installed's not.
    fireEvent.click(await within(sources).findByRole("button", { name: "npm" }));
    expect(await findByText("typescript", { selector: "[data-tool-row] p" })).toBeInTheDocument();
    expect(queryByText("jq", { selector: "[data-tool-row] p" })).toBeNull();
    expect(getByRole("heading", { level: 1 })).toHaveTextContent(/^npm$/);
    await waitFor(() => expect(subtitle()).toBe("1 tool"));
    expect(within(sources).getByRole("button", { name: "npm" })).toHaveAttribute("aria-current", "page");
    expect(getByRole("button", { name: "Installed" })).not.toHaveAttribute("aria-current");

    // The sidebar's Installed: everything, which its count counts.
    fireEvent.click(getByRole("button", { name: "Installed" }));
    expect(await findByText("jq", { selector: "[data-tool-row] p" })).toBeInTheDocument();
    expect(getByRole("heading", { level: 1 })).toHaveTextContent(/^Installed$/);
    await waitFor(() => expect(subtitle()).toBe("2 tools"));
    expect(getByRole("button", { name: "Installed" })).toHaveAttribute("aria-current", "page");
    expect(within(sources).getByRole("button", { name: "npm" })).not.toHaveAttribute("aria-current");

    // Another page selects its own row, and a source's again opens its tools.
    fireEvent.click(getByRole("button", { name: "Updates" }));
    expect(getByRole("heading", { level: 1 })).toHaveTextContent(/^Updates$/);
    fireEvent.click(within(sources).getByRole("button", { name: "Homebrew" }));
    expect(await findByText("jq", { selector: "[data-tool-row] p" })).toBeInTheDocument();
    expect(getByRole("heading", { level: 1 })).toHaveTextContent(/^Homebrew$/);
    expect(queryByText("typescript", { selector: "[data-tool-row] p" })).toBeNull();
  });

  it("titles a source with nothing installed by its name, with no count, and keeps it", async () => {
    const brew = snapshot.instances[0];
    const npm = { ...brew, id: "npm:/opt/homebrew", adapter_id: "npm", exe_path: "/opt/homebrew/bin/npm" };
    mockBackend({ ...snapshot, instances: [brew, npm] });
    const { findByRole, getByRole, findByText } = renderWithProviders(<App />);
    const sources = await findByRole("list", { name: "Sources" });

    fireEvent.click(await within(sources).findByRole("button", { name: "npm" }));
    expect(await findByText("Nothing installed with npm")).toBeInTheDocument();
    expect(getByRole("heading", { level: 1 })).toHaveTextContent(/^npm$/);
    // No count under it: its status says nothing, out of sight.
    const status = getByRole("heading", { level: 1 }).nextElementSibling;
    expect(status).toHaveAttribute("role", "status");
    expect(status).toBeEmptyDOMElement();
    expect(status).toHaveClass("sr-only");
    expect(useUiStore.getState().installedFilter).toBe(npm.id);
  });

  it("titles a Homebrew by which one it is where the Mac has two: Apple silicon's, or an Intel Mac's", async () => {
    const brew = snapshot.instances[0];
    const intel = { ...brew, id: "brew:/usr/local", exe_path: "/usr/local/bin/brew", prefix: "/usr/local" };
    mockBackend({ ...snapshot, instances: [brew, intel] });
    const { findByRole, getByRole, findByText } = renderWithProviders(<App />);
    const sources = await findByRole("list", { name: "Sources" });

    fireEvent.click(await within(sources).findByRole("button", { name: "Homebrew (Intel)" }));
    expect(await findByText("Nothing installed with Homebrew (Intel)")).toBeInTheDocument();
    expect(getByRole("heading", { level: 1 })).toHaveTextContent(/^Homebrew \(Intel\)$/);
    fireEvent.click(within(sources).getByRole("button", { name: "Homebrew (Apple silicon)" }));
    await waitFor(() => expect(getByRole("heading", { level: 1 })).toHaveTextContent(/^Homebrew \(Apple silicon\)$/));
  });

  it("titles every page in one header, with that page's own way to look again beside it", async () => {
    const { getByRole, findByText, getAllByRole, queryAllByRole } = renderWithProviders(<App />);
    await findByText("Everything is up to date");

    // The pages about the sources check them again; Other Programs scans
    // again, and only that; Settings has nothing to look again at.
    const actions: Array<[string, string[]]> = [
      ["Overview", ["Check Again"]],
      ["Updates", ["Check Again"]],
      ["Installed", ["Check Again"]],
      ["Other Programs", ["Scan Again"]],
      ["Settings", []],
    ];
    for (const [name, expected] of actions) {
      fireEvent.click(getByRole("button", { name }));
      // One page title, and it is this page's.
      const titles = getAllByRole("heading", { level: 1 });
      expect(titles.map((title) => title.textContent)).toEqual([name]);
      const header = titles[0].closest("header") as HTMLElement;
      // An icon button each: its name is its label.
      expect(within(header).queryAllByRole("button").map((button) => button.getAttribute("aria-label"))).toEqual(
        expected,
      );
      // Never a second one stacked under the header -- but for the
      // Overview's status row and the Updates page's empty list, whose
      // one button it is when there is nothing else to do there
      // (everything is up to date here), as macOS's empty states offer one.
      const again = queryAllByRole("button", { name: /^(Check|Scan) Again$/ });
      const inStatusRow = again.filter((button) => button.closest("[data-status], [data-empty-state]") !== null);
      expect(inStatusRow).toHaveLength(name === "Overview" || name === "Updates" ? 1 : 0);
      expect(again.length - inStatusRow.length).toBe(expected.length);
    }
  });

  it("keeps each page's way to look again at the toolbar's right end, after the page's own controls", async () => {
    const brew = snapshot.instances[0];
    mockBackend({
      ...snapshot,
      updates: [
        {
          key: { instance_id: brew.id, kind: "Formula", name: "jq" },
          current: "1.8.1",
          target: "1.8.2",
          channel: "Native",
          checkable: true,
          warnings: [],
          blocked: null,
        },
      ],
    });
    const { getByRole, findByRole } = renderWithProviders(<App />);
    await findByRole("button", { name: "Review Updates" });

    // Every control in the toolbar, in order, by its name.
    const controls = () =>
      [...getByRole("banner").querySelectorAll<HTMLElement>("button, input, select")].map(
        (control) => control.getAttribute("aria-label") ?? control.textContent,
      );
    // The page's own controls first -- what it shows, Update All, the
    // sort and the search -- and the ⟳ last: a button before it whose
    // words change grows to its left, and the ⟳ never moves.
    const pages: Array<[string, string[], string]> = [
      ["Overview", [], "Check Again"],
      ["Updates", ["Show", "Update All"], "Check Again"],
      ["Installed", ["Show", "Sort Order", "Search installed tools"], "Check Again"],
      ["Other Programs", [], "Scan Again"],
    ];
    for (const [page, own, again] of pages) {
      fireEvent.click(getByRole("button", { name: page }));
      await waitFor(() => expect(controls()).toEqual([...own, again]));
    }
  });

  it("says in Other Programs' toolbar when its scan answered, and never when the sources were checked", async () => {
    const { getByRole, findByText } = renderWithProviders(<App />);
    await findByText("Everything is up to date");
    const toolbar = getByRole("heading", { level: 1 }).closest("header") as HTMLElement;
    const checkAgain = within(toolbar).getByRole("button", { name: "Check Again" });
    await waitFor(() => expect(checkAgain.getAttribute("title")).toMatch(/^Check Again \(⌘R\) · Checked /));

    fireEvent.click(getByRole("button", { name: "Other Programs" }));

    const scanAgain = getByRole("button", { name: "Scan Again" });
    await waitFor(() => expect(scanAgain).toHaveAttribute("title", "Scan Again · Scanned just now"));
    expect(scanAgain).not.toHaveAttribute("aria-disabled");
  });

  it("gives each page's toolbar its subtitle: what the page lists, or none on the Overview and Settings", async () => {
    const brew = snapshot.instances[0];
    const update = (name: string) => ({
      key: { instance_id: brew.id, kind: "Formula" as const, name },
      current: "1.0.0",
      target: "1.1.0",
      channel: "Native" as const,
      checkable: true,
      warnings: [],
      blocked: null,
    });
    const artifact = snapshot.artifacts[0];
    mockBackend({
      ...snapshot,
      artifacts: [artifact, { ...artifact, key: { ...artifact.key, name: "wget" }, display_name: "wget" }],
      updates: [update("jq"), update("wget")],
    });
    // Three programs no source accounts for.
    const program = (name: string): UnknownEntry => ({
      path: `~/.local/bin/${name}`,
      kind: "File",
      resolved: `/Users/you/.local/bin/${name}`,
      link_target: null,
      size_bytes: 1024,
      modified_at: 1789700000,
      owned_by_me: true,
      app_bundle: null,
    });
    const answer = mockInvoke.getMockImplementation() as (cmd: string, args?: InvokeArgs) => Promise<unknown>;
    mockInvoke.mockImplementation((cmd: string, args?: InvokeArgs) =>
      cmd === "scan_unknown"
        ? Promise.resolve({ ...emptyScan, entries: ["a", "b", "c"].map(program) })
        : answer(cmd, args),
    );
    const { getByRole, findByRole } = renderWithProviders(<App />);
    await findByRole("button", { name: "Review Updates" });

    // The status under the title, which says nothing, out of sight, on a
    // page with no subtitle.
    const subtitleOf = () => {
      const status = getByRole("heading", { level: 1 }).nextElementSibling;
      expect(status).toHaveAttribute("role", "status");
      const text = status?.textContent ?? "";
      if (text === "") expect(status).toHaveClass("sr-only");
      return text === "" ? null : text;
    };
    const expected: Array<[string, string | null]> = [
      ["Overview", null],
      ["Updates", "2 updates available"],
      ["Installed", "2 tools"],
      ["Other Programs", "3 programs"],
      ["Settings", null],
    ];
    for (const [name, subtitle] of expected) {
      fireEvent.click(getByRole("button", { name }));
      await waitFor(() => expect(subtitleOf()).toBe(subtitle));
    }
  });

  it("says Other Programs' scan under way, then what it found, through the toolbar's one status node", async () => {
    const answer = mockInvoke.getMockImplementation() as (cmd: string, args?: InvokeArgs) => Promise<unknown>;
    let release: (() => void) | undefined;
    const program: UnknownEntry = {
      path: "~/.local/bin/a",
      kind: "File",
      resolved: "/Users/you/.local/bin/a",
      link_target: null,
      size_bytes: 1024,
      modified_at: 1789700000,
      owned_by_me: true,
      app_bundle: null,
    };
    mockInvoke.mockImplementation((cmd: string, args?: InvokeArgs) =>
      cmd === "scan_unknown"
        ? new Promise((resolve) => {
            release = () => resolve({ ...emptyScan, entries: [program] });
          })
        : answer(cmd, args),
    );
    const { getByRole, findByText } = renderWithProviders(<App />);
    await findByText("Everything is up to date");
    const status = getByRole("heading", { level: 1 }).nextElementSibling as HTMLElement;
    expect(status).toHaveAttribute("role", "status");

    fireEvent.click(getByRole("button", { name: "Other Programs" }));
    await waitFor(() => expect(status).toHaveTextContent("Scanning…"));
    expect(getByRole("heading", { level: 1 }).nextElementSibling).toBe(status);
    await waitFor(() => expect(release).toBeDefined());
    await act(async () => release?.());
    await waitFor(() => expect(status).toHaveTextContent("1 program"));
    expect(getByRole("heading", { level: 1 }).nextElementSibling).toBe(status);
  });

  it("says in the toolbar's subtitle that a check is under way, and as an alert that it failed", async () => {
    const answer = mockInvoke.getMockImplementation() as (cmd: string, args?: InvokeArgs) => Promise<unknown>;
    let finish: (() => void) | undefined;
    const { getByRole, findByText, findByRole } = renderWithProviders(<App />);
    await findByText("Everything is up to date");
    fireEvent.click(getByRole("button", { name: "Installed" }));
    await waitFor(() => expect(getByRole("heading", { level: 1 }).nextElementSibling).toHaveTextContent("1 tool"));
    // One status node, which a screen reader hears change: 「正在检查…」 is
    // said in it, not in a node of its own.
    const status = getByRole("heading", { level: 1 }).nextElementSibling as HTMLElement;
    expect(status).toHaveAttribute("role", "status");

    mockInvoke.mockImplementation((cmd: string, args?: InvokeArgs) =>
      cmd === "refresh"
        ? new Promise((_, reject) => {
            finish = () => reject("the session is gone");
          })
        : answer(cmd, args),
    );
    fireEvent.click(getByRole("button", { name: "Check Again" }));
    expect(await within(getByRole("banner")).findByText("Checking…")).toBe(status);

    await waitFor(() => expect(finish).toBeDefined());
    finish?.();
    const alert = await findByRole("alert");
    expect(alert).toHaveTextContent("Couldn't check");
    expect(alert.closest("header")).not.toBeNull();
    expect(alert.className).toContain("text-danger-text");
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
    const { findByRole, getByRole } = renderWithProviders(<App />);

    fireEvent.click(await findByRole("button", { name: "Review Updates" }));

    // The toolbar's subtitle says how many, and the page does not say it
    // again over its list: once a screen.
    expect(await within(getByRole("banner")).findByText("2 updates available")).toBeInTheDocument();
    expect(getByRole("button", { name: "Updates" })).toHaveAttribute("aria-current", "page");
    expect(await findByRole("checkbox", { name: "Select glib for update" })).toBeChecked();
    expect(within(getByRole("main")).getAllByText("2 updates available")).toHaveLength(1);
    expect(getByRole("checkbox", { name: "Select wget for update" })).toBeChecked();
    // The page's one action, in the toolbar, counting the ticked rows.
    expect(within(getByRole("banner")).getByRole("button", { name: "Update Selected (2)" })).toBeEnabled();
  });

  it("opens the Installed page on a launcher left without its program from the Overview's Show, the tool selected", async () => {
    // Rows need a height to be drawn in jsdom.
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (this: HTMLElement) {
      return this.getAttribute("data-index") === null ? 600 : 56;
    });
    vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
    const grok = {
      ...snapshot.instances[0],
      id: "standalone-grok",
      adapter_id: "standalone-grok",
      exe_path: "/Users/someone/.grok/bin/grok",
      prefix: "/Users/someone/.grok",
      version: null,
      status: { unavailable: null, notes: ["LauncherOnly" as const] },
    };
    mockBackend({
      ...snapshot,
      instances: [...snapshot.instances, grok],
      artifacts: [
        ...snapshot.artifacts,
        {
          ...snapshot.artifacts[0],
          key: { instance_id: grok.id, kind: "Binary", name: "grok" },
          display_name: "Grok Build",
          version: "",
          description: null,
          homepage: null,
        },
      ],
    });
    const { findByRole, getByRole } = renderWithProviders(<App />);

    const problems = await findByRole("list", { name: "Needs attention" });
    const problem = within(problems).getByText("Grok Build's program files are missing").closest("li") as HTMLElement;
    // What the user can do about it in the app is its Uninstall…: Show
    // leads there. Checking again is the toolbar's ⟳.
    expect(within(problem).queryByRole("button", { name: "Check Again" })).toBeNull();
    fireEvent.click(within(problem).getByRole("button", { name: "Show" }));

    expect(getByRole("heading", { level: 1 })).toHaveTextContent("Installed");
    expect(getByRole("button", { name: "Installed" })).toHaveAttribute("aria-current", "page");
    const inspector = await findByRole("complementary", { name: "Grok Build" });
    expect(within(inspector).getByRole("button", { name: "Uninstall…" })).toBeInTheDocument();
    expect(getByRole("button", { name: "Details: Grok Build" })).toHaveAttribute("aria-pressed", "true");
    expect(within(getByRole("banner")).getByRole("button", { name: "Check Again" })).toBeInTheDocument();
  });

  it("hands the focus from an uninstall's confirmation to its log, and back to the row when the log closes", async () => {
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
    const uninstall = within(row as HTMLElement).getByRole("button", { name: "Uninstall jq…" });
    fireEvent.click(uninstall);
    const sheet = await findByRole("dialog", { name: "Uninstall “jq”?" });
    const confirm = within(sheet).getByRole("button", { name: "Uninstall" });
    await waitFor(() => expect(confirm).toBeEnabled());
    fireEvent.click(confirm);

    const log = await findByRole("dialog", { name: "Operation log" });
    await waitFor(() => expect(document.activeElement).toBe(log));
    fireEvent.keyDown(log, { key: "Escape" });

    // Its row, not its Uninstall, which says 「正在卸载…」 now, off, and would
    // hand the focus on to the window's body.
    await waitFor(() => expect(document.activeElement).toBe(row));
  });

  it("opens Settings at the hidden updates from the Overview's count of them, the focus on their first title", async () => {
    // jq's update, which the user asked never to be reminded about: the
    // Updates page lists nothing, and Settings lists it.
    const jq = snapshot.artifacts[0].key;
    mockBackend(
      {
        ...snapshot,
        updates: [
          { key: jq, current: "1.8.2", target: "1.8.3", channel: "Native", checkable: true, warnings: [], blocked: null },
        ],
      },
      { ...defaultSettings, ignored_updates: [jq] },
    );
    // jsdom lays nothing out and has no `scrollIntoView`: what is scrolled
    // into view is noted.
    const scrolled: Element[] = [];
    Element.prototype.scrollIntoView = function (this: Element) {
      scrolled.push(this);
    };
    try {
      const { findByRole, getByRole } = renderWithProviders(<App />);
      const headline = await findByRole("heading", { level: 2, name: "Nothing to update" });

      fireEvent.click(within(headline.nextElementSibling as HTMLElement).getByRole("button", { name: "1 hidden" }));

      expect(await findByRole("heading", { level: 1, name: "Settings" })).toBeInTheDocument();
      expect(getByRole("button", { name: "Settings" })).toHaveAttribute("aria-current", "page");
      // Its two groups of hidden updates, the focus on the first one's title.
      const skipped = getByRole("region", { name: "Skipped versions" });
      const never = getByRole("region", { name: "Tools with reminders off" });
      await waitFor(() => expect(getByRole("heading", { level: 2, name: "Skipped versions" })).toHaveFocus());
      // On the page, those groups alone; in the sidebar, the row selected
      // (`Sidebar`), last of all Settings'.
      const sidebar = getByRole("navigation");
      expect(scrolled.filter((element) => !sidebar.contains(element))).toEqual([skipped.parentElement]);
      const rows = scrolled.filter((element) => sidebar.contains(element));
      expect(rows[rows.length - 1]).toBe(getByRole("button", { name: "Settings" }));
      expect(skipped.parentElement?.contains(never)).toBe(true);
      expect(within(never).getByRole("button", { name: "Remind me again about jq" })).toBeInTheDocument();
    } finally {
      delete (Element.prototype as Partial<Element>).scrollIntoView;
    }
  });

  it("keeps Settings reachable when no source is installed", async () => {
    mockBackend({ ...snapshot, detect: "Missing", instances: [], artifacts: [] });
    const { getByRole, findByText, findByRole } = renderWithProviders(<App />);
    await findByText("No tools to manage");

    fireEvent.click(getByRole("button", { name: "Settings" }));

    expect(await findByRole("heading", { name: "Settings" })).toBeInTheDocument();
  });

  it("switches to Other Programs, under 「来源」 with no source, and outside the snapshot's empty states", async () => {
    // A Mac with no source at all: SnapshotStatus shows "Banager found
    // nothing it can manage" for the Installed and Updates pages. That is
    // exactly where no source accounts for anything on the machine, so this
    // page must not be behind that gate -- nor its row behind the sources'.
    mockBackend({ ...snapshot, detect: "Missing", instances: [], artifacts: [] });
    const { getByRole, findByText, findByRole } = renderWithProviders(<App />);
    await findByText("No tools to manage");
    const sources = getByRole("list", { name: "Sources" });

    fireEvent.click(within(sources).getByRole("button", { name: "Other Programs" }));

    // Its title is the page header's; the page adds no second one.
    expect(await findByRole("heading", { level: 1, name: "Other Programs" })).toBeInTheDocument();
    expect(await findByText("No other programs")).toBeInTheDocument();
    expect(within(sources).getByRole("button", { name: "Other Programs" })).toHaveAttribute("aria-current", "page");
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

  it("keeps the Dock's badge up to date as an update starts, without drawing the window again for it", async () => {
    const brew = snapshot.instances[0];
    const update = (name: string) => ({
      key: { instance_id: brew.id, kind: "Formula" as const, name },
      current: "1.0.0",
      target: "1.1.0",
      channel: "Native" as const,
      checkable: true,
      warnings: [],
      blocked: null,
    });
    mockBackend({ ...snapshot, updates: [update("glib"), update("wget")] });
    const dock = watchDock();
    const { queryClient, getByRole } = renderWithProviders(<App />);
    await waitFor(() => expect(dock.badge()).toBe(2));
    const appDraws = vi.mocked(useNoBrowserContextMenu).mock.calls.length;

    // glib's update is queued, as an operation's event tells the page
    // (`useOperationEvents`): its row is taken, and one update is left to
    // start. Update all changes the operations hundreds of times over.
    const queued: OpSummary = {
      id: 1,
      kind: "Upgrade",
      instance_id: brew.id,
      artifact_kind: "Formula",
      name: "glib",
      status: "Queued",
      outcome: null,
      argv_preview: [],
      cancel_policy: "KillThenReconcile",
    };
    const answer = mockInvoke.getMockImplementation();
    mockInvoke.mockImplementation((cmd: string, args?: InvokeArgs) =>
      cmd === "list_operations" ? Promise.resolve([queued]) : answer!(cmd, args),
    );
    await queryClient.invalidateQueries({ queryKey: queryKeys.operations });

    await waitFor(() => expect(dock.badge()).toBe(1));
    expect(getByRole("button", { name: "Updates" })).toHaveAccessibleDescription("1 can be updated");
    // The badge's and the notification's hooks drew again; the window --
    // the page, its rows, the header -- did not, for them.
    expect(vi.mocked(useNoBrowserContextMenu).mock.calls.length).toBe(appDraws);
  });
});

describe("the update notification", () => {
  it("is told, after the first check, which updates Update all would take, with the check's round", async () => {
    const { findByText } = renderWithProviders(<App />);
    await findByText("Everything is up to date");

    await waitFor(() =>
      expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "report_update_set")).toEqual([
        ["report_update_set", { round: snapshot.round, updates: [] }],
      ]),
    );
  });

  it("opens the Updates page when clicked, whichever page the window was left on", async () => {
    const rust = fakeMenuBar();
    const { findByText, findByRole, getByRole } = renderWithProviders(<App />);
    await findByText("Everything is up to date");
    fireEvent.click(getByRole("button", { name: "Settings" }));
    await findByRole("heading", { level: 1, name: "Settings" });
    expect(rust.listening()).toContain(OPEN_UPDATES_EVENT);

    rust.hear(OPEN_UPDATES_EVENT);

    expect(await findByRole("heading", { level: 1, name: "Updates" })).toBeInTheDocument();
    expect(getByRole("button", { name: "Updates" })).toHaveAttribute("aria-current", "page");
  });
});

describe("quitting while an operation is under way", () => {
  it("is asked about from the start: the window listens, and then tells Rust to ask", async () => {
    const rust = fakeMenuBar();
    const { findByText } = renderWithProviders(<App />);
    await findByText("Everything is up to date");

    expect(rust.listening()).toContain(QUIT_REQUESTED_EVENT);
    await waitFor(() =>
      expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "ask_before_quit")).toEqual([
        ["ask_before_quit", { ask: true }],
      ]),
    );
  });

  it("asks over whichever page is open, and quits on Quit anyway", async () => {
    const rust = fakeMenuBar();
    const running: OpSummary = {
      id: 1,
      kind: "Upgrade",
      instance_id: "brew:/opt/homebrew",
      artifact_kind: "Formula",
      name: "jq",
      status: "Running",
      outcome: null,
      argv_preview: [],
      cancel_policy: "KillThenReconcile",
    };
    const answer = mockInvoke.getMockImplementation();
    mockInvoke.mockImplementation((cmd: string, args?: InvokeArgs) => {
      if (cmd === "list_operations") return Promise.resolve([running]);
      return answer === undefined ? Promise.resolve(undefined) : answer(cmd, args);
    });
    const { findByText, findByRole, getByRole } = renderWithProviders(<App />);
    await findByText("Everything is up to date");
    fireEvent.click(getByRole("button", { name: "Settings" }));
    await findByRole("heading", { level: 1, name: "Settings" });
    await waitFor(() => expect(rust.listening()).toContain(QUIT_REQUESTED_EVENT));

    rust.hear(QUIT_REQUESTED_EVENT, 1);

    const dialog = await findByRole("dialog", { name: "1 operation hasn't finished" });
    // On screen, and Rust is told so: it waits for the answer.
    await waitFor(() =>
      expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "quit_question_shown")).toEqual([
        ["quit_question_shown", { question: 1 }],
      ]),
    );
    fireEvent.click(within(dialog).getByRole("button", { name: "Quit" }));
    await waitFor(() =>
      expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "quit_anyway")).toEqual([["quit_anyway"]]),
    );
  });
});

describe("the menu bar's items that act in the page", () => {
  /** How many times the page has asked for a refresh. */
  function refreshes(): number {
    return mockInvoke.mock.calls.filter(([cmd]) => cmd === "refresh").length;
  }

  it("are listened for from the start: Settings…, the View menu's four pages, Check Again, Search and Help's Copy Diagnostic Info", async () => {
    const menu = fakeMenuBar();
    const { findByText } = renderWithProviders(<App />);
    await findByText("Everything is up to date");

    expect(menu.listening().filter((event) => event.startsWith("menu://"))).toEqual([
      "menu://check-again",
      "menu://copy-diagnostics",
      "menu://installed",
      "menu://overview",
      "menu://search",
      "menu://settings",
      "menu://unknown",
      "menu://updates",
    ]);
  });

  it("open Settings on Settings…, as the sidebar's Settings does", async () => {
    const menu = fakeMenuBar();
    const { findByText, findByRole, getByRole } = renderWithProviders(<App />);
    await findByText("Everything is up to date");

    menu.choose("settings");

    expect(await findByRole("heading", { level: 1, name: "Settings" })).toBeInTheDocument();
    expect(getByRole("button", { name: "Settings" })).toHaveAttribute("aria-current", "page");
  });

  it("copy the diagnostic text on Help's Copy Diagnostic Info, without the tools, then open Settings where it says so", async () => {
    const menu = fakeMenuBar();
    const { findByText, findByRole, container } = renderWithProviders(<App />);
    await findByText("Everything is up to date");
    const writeText = vi.fn(async (_text: string) => {});
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    const scrolled: Element[] = [];
    Element.prototype.scrollIntoView = function (this: Element) {
      scrolled.push(this);
    };
    try {
      menu.choose("copyDiagnostics");

      expect(writeText).toHaveBeenCalledTimes(1);
      expect(writeText.mock.calls[0][0].startsWith("Diagnostic info\n")).toBe(true);
      expect(writeText.mock.calls[0][0]).not.toMatch(/^ {4}\S/m);
      expect(await findByRole("heading", { level: 1, name: "Settings" })).toBeInTheDocument();
      await waitFor(() =>
        expect(container.querySelector("[data-diagnostics-status]")).toHaveTextContent("Copied"),
      );
      // At the foot of Settings: its row brought into view, once.
      const row = container.querySelector("[data-copy-diagnostics]")?.closest("div.flex.min-h-9");
      expect(scrolled.filter((element) => element === row)).toHaveLength(1);
    } finally {
      Object.defineProperty(navigator, "clipboard", { value: undefined, configurable: true });
      delete (Element.prototype as { scrollIntoView?: unknown }).scrollIntoView;
    }
  });

  it("open each page on its item in the View menu (⌘1 to ⌘4), as its row in the sidebar does", async () => {
    const menu = fakeMenuBar();
    const { findByText, findByRole, getByRole } = renderWithProviders(<App />);
    await findByText("Everything is up to date");

    // From the Overview, where the window opens, to each of the others and back.
    for (const [command, page] of [
      ["unknown", "Other Programs"],
      ["installed", "Installed"],
      ["updates", "Updates"],
      ["overview", "Overview"],
    ] as const) {
      menu.choose(command);

      expect(await findByRole("heading", { level: 1, name: page })).toBeInTheDocument();
      expect(getByRole("button", { name: page })).toHaveAttribute("aria-current", "page");
    }
  });

  it("open Other Programs on ⌘4 from a source's tools: its own page, and its row under 「来源」 selected in the source's place, in view", async () => {
    const menu = fakeMenuBar();
    // jsdom lays nothing out and has no `scrollIntoView`: what is scrolled
    // into view is noted, with how.
    const scrolled: Array<{ element: Element; options: boolean | ScrollIntoViewOptions | undefined }> = [];
    Element.prototype.scrollIntoView = function (this: Element, options?: boolean | ScrollIntoViewOptions) {
      scrolled.push({ element: this, options });
    };
    onTestFinished(() => {
      delete (Element.prototype as Partial<Element>).scrollIntoView;
    });
    const { findByRole, findByText, getByRole, queryByRole } = renderWithProviders(<App />);
    const sources = await findByRole("list", { name: "Sources" });
    const homebrew = await within(sources).findByRole("button", { name: "Homebrew" });
    fireEvent.click(homebrew);
    expect(await findByRole("heading", { level: 1, name: "Homebrew" })).toBeInTheDocument();
    expect(homebrew).toHaveAttribute("aria-current", "page");

    menu.choose("unknown");

    expect(await findByRole("heading", { level: 1, name: "Other Programs" })).toBeInTheDocument();
    // Its page, with its own list and Scan again, not the Installed page's
    // search over one source's tools.
    expect(await findByText("No other programs")).toBeInTheDocument();
    expect(within(getByRole("banner")).getByRole("button", { name: "Scan Again" })).toBeInTheDocument();
    expect(queryByRole("searchbox")).toBeNull();
    expect(queryByRole("textbox", { name: "Search installed tools" })).toBeNull();
    // The last row under 「来源」 is the one selected, and the source's is not.
    const other = within(sources).getByRole("button", { name: "Other Programs" });
    const rows = within(sources).getAllByRole("button");
    expect(rows[rows.length - 1]).toBe(other);
    expect([...document.querySelectorAll('[aria-current="page"]')]).toEqual([other]);
    expect(homebrew).not.toHaveAttribute("aria-current");
    // Brought into view: the last row, below the fold of a short window
    // on a Mac with many sources.
    expect(scrolled[scrolled.length - 1]).toEqual({ element: other, options: { block: "nearest" } });
  });

  it("open Installed on everything on Installed (⌘3), from one source's tools and a search, as the sidebar's Installed does", async () => {
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
    const menu = fakeMenuBar();
    const { findByRole, findByText, findByLabelText, getByRole, queryByText } = renderWithProviders(<App />);
    const sources = await findByRole("list", { name: "Sources" });
    fireEvent.click(await within(sources).findByRole("button", { name: "npm" }));
    fireEvent.change(await findByLabelText("Search installed tools"), { target: { value: "type" } });
    expect(await findByText("typescript", { selector: "[data-tool-row] p" })).toBeInTheDocument();
    expect(queryByText("jq", { selector: "[data-tool-row] p" })).toBeNull();

    menu.choose("installed");

    expect(await findByText("jq", { selector: "[data-tool-row] p" })).toBeInTheDocument();
    expect(queryByText("typescript", { selector: "[data-tool-row] p" })).toBeInTheDocument();
    expect(getByRole("heading", { level: 1 })).toHaveTextContent(/^Installed$/);
    expect(await findByLabelText("Search installed tools")).toHaveValue("");
    expect(getByRole("button", { name: "Installed" })).toHaveAttribute("aria-current", "page");
    expect(within(sources).getByRole("button", { name: "npm" })).not.toHaveAttribute("aria-current");
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
    const toolbar = getByRole("heading", { level: 1 }).closest("header") as HTMLElement;
    const checkAgain = within(toolbar).getByRole("button", { name: "Check Again" });
    await waitFor(() => expect(checkAgain).not.toHaveAttribute("aria-disabled"));
    expect(refreshes()).toBe(1);

    menu.choose("checkAgain");

    await waitFor(() => expect(refreshes()).toBe(2));
    // The header says so, as for its own button.
    expect(checkAgain).toHaveAttribute("aria-disabled", "true");
    expect(checkAgain).toHaveAttribute("title", "Check Again (⌘R) · Checking…");

    // Chosen again while it runs: no second check, now or after it.
    menu.choose("checkAgain");
    menu.choose("checkAgain");
    expect(waiting).toHaveLength(1);
    waiting[0](snapshot);

    await waitFor(() => expect(checkAgain).not.toHaveAttribute("aria-disabled"));
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

    const box = await findByLabelText("Search installed tools");
    await waitFor(() => expect(document.activeElement).toBe(box));
    expect(getByRole("heading", { level: 1, name: "Installed" })).toBeInTheDocument();
    expect(getByRole("button", { name: "Installed" })).toHaveAttribute("aria-current", "page");
  });

  it("keep the Installed page's search on Search there, its text selected to be typed over", async () => {
    const menu = fakeMenuBar();
    const { findByText, findByLabelText, getByRole } = renderWithProviders(<App />);
    await findByText("Everything is up to date");
    fireEvent.click(getByRole("button", { name: "Installed" }));
    const box = (await findByLabelText("Search installed tools")) as HTMLInputElement;
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
    expect(queryByLabelText("Search installed tools")).toBeNull();
    await waitFor(() => expect(finishFirstCheck).toBeDefined());
    finishFirstCheck?.(snapshot);

    const box = await findByLabelText("Search installed tools");
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
    expect(queryByLabelText("Search installed tools")).toBeNull();
    await waitFor(() => expect(answers.has("get_snapshot")).toBe(true));
    answers.get("get_snapshot")?.(snapshot);

    const box = await findByLabelText("Search installed tools");
    await waitFor(() => expect(document.activeElement).toBe(box));
    // The startup check, which nothing here waited on, finishes too.
    answers.get("refresh")?.(snapshot);
    await waitFor(() => expect(getByRole("button", { name: "Check Again" })).not.toHaveAttribute("aria-disabled"));
  });
});
