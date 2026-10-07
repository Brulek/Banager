import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { invoke, type InvokeArgs } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { command } from "../test/command";
import { WithToolbarSlot } from "../test/toolbarSlot";
import { InstalledPage } from "./InstalledPage";
import { BUTTON } from "../components/ui/controls";
import { UpdatesPage } from "./UpdatesPage";
import { SnapshotStatus } from "../components/SnapshotStatus";
import { PageHeader } from "../components/PageHeader";
import { useUiStore } from "../store/ui";
import { queryKeys } from "../lib/queries";
import i18n from "../i18n";
import { loadToolIcons } from "../lib/toolIcons";
import { lazyDescriptionTable } from "../lib/toolDescriptions";
import type {
  InstalledArtifact,
  ManagerInstance,
  OpRequest,
  OpSummary,
  Settings,
  Snapshot,
  UpdateCandidate,
} from "../lib/types";
import { NO_FACTS } from "../lib/types";
import { failureCause } from "../lib/failureCause";
import { writeInventoryPreview } from "../lib/events";

const mockInvoke = vi.mocked(invoke);

/**
 * A row's own Uninstall, named with its tool's name inside its words
 * ("Uninstall jq…"); and any Uninstall, the row's or the inspector's,
 * whose name is its words alone.
 */
const ROW_UNINSTALL = /^Uninstall .+…$/;
const ANY_UNINSTALL = /^Uninstall(?: .+)?…$/;

// The pretend Mac's two models (`MODELS` in src/dev/mockData.ts, which no
// test outside src/dev may import): one from another registry, one from
// Ollama's own.
const MODELS = {
  coder: "modelscope.cn/Qwen/Qwen2.5-Coder-7B-Instruct-GGUF:Q4_K_M",
  llama: "llama3.2:3b",
} as const;

const brew: ManagerInstance = {
  id: "brew:/opt/homebrew",
  adapter_id: "brew",
  exe_path: "/opt/homebrew/bin/brew",
  prefix: "/opt/homebrew",
  scope: "User",
  version: "7.0.3",
  status: { unavailable: null, notes: [] },
  answered_at: null,
  unverified_version: null,
  read_only_reason: null,
};

const snapshot: Snapshot = {
  generation: 1,
  round: 1,
  detect: "Found",
  instances: [brew],
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
      uninstall_blocked: null,
      facts: NO_FACTS,
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
      blocked: null,
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
  skipped_versions: [],
  include_self_updating: false,
  auto_check: false,
  notify_updates: false,
};

const pip: ManagerInstance = {
  id: "pip:/usr/bin/python3",
  adapter_id: "pip",
  exe_path: "/usr/bin/python3",
  prefix: "/usr",
  scope: "User",
  version: "26.2.1",
  status: { unavailable: null, notes: [] },
  answered_at: null,
  unverified_version: null,
  read_only_reason: "ByDesign",
};

// One pip instance with one package.
const pipSnapshot: Snapshot = {
  generation: 1,
  round: 1,
  detect: "Found",
  instances: [pip],
  artifacts: [
    {
      key: { instance_id: "pip:/usr/bin/python3", kind: "Package", name: "requests" },
      display_name: "requests",
      version: "2.32.3",
      // Unknown, not Requested: pip's `--not-required` marks a leaf
      // package, which is not the same as "the user asked for it", so
      // Task 8's adapter can only ever emit Unknown or Dependency here.
      // A "Requested" fixture would pass against data pip cannot produce.
      reason: "Unknown",
      description: "Python HTTP for Humans.",
      homepage: null,
      size_bytes: null,
      installed_at: null,
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

const claudeInstance: ManagerInstance = {
  id: "standalone-claude",
  adapter_id: "standalone-claude",
  exe_path: "/Users/someone/.local/bin/claude",
  prefix: "/Users/someone/.local/share/claude",
  scope: "User",
  version: "2.1.281",
  status: { unavailable: null, notes: [] },
  answered_at: null,
  unverified_version: null,
  read_only_reason: null,
};

const claudeArtifact: InstalledArtifact = {
  key: { instance_id: "standalone-claude", kind: "Binary", name: "claude" },
  display_name: "Claude Code",
  version: "2.1.281",
  reason: "Requested",
  description: null,
  homepage: "https://code.claude.com/docs/en/setup",
  size_bytes: null,
  installed_at: null,
  path: "/Users/someone/.local/share/claude/versions/2.1.281",
  auto_updates: true,
  uninstall_blocked: null,
  facts: NO_FACTS,
};

const OLLAMA = "ollama:http://127.0.0.1:11434";
const ollama: ManagerInstance = {
  id: OLLAMA,
  adapter_id: "ollama",
  exe_path: "/usr/local/bin/ollama",
  prefix: "/usr/local",
  scope: "User",
  version: "0.34.1",
  status: { unavailable: null, notes: [] },
  answered_at: null,
  unverified_version: null,
  read_only_reason: null,
};

/** A Homebrew formula named `name`, with `over`'s fields. */
function formula(name: string, over: Partial<InstalledArtifact> = {}): InstalledArtifact {
  return {
    ...snapshot.artifacts[0],
    key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name },
    display_name: name,
    description: `${name} blurb`,
    ...over,
  };
}

// What the backend answers. Each test sets what it needs; `plan_operation`
// issues a plan for whatever it is asked, and `submit_operation` starts
// operation 7.
let served: Snapshot;
let servedSettings: Settings;
let operations: OpSummary[];

// Heights a slot reports to the virtualizer. `rowHeights` lets one test
// make a single slot taller than the rest; every other one falls back to
// DEFAULT_ROW_HEIGHT, and the scroll container is VIEWPORT tall.
const DEFAULT_ROW_HEIGHT = 56;
let rowHeights: Record<number, number> = {};
let viewport = 600;

function issuedPlan(request: OpRequest, affected: string[] = []) {
  return {
    id: "1",
    plan: {
      request,
      action: {
        Command: {
          program: "/opt/homebrew/bin/brew",
          args: [request.kind === "Uninstall" ? "uninstall" : "upgrade", "--formula", request.name],
          env: [],
        },
      },
      needs_password: false,
      locks: ["brew:/opt/homebrew"],
      cancel_policy: "KillThenReconcile",
      warnings: [],
      affected,
      timeout_secs: 1800,
    },
    issued_at: 1758000000,
  };
}

let planAffected: string[];

beforeEach(() => {
  mockInvoke.mockReset();
  served = snapshot;
  servedSettings = settings;
  operations = [];
  rowHeights = {};
  viewport = 600;
  planAffected = [];
  // @tanstack/react-virtual measures both its scroll container and each
  // slot through offsetWidth / offsetHeight, which jsdom hardcodes to 0
  // with no layout engine behind them; without these the virtualizer sees
  // a zero-size viewport and renders no rows at all. The slots are the
  // elements carrying `data-index`; everything else, the scroll container
  // included, gets the viewport's height.
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (
    this: HTMLElement,
  ) {
    const index = this.getAttribute("data-index");
    if (index === null) return viewport;
    return rowHeights[Number(index)] ?? DEFAULT_ROW_HEIGHT;
  });
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
  mockInvoke.mockImplementation((cmd: string, args?: unknown) => {
    if (cmd === "get_snapshot") return Promise.resolve(served);
    if (cmd === "get_settings") return Promise.resolve(servedSettings);
    if (cmd === "list_operations") return Promise.resolve(operations);
    if (cmd === "plan_operation") {
      return Promise.resolve(issuedPlan((args as { request: OpRequest }).request, planAffected));
    }
    if (cmd === "submit_operation") return Promise.resolve(7);
    return Promise.resolve(undefined);
  });
});

afterEach(() => {
  vi.restoreAllMocks();
});

/** The page as `App` draws it: its sort and search field in the toolbar's slot. */
function renderInstalled(options?: Parameters<typeof renderWithProviders>[1]) {
  return renderWithProviders(
    <WithToolbarSlot>
      <InstalledPage />
    </WithToolbarSlot>,
    options,
  );
}

// The list's row for `name` -- the name as the row shows it.
function rowOf(name: string): HTMLElement {
  const rows = screen
    .getAllByText(name, { selector: "[data-tool-row] p" })
    .map((element) => element.closest("[data-tool-row]"));
  if (rows.length !== 1 || !(rows[0] instanceof HTMLElement)) {
    throw new Error(`expected one row named ${name}, found ${rows.length}`);
  }
  return rows[0];
}

async function findRow(name: string): Promise<HTMLElement> {
  await screen.findByText(name, { selector: "[data-tool-row] p" });
  return rowOf(name);
}

// The names of the rows, top to bottom.
function rowNames(): string[] {
  return [...document.querySelectorAll("[data-tool-row]")].map(
    (row) => row.querySelector("p")?.textContent ?? "",
  );
}

// The words of a row's status: one at most (spec §3.4).
function chipsOf(row: HTMLElement): string[] {
  const status = row.querySelector("[data-status]");
  return status === null ? [] : [...status.children].map((chip) => chip.textContent ?? "");
}

// What a row's version column says: "2.88.3 → 2.90.0" where an update is listed.
function versionShown(row: HTMLElement): string | null {
  return row.querySelector(".tabular-nums")?.textContent ?? null;
}

// Every status word `name`'s details list, in order -- the row shows the
// first that is not a normal state, and the inspector every one -- read
// from its inspector, which is closed again.
async function drawerChips(name: string): Promise<string[]> {
  const inspector = await openDetails(name);
  const list = inspector.querySelector("[data-status-list]");
  const words = list === null ? [] : [...list.children].map((item) => item.firstElementChild?.textContent ?? "");
  fireEvent.keyDown(inspector, { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("complementary")).toBeNull());
  return words;
}

// Opens the chip called `label` on `row`, and returns what it shows.
function chipDetail(row: HTMLElement, label: string): HTMLElement {
  // By its word: a chip the same on many rows is named with its tool's
  // name too ("Can't uninstall jq now").
  const chip = within(row).getByRole("button", { name: (_name, element) => element.textContent === label });
  fireEvent.click(chip);
  const panel = document.getElementById(chip.getAttribute("aria-controls") ?? "");
  if (panel === null) throw new Error(`the ${label} chip opened nothing`);
  return panel;
}

// A paragraph whose whole text, across the `<code>` a command is set in,
// is `text`.
function wholeSentence(text: string) {
  return (_content: string, element: Element | null) =>
    (element?.tagName === "P" || element?.hasAttribute("data-detail-line") === true) && element.textContent === text;
}

// Opens a confirmation's "Show Command": it is one press away.
function showCommand(dialog: HTMLElement) {
  const disclosure = within(dialog).getByRole("button", { name: /^Show Command/ });
  if (disclosure.getAttribute("aria-expanded") !== "true") fireEvent.click(disclosure);
}

// Presses `name`'s row itself, and returns the inspector that shows it.
// Opens the ⓘ after `word` in the inspector's 「状态」 row, and returns
// what it shows: the word's why, as a row's ⓘ shows it.
function statusWhy(inspector: HTMLElement, word: string): HTMLElement {
  const info = within(inspector).getByRole("button", { name: `Details: ${word}` });
  fireEvent.click(info);
  const panel = document.getElementById(info.getAttribute("aria-controls") ?? "");
  if (panel === null) throw new Error(`the ${word} ⓘ opened nothing`);
  return panel;
}

async function openDetails(name: string): Promise<HTMLElement> {
  const row = await findRow(name);
  fireEvent.click(within(row).getByRole("button", { name: `Details: ${name}` }));
  return screen.findByRole("complementary", { name });
}

describe("InstalledPage", () => {
  it("shows the first check's spinner and why it takes a while until the backend has answered, not Loading…", async () => {
    // At launch `get_snapshot` has not answered yet: the first check is
    // under way, as the Overview says in the same words.
    const answer = mockInvoke.getMockImplementation();
    mockInvoke.mockImplementation((cmd: string, args?: InvokeArgs) =>
      cmd === "get_snapshot" ? new Promise(() => {}) : answer!(cmd, args),
    );
    const { findByRole, getByText, queryByText, container } = renderInstalled();

    expect(await findByRole("heading", { level: 2, name: "Checking…" })).toBeInTheDocument();
    expect(
      getByText("The first check looks up every tool's newest version online, and sometimes takes a minute or two."),
    ).toBeInTheDocument();
    expect(container.querySelector("[data-first-check] svg")).toHaveAttribute("width", "32");
    expect(queryByText("Loading…")).not.toBeInTheDocument();
  });

  it("shows the requested artifact and folds the one other software brought in into a line", async () => {
    const { findByText, queryByText, getByRole } = renderInstalled();

    await findByText("jq");
    expect(queryByText("glib")).not.toBeInTheDocument();
    expect(getByRole("button", { name: /^1 more package was installed for other software to use/ })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
  });

  it("unfolds the component under its line, and folds it back", async () => {
    const { findByText, getByRole, queryByText } = renderInstalled();

    await findByText("jq");
    fireEvent.click(getByRole("button", { name: /^1 more package was installed for other software to use/ }));
    await findByText("glib");
    // Under its line, which stays to fold it back up.
    const fold = getByRole("button", { name: /^Hide 1 package/ });
    expect(fold).toHaveAttribute("aria-expanded", "true");
    expect(rowNames()).toEqual(["jq", "glib"]);
    expect(
      fold.compareDocumentPosition(rowOf("glib")) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();

    fireEvent.click(fold);
    await waitFor(() => expect(queryByText("glib")).not.toBeInTheDocument());
  });

  it("parts a row by a hairline from the next row only: none over a fold's line, the selected row, or under the last", async () => {
    served = { ...snapshot, artifacts: [...snapshot.artifacts, formula("wget"), formula("zlib")] };
    const { getByRole } = renderInstalled();
    await findRow("zlib");
    expect(rowNames()).toEqual(["jq", "wget", "zlib"]);
    // A slot with no hairline under it is marked for index.css, from the
    // list's items (`hairlineBefore`), not found by the stylesheet.
    const runEnds = () =>
      ["jq", "wget", "zlib"].map((name) => rowOf(name).closest("[data-list-slot]")?.hasAttribute("data-run-end"));
    expect(runEnds()).toEqual([false, false, true]);
    const fold = getByRole("button", { name: /^1 more package was installed for other software to use/ });
    expect(fold.closest("[data-list-slot]")).toHaveAttribute("data-run-end");

    // None over the selection: the row before's is hidden, its own by index.css.
    await openDetails("wget");
    expect(runEnds()).toEqual([true, false, true]);
    await openDetails("jq");
    expect(runEnds()).toEqual([false, false, true]);
  });

  it("filters rows by the search box, by the name a row shows or the package's own, and says when nothing matches", async () => {
    served = {
      ...snapshot,
      artifacts: [
        ...snapshot.artifacts,
        formula("visual-studio-code", {
          key: { instance_id: "brew:/opt/homebrew", kind: "Cask", name: "visual-studio-code" },
          display_name: "Microsoft Visual Studio Code",
        }),
      ],
    };
    const { findByText, queryByText, getByRole, getByText } = renderInstalled();

    await findByText("jq");
    const search = getByRole("searchbox", { name: "Search installed tools" });
    expect(search).toHaveAttribute("placeholder", "Search");

    fireEvent.change(search, { target: { value: "VISUAL-studio" } });
    await waitFor(() => expect(rowNames()).toEqual(["Microsoft Visual Studio Code"]));

    const status = document.querySelector("[data-search-status]") as HTMLElement;
    expect(status).toHaveAttribute("role", "status");
    expect(status).toBeEmptyDOMElement();
    fireEvent.change(search, { target: { value: "nonexistent" } });
    await waitFor(() => expect(queryByText("jq")).not.toBeInTheDocument());
    // Said to a screen reader through the search's status, the same node
    // as before, beside the field -- and not twice.
    expect(document.querySelector("[data-search-status]")).toBe(status);
    expect(status).toHaveTextContent("Nothing matches “nonexistent”");
    expect(status).toHaveClass("sr-only");
    // One line in the middle of the list's area, 13 muted, no symbol.
    const line = getByText("Nothing matches “nonexistent”", { selector: "[data-list-empty]" });
    expect(line).toHaveAttribute("aria-hidden", "true");
    expect(line.className.split(" ")).toEqual(
      expect.arrayContaining(["flex", "flex-1", "items-center", "justify-center", "text-body", "text-muted"]),
    );
    expect(line.parentElement?.className.split(" ")).toEqual(expect.arrayContaining(["flex", "h-full", "flex-col"]));
    expect(line.parentElement?.parentElement).toHaveAttribute("data-list");
    expect(line.querySelector("svg")).toBeNull();
    expect(line.className).not.toMatch(/py-10/);

    // Something matches again: the status has nothing to say.
    fireEvent.change(search, { target: { value: "jq" } });
    await waitFor(() => expect(rowNames()).toEqual(["jq"]));
    expect(status).toBeEmptyDOMElement();
  });

  it("finds a tool by a command it puts on the Mac, by the command's start, and names the command on the row", async () => {
    const runs = (names: string[]) => names.map((name) => ({ name, state: "Runs" as const }));
    served = {
      ...snapshot,
      artifacts: [
        ...snapshot.artifacts,
        formula("ripgrep", { facts: { ...NO_FACTS, commands: runs(["rg"]) } }),
        formula("python@3.13", {
          facts: { ...NO_FACTS, commands: runs(["idle3.13", "pip3.13", "pydoc3.13", "python3.13"]) },
        }),
        formula("gh", { facts: { ...NO_FACTS, commands: runs(["gh"]) } }),
        // A package called as its command, which its row shows neither of.
        formula("agy", {
          display_name: "Antigravity CLI",
          description: "Google's AI coding assistant",
          facts: { ...NO_FACTS, commands: runs(["agy"]) },
        }),
      ],
    };
    const { findByText, getByRole } = renderInstalled();

    await findByText("jq");
    const search = getByRole("searchbox", { name: "Search installed tools" });
    // What it searches by, in its tooltip; the placeholder stays 「Search」.
    expect(search).toHaveAttribute("title", "Search by name, description or command");
    expect(search).toHaveAttribute("placeholder", "Search");
    const noteOf = (name: string) => within(rowOf(name)).queryByText(/^Command: /)?.textContent ?? null;

    fireEvent.change(search, { target: { value: "RG" } });
    await waitFor(() => expect(rowNames()).toEqual(["ripgrep"]));
    expect(noteOf("ripgrep")).toBe("Command: rg");
    // A screen reader hears it too, after the row's name, as the row's description.
    expect(rowOf("ripgrep")).toHaveAttribute("role", "group");
    expect(rowOf("ripgrep")).toHaveAccessibleDescription("Command: rg");

    fireEvent.change(search, { target: { value: "pip3.13" } });
    await waitFor(() => expect(rowNames()).toEqual(["python@3.13"]));
    expect(noteOf("python@3.13")).toBe("Command: pip3.13");

    // Found by its name: no word about a command, though one is called so too.
    fireEvent.change(search, { target: { value: "gh" } });
    await waitFor(() => expect(rowNames()).toEqual(["gh"]));
    expect(noteOf("gh")).toBeNull();
    expect(rowOf("gh")).not.toHaveAttribute("aria-describedby");

    // Found by its package's name, which is also its command, and which
    // the row does not show: the row says which command.
    fireEvent.change(search, { target: { value: "agy" } });
    await waitFor(() => expect(rowNames()).toEqual(["Antigravity CLI"]));
    expect(noteOf("Antigravity CLI")).toBe("Command: agy");

    // Letters in a command's middle find nothing.
    fireEvent.change(search, { target: { value: "ip3" } });
    await waitFor(() => expect(rowNames()).toEqual([]));
  });

  it("puts its sort and its search field in the toolbar: a grey popup button, then a quiet field 200 wide", async () => {
    const { findByRole, getByRole, container } = renderInstalled();

    const search = await findByRole("searchbox", { name: "Search installed tools" });
    const slot = container.querySelector("[data-toolbar-slot]") as HTMLElement;
    expect(slot.contains(search)).toBe(true);
    // 24 high, a control's corners, the quietest fill and no outline.
    expect(search.className.split(" ")).toEqual(
      expect.arrayContaining(["h-6", "rounded-control", "bg-fill-subtle", "text-body", "appearance-none"]),
    );
    expect(search.className).not.toMatch(/\bborder\b|shadow/);
    expect((search.parentElement as HTMLElement).className.split(" ")).toEqual(expect.arrayContaining(["w-50", "h-6"]));
    expect(search.parentElement?.querySelector("svg")).not.toBeNull();

    // The sort before it (spec §3.2): the value and ⌄ on a grey button.
    const sort = getByRole("combobox", { name: "Sort Order" });
    expect(slot.contains(sort)).toBe(true);
    expect(sort.compareDocumentPosition(search) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    const popup = sort.parentElement as HTMLElement;
    expect(popup.className.split(" ")).toEqual(expect.arrayContaining(["h-6", "rounded-control", "bg-fill", "text-body"]));
    expect(within(sort).getAllByRole("option").map((option) => option.textContent)).toEqual([
      "By Name",
      "By Source",
      "By Size",
      "By Date Installed",
    ]);
    // Nothing of either over the list any more.
    const list = container.querySelector("[data-list]") as HTMLElement;
    expect(list.parentElement?.previousElementSibling).toBeNull();
  });

  it("focuses its search field for ⌘F once it is in the toolbar, its text selected to be typed over", async () => {
    useUiStore.setState({ page: "installed", query: "gl" });
    act(() => useUiStore.getState().searchInstalled());
    const { findByRole } = renderInstalled();

    const search = (await findByRole("searchbox", { name: "Search installed tools" })) as HTMLInputElement;
    await waitFor(() => expect(document.activeElement).toBe(search));
    expect(search.selectionStart).toBe(0);
    expect(search.selectionEnd).toBe(2);
    expect(useUiStore.getState().searchFocusRequested).toBe(false);

    // Pressed again with the page open, the field already there.
    act(() => search.blur());
    act(() => useUiStore.getState().searchInstalled());
    await waitFor(() => expect(document.activeElement).toBe(search));
    expect(useUiStore.getState().searchFocusRequested).toBe(false);
  });

  it("draws a source's components as a 32-high line that discloses them: a 10pt triangle, 13 muted", async () => {
    renderInstalled();

    await findRow("jq");
    const fold = screen.getByRole("button", { name: /^1 more package was installed for other software to use/ });
    expect(fold.className.split(" ")).toEqual(expect.arrayContaining(["h-8", "px-5", "text-body", "text-muted"]));
    expect(fold.className).not.toMatch(/rounded|bg-|border/);
    // On the rows' grid, as the Updates page's 「另有N个无法在这里更新」:
    // the triangle in a 16 slot 8 in from the rows' edge, centred on the
    // 32 avatars' column (x 36), the words 20 past it, where the names
    // start (x 64) -- no gap of the line's own to throw either off.
    expect(fold.className.split(" ")).not.toContain("gap-1.5");
    const [slot, words] = [...fold.children] as HTMLElement[];
    expect(slot).toHaveAttribute("data-disclosure-symbol");
    expect(slot.className.split(" ")).toEqual(
      expect.arrayContaining(["ml-2", "w-4", "flex", "justify-center", "shrink-0"]),
    );
    expect(words.className.split(" ")).toContain("ml-5");
    expect(words.textContent).toBe("1 more package was installed for other software to use");
    const avatar = rowOf("jq").querySelector("[aria-hidden='true']") as HTMLElement;
    expect(avatar.querySelector("[data-program-tile]")?.className).toMatch(/\bh-8 w-8\b/);
    expect(avatar.parentElement?.nextElementSibling?.className.split(" ")).toContain("ml-3");
    const triangle = slot.firstElementChild as SVGElement;
    expect(triangle).toHaveAttribute("width", "10");
    expect(triangle.getAttribute("class")).not.toContain("rotate-90");
    // One of the rows ↑ ↓ reach, in the Tab order's roving; its inset
    // focus ring (index.css) is drawn in its own box.
    expect(fold).toHaveAttribute("data-row-focus");
    expect(fold.className.split(" ")).toContain("relative");

    fireEvent.click(fold);
    await findRow("glib");
    const open = screen.getByRole("button", { name: /^Hide 1 package/ });
    expect(open.querySelector("svg")?.getAttribute("class")).toContain("rotate-90");
  });

  it("narrows the inspector to 260 in a window under 900 wide, beside the list, never over it", async () => {
    // The page as a window at its narrowest lays it out: 592 wide (800
    // less the sidebar's 208).
    let pageWidth = 592;
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
      const width = pageWidth;
      return { width, height: 500, top: 0, left: 0, right: width, bottom: 500, x: 0, y: 0, toJSON: () => ({}) };
    });
    const { unmount } = renderInstalled();

    let inspector = await openDetails("jq");
    expect(inspector).toHaveAttribute("data-inspector", "narrow");
    expect(inspector.className.split(" ")).toEqual(
      expect.arrayContaining(["w-65", "shrink-0", "border-l", "border-separator", "bg-content"]),
    );
    expect(inspector.className).not.toMatch(/\bw-75\b|absolute|shadow/);
    // The list its own box before it, its right edge the inspector's
    // hairline: its rows' selection and separators end there.
    const list = document.querySelector("[data-list]") as HTMLElement;
    expect(list.parentElement?.className.split(" ")).toEqual(expect.arrayContaining(["min-w-0", "flex-1"]));
    expect(list.parentElement?.nextElementSibling).toBe(inspector);
    unmount();

    // A window 900 wide: the page 692, and the inspector 300.
    pageWidth = 692;
    renderInstalled();
    inspector = await openDetails("jq");
    expect(inspector).toHaveAttribute("data-inspector", "wide");
    expect(inspector.className.split(" ")).toContain("w-75");
  });

  it("checks no spelling in the search box: a tool's name is no word", async () => {
    const { findByRole } = renderInstalled();

    expect(await findByRole("searchbox", { name: "Search installed tools" })).toHaveAttribute("spellcheck", "false");
  });

  it("opens the uninstall dialog and plans it when the row's Uninstall is pressed", async () => {
    const { findByRole } = renderInstalled();

    const jq = await findRow("jq");
    fireEvent.click(within(jq).getByRole("button", { name: ROW_UNINSTALL }));

    const dialog = await findByRole("alertdialog", { name: "Uninstall “jq”?" });
    expect(mockInvoke).toHaveBeenCalledWith("plan_operation", {
      request: {
        kind: "Uninstall",
        instance_id: "brew:/opt/homebrew",
        artifact_kind: "Formula",
        name: "jq",
      },
    });
    await within(dialog).findByRole("button", { name: "Show Command" });
    showCommand(dialog);
    expect(within(dialog).getByText(command("/opt/homebrew/bin/brew uninstall --formula jq"))).toBeInTheDocument();
    // The row's button, not the row: nothing selected under the dialog.
    expect(screen.queryByRole("complementary", { name: "jq" })).toBeNull();
  });

  it("offers Uninstall as a grey button, on the row and in the inspector, where the inspector's Update is the default", async () => {
    // Uninstall must not look like the thing to do, nor like a warning: a
    // grey button with nothing red about it, under the pointer or not
    // (`RowAction`).
    renderInstalled();

    const rowUninstall = within(await findRow("jq")).getByRole("button", { name: ROW_UNINSTALL });
    // Named with its tool, its words first, as every row has one.
    expect(rowUninstall).toHaveAccessibleName("Uninstall jq…");
    expect(rowUninstall).toHaveTextContent(/^Uninstall…$/);
    expect(rowUninstall.className).toBe(BUTTON.regular.grey);
    expect(rowUninstall.className).not.toMatch(/danger|accent/);

    fireEvent.click(screen.getByRole("button", { name: /^1 more package was installed for other software to use/ }));
    // The inspector is a pane, not a dialog: a pane's regular buttons,
    // Uninstall at the left of its foot and Update at the right.
    const inspector = await openDetails("glib");
    const inspectorUninstall = within(inspector).getByRole("button", { name: "Uninstall…" });
    expect(inspectorUninstall.className).toBe(BUTTON.regular.grey);
    expect(inspectorUninstall.className).not.toMatch(/danger|accent/);
    const update = within(inspector).getByRole("button", { name: "Update" });
    expect(update.className).toBe(BUTTON.regular.default);
    expect(inspectorUninstall.compareDocumentPosition(update) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(update.parentElement).toBe(inspectorUninstall.parentElement);
  });

  it("names a row's Uninstall with its tool in Chinese too", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      renderInstalled();
      const jq = await findRow("jq");
      expect(within(jq).getByRole("button", { name: "卸载jq…" })).toHaveTextContent(/^卸载…$/);
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("gives the focus back to the row's Uninstall when its confirmation is cancelled", async () => {
    renderInstalled();

    const uninstall = within(await findRow("jq")).getByRole("button", { name: ROW_UNINSTALL });
    fireEvent.click(uninstall);
    const dialog = await screen.findByRole("alertdialog", { name: "Uninstall “jq”?" });
    await waitFor(() => expect(document.activeElement).toBe(within(dialog).getByRole("button", { name: "Cancel" })));

    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    await waitFor(() => expect(document.activeElement).toBe(uninstall));
    expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "submit_operation")).toHaveLength(0);
  });

  it("opens the log once an uninstall has started and the focus is back on its row", async () => {
    // The log drawer gives the focus back to what had it as it opened: so
    // it opens after the sheet has handed the focus back, and closing it
    // lands on the row where the user began -- not its Uninstall, off
    // from now on as 「正在卸载…」, which would drop the focus to the body.
    renderInstalled();

    const row = await findRow("jq");
    const uninstall = within(row).getByRole("button", { name: ROW_UNINSTALL });
    fireEvent.click(uninstall);
    const dialog = await screen.findByRole("alertdialog", { name: "Uninstall “jq”?" });
    const confirm = within(dialog).getByRole("button", { name: "Uninstall" });
    await waitFor(() => expect(confirm).toBeEnabled());
    fireEvent.click(confirm);

    await waitFor(() => expect(useUiStore.getState().drawerOpen).toBe(true));
    expect(useUiStore.getState().focusedOpId).toBe(7);
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(document.activeElement).toBe(row);
  });

  it("disables the dialog's confirm button when the plan reports dependents", async () => {
    planAffected = ["jq-cli-wrapper"];
    const { findByRole } = renderInstalled();

    fireEvent.click(within(await findRow("jq")).getByRole("button", { name: ROW_UNINSTALL }));

    const dialog = await findByRole("alertdialog", { name: "Uninstall “jq”?" });
    await within(dialog).findByText("jq-cli-wrapper");
    expect(within(dialog).getByRole("button", { name: "Uninstall" })).toBeDisabled();
  });

  it("says in a line that a source's version has not been tested, with why behind Details", async () => {
    served = { ...snapshot, instances: [{ ...brew, unverified_version: "99.9.9" }] };
    const { findByText, getByRole } = renderInstalled();

    await findByText("jq");
    // Named: the list is not grouped by source, so the line says whose.
    await findByText("Homebrew 99.9.9 not tested");
    const details = getByRole("button", { name: "Details: Homebrew 99.9.9 not tested" });
    fireEvent.click(details);
    expect(document.getElementById(details.getAttribute("aria-controls") ?? "")).toHaveTextContent(
      "Not tested with this version of Homebrew yet.",
    );
  });

  it("folds a version not tested in with the sources' lines, a silent source's first, and unfolds them all in their order", async () => {
    // After an update, 「Claude Code 2.1.290 版未经测试」 stacked up over
    // the list with 「uv 没有应答」 and the rest, a line each.
    const uv: ManagerInstance = {
      ...brew,
      id: "uv:/Users/someone/.local/share/uv/tools",
      adapter_id: "uv",
      exe_path: "/opt/homebrew/bin/uv",
      prefix: "/Users/someone/.local/share/uv/tools",
      version: "0.12.17",
      status: { unavailable: "NotResponding", notes: [] },
    };
    served = {
      ...snapshot,
      instances: [
        {
          ...claudeInstance,
          version: "2.1.290",
          answered_at: null,
          unverified_version: "2.1.290",
          status: { unavailable: null, notes: ["ShadowedByNpm"] },
        },
        uv,
      ],
      artifacts: [
        { ...claudeArtifact, version: "2.1.290" },
        formula("ruff", { key: { instance_id: uv.id, kind: "Tool", name: "ruff" } }),
      ],
      updates: [],
    };
    renderInstalled();

    await findRow("Claude Code");
    expect(screen.getByText("uv isn't responding")).toBeInTheDocument();
    expect(screen.queryByText("Typing claude in Terminal runs a program with that name from npm")).toBeNull();
    expect(screen.queryByText("Claude Code 2.1.290 not tested")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "2 more notes" }));

    const fewer = screen.getByRole("button", { name: "Show Fewer" });
    const lines = document.getElementById(fewer.getAttribute("aria-controls") ?? "");
    if (lines === null) throw new Error("Show fewer controls nothing");
    expect(
      within(lines)
        .getAllByRole("button", { name: /^Details: / })
        .map((button) => button.getAttribute("aria-label")),
    ).toEqual([
      "Details: Typing claude in Terminal runs a program with that name from npm",
      "Details: uv isn't responding",
      "Details: Claude Code 2.1.290 not tested",
    ]);
  });

  it("offers no Uninstall on a pinned package, keeps its description, and says how to release the pin behind its chip", async () => {
    // `brew uninstall jq` refuses a pinned formula without `--force` and
    // still exits 0 (`UninstallBlocked::Pinned` in
    // crates/banager-core/src/model.rs), so the row must not offer it. jq
    // is up to date: the pin comes from the inventory, not from an update.
    // The cask shows the `--cask` form; the unpinned formula keeps its button.
    served = {
      ...snapshot,
      artifacts: [
        { ...snapshot.artifacts[0], uninstall_blocked: "Pinned" },
        formula("onyx", {
          key: { instance_id: "brew:/opt/homebrew", kind: "Cask", name: "onyx" },
          display_name: "OnyX",
          description: "Verify system files structure",
          uninstall_blocked: "Pinned",
          facts: NO_FACTS,
        }),
        formula("wget", { description: "Internet file retriever" }),
      ],
      updates: [],
    };
    const { getAllByRole } = renderInstalled();

    await findRow("wget");
    // Only wget's.
    expect(getAllByRole("button", { name: ANY_UNINSTALL })).toHaveLength(1);
    expect(within(rowOf("wget")).getByRole("button", { name: ROW_UNINSTALL })).toBeInTheDocument();
    const jq = rowOf("jq");
    // The reason is behind the chip; the row keeps saying what jq is.
    expect(within(jq).getByText("Lightweight and flexible command-line JSON processor")).toBeInTheDocument();
    const detail = chipDetail(jq, "Pinned");
    expect(
      within(detail).getByText(
        wholeSentence("It's pinned in Homebrew. To uninstall it, first run /opt/homebrew/bin/brew unpin jq in Terminal."),
      ),
    ).toBeInTheDocument();
    expect(within(detail).getByText("/opt/homebrew/bin/brew unpin jq").tagName).toBe("CODE");
    expect(
      within(chipDetail(rowOf("OnyX"), "Pinned")).getByText("/opt/homebrew/bin/brew unpin --cask onyx").tagName,
    ).toBe("CODE");
    expect(within(rowOf("wget")).getByText("Internet file retriever")).toBeInTheDocument();
  });

  it("keeps a status column on every row while any row has a word, and gives its room to the names where none has", async () => {
    // jq up to date, glib -- asked for here -- with an update: neither is
    // a word on its row (the version says the update), so neither row has
    // the column.
    const glib = { ...snapshot.artifacts[1], reason: "Requested" as const };
    served = { ...snapshot, artifacts: [snapshot.artifacts[0], glib] };
    const first = renderInstalled();
    expect(chipsOf(await findRow("jq"))).toEqual([]);
    expect(chipsOf(await findRow("glib"))).toEqual([]);
    expect(document.querySelectorAll("[data-status-column]")).toHaveLength(0);
    first.unmount();

    // jq pinned: its word, and glib's empty column beside it.
    served = { ...snapshot, artifacts: [{ ...snapshot.artifacts[0], uninstall_blocked: "Pinned" }, glib] };
    renderInstalled();
    expect((await findRow("jq")).querySelector("[data-status-column]")).toHaveTextContent("Pinned");
    expect((await findRow("glib")).querySelector("[data-status-column]")?.childElementCount).toBe(0);
  });

  it("says 「已停用」 once on a package Homebrew disabled that has a newer version held back", async () => {
    // Homebrew's own line ("Disabled ... won't provide more updates")
    // already says why no update comes, so the held-back update
    // (`UpdateBlocked::Disabled`) adds no second "Disabled".
    const jqKey = snapshot.artifacts[0].key;
    served = {
      ...snapshot,
      artifacts: [
        {
          ...snapshot.artifacts[0],
          facts: {
            ...NO_FACTS,
            homebrew: {
              deprecated: null,
              disabled: { date: null, reason: null, replacement: null },
              caveats: null,
              other_versions: [],
            },
          },
        },
      ],
      updates: [
        {
          key: jqKey,
          current: snapshot.artifacts[0].version,
          target: "9.9",
          channel: "Native",
          checkable: true,
          warnings: [],
          blocked: "Disabled",
        },
      ],
    };
    const { queryAllByRole } = renderInstalled();
    await findRow("jq");
    expect(queryAllByRole("button", { name: /^Update/ })).toHaveLength(0);
    expect(chipsOf(await findRow("jq"))).toEqual(["Disabled"]);
    // The details' status list leaves Homebrew's mark to its own group, so
    // with no second chip it lists nothing.
    expect(await drawerChips("jq")).toEqual([]);
  });

  it("promises nothing about when Uninstall comes back on a silent source's pinned row", async () => {
    // A row carried forward from a Homebrew that did not answer has no
    // Uninstall button until Homebrew answers a check again, pinned or
    // not. So the pin's sentence says what stands in the way and what
    // removes it, and nothing about when the button returns.
    served = {
      ...snapshot,
      instances: [{ ...brew, status: { unavailable: "NotResponding", notes: [] } }],
      artifacts: [{ ...snapshot.artifacts[0], uninstall_blocked: "Pinned" }],
      updates: [],
    };
    const { queryAllByRole } = renderInstalled();

    const jq = await findRow("jq");
    expect(queryAllByRole("button", { name: ANY_UNINSTALL })).toHaveLength(0);
    const detail = chipDetail(jq, "Pinned");
    expect(detail).toHaveTextContent(
      "It's pinned in Homebrew. To uninstall it, first run /opt/homebrew/bin/brew unpin jq in Terminal.",
    );
    expect(detail.textContent).not.toMatch(/next time|at the latest|answers/);
  });

  it("offers no Uninstall on a tool with no safe uninstall method, and says so behind its chip without a command", async () => {
    // `UninstallBlocked::NoSafeMethod` (phase 4): the tool has no
    // uninstall command and Banager has no safe way yet to remove its
    // files, so the row hides the button and its chip says why -- and,
    // unlike a pin, sets no command as code, because there is nothing to
    // run first. `Session::issue_plan` refuses it in Rust too.
    served = {
      ...snapshot,
      instances: [claudeInstance],
      artifacts: [{ ...claudeArtifact, uninstall_blocked: "NoSafeMethod" }],
      updates: [],
    };
    const { queryAllByRole, container } = renderInstalled();

    const claude = await findRow("Claude Code");
    expect(queryAllByRole("button", { name: ANY_UNINSTALL })).toHaveLength(0);
    expect(chipDetail(claude, "Manual uninstall")).toHaveTextContent(
      "Claude Code has no uninstall command, and its files can't yet be removed safely from here. Follow Claude Code's official documentation to uninstall it.",
    );
    expect(container.querySelector("code")).toBeNull();
  });

  it("offers no Uninstall on a uv tool while UV_TOOL_DIR is set, and says why behind its chip", async () => {
    // `UninstallBlocked::UvToolDirSet` (uv's inventory, on every uv tool):
    // with UV_TOOL_DIR set, `uv tool uninstall` of the last tool also
    // deletes the folder above the tools folder when that holds no other
    // folder, so no uv tool's row offers Uninstall -- whether or not it is
    // the last -- and its chip says why, with nothing set apart to copy.
    // `Session::issue_plan` and uv's own plan refuse it in Rust too.
    const uv: ManagerInstance = {
      ...brew,
      id: "uv",
      adapter_id: "uv",
      exe_path: "/opt/homebrew/bin/uv",
      prefix: "/opt/homebrew/bin",
      version: "0.12.17",
    };
    served = {
      ...snapshot,
      instances: [uv],
      artifacts: [
        formula("ruff", {
          key: { instance_id: "uv", kind: "Tool", name: "ruff" },
          uninstall_blocked: "UvToolDirSet",
          facts: NO_FACTS,
        }),
      ],
      updates: [],
    };
    const { queryAllByRole, container } = renderInstalled();

    const ruff = await findRow("ruff");
    expect(queryAllByRole("button", { name: ANY_UNINSTALL })).toHaveLength(0);
    expect(chipDetail(ruff, "Can't uninstall here")).toHaveTextContent(
      "With UV_TOOL_DIR set, when uv uninstalls its last tool it also deletes the folder above UV_TOOL_DIR and everything in it, if that folder holds no other folder. So no uv tool can be uninstalled here while it's set.",
    );
    expect(container.querySelector("code")).toBeNull();
  });

  it("offers no Uninstall on npm's own npm, and says why behind its chip, while its other packages keep theirs", async () => {
    // `UninstallBlocked::SourceProgram` (npm's inventory, on its own `npm`):
    // `npm uninstall -g npm` would take the npm every other package is
    // updated and uninstalled with. `Session::issue_plan` and npm's own
    // plan refuse it in Rust too.
    const npm: ManagerInstance = {
      ...brew,
      id: "npm:/opt/homebrew",
      adapter_id: "npm",
      exe_path: "/opt/homebrew/bin/npm",
      version: "12.0.2",
    };
    const npmRow = (name: string, blocked: InstalledArtifact["uninstall_blocked"]) =>
      formula(name, { key: { instance_id: npm.id, kind: "Package", name }, uninstall_blocked: blocked, facts: NO_FACTS });
    served = {
      ...snapshot,
      instances: [npm],
      artifacts: [npmRow("npm", "SourceProgram"), npmRow("prettier", null)],
      updates: [],
    };
    const { queryAllByRole, container } = renderInstalled();

    const own = await findRow("npm");
    expect(queryAllByRole("button", { name: ANY_UNINSTALL })).toHaveLength(1);
    expect(within(rowOf("prettier")).getByRole("button", { name: ANY_UNINSTALL })).toBeInTheDocument();
    expect(chipDetail(own, "Can't uninstall here")).toHaveTextContent(
      "This is npm itself. Every tool installed with npm is updated and uninstalled with it, so it can't be uninstalled here.",
    );
    expect(container.querySelector("code")).toBeNull();
  });

  it("shows the standalone summary beside its chip, and a Homebrew package with no description what Homebrew says it is", async () => {
    // A standalone artifact carries `description: null` (the line has to
    // be localised, so its key lives in `STANDALONE_SUMMARY_KEYS`); a
    // Homebrew package with no description says what Homebrew says it is.
    served = {
      ...snapshot,
      instances: [brew, claudeInstance],
      artifacts: [
        { ...snapshot.artifacts[0], description: null },
        { ...claudeArtifact, uninstall_blocked: "NoSafeMethod" },
      ],
      updates: [],
    };
    const { queryAllByRole, queryByText } = renderInstalled();

    const claude = await findRow("Claude Code");
    expect(within(claude).getByText("Anthropic's AI coding assistant")).toBeInTheDocument();
    expect(chipsOf(claude)).toContain("Manual uninstall");
    expect(within(rowOf("jq")).getByText("Homebrew package")).toBeInTheDocument();
    expect(queryByText(/No description/)).toBeNull();
    // Only the Homebrew artifact may offer Uninstall.
    expect(queryAllByRole("button", { name: ANY_UNINSTALL })).toHaveLength(1);
  });

  it("offers Uninstall on Claude Code, beside its summary, now that it has a path list", async () => {
    // Phase 4 step C: the standalone artifact carries no `uninstall_blocked`
    // once its recipe lists the paths to move, so the row shows its
    // summary and an Uninstall button like any other package's, and
    // `Session::issue_plan` lets the plan through (`blocked_uninstall` in
    // session/plans.rs refuses only an artifact that carries one).
    served = { ...snapshot, instances: [claudeInstance], artifacts: [claudeArtifact], updates: [] };
    const { getAllByRole, queryByText } = renderInstalled();

    const claude = await findRow("Claude Code");
    expect(within(claude).getByText("Anthropic's AI coding assistant")).toBeInTheDocument();
    expect(getAllByRole("button", { name: ANY_UNINSTALL })).toHaveLength(1);
    expect(queryByText("Manual uninstall")).toBeNull();
  });

  it("says what each row's source says it is when the source gave none, in both languages", async () => {
    // npm's and Ollama's inventories never carry a description, and some
    // casks have none: each row says what its source says it is -- an
    // app's cask is an app, a font's is not called one -- never "No
    // description".
    served = {
      ...snapshot,
      instances: [brew, { ...brew, id: "npm:/opt/homebrew", adapter_id: "npm" }, ollama],
      artifacts: [
        formula("iterm2", {
          key: { instance_id: "brew:/opt/homebrew", kind: "Cask", name: "iterm2" },
          display_name: "iTerm2",
          description: null,
          path: "/Applications/iTerm.app",
        }),
        formula("font-jetbrains-mono", {
          key: { instance_id: "brew:/opt/homebrew", kind: "Cask", name: "font-jetbrains-mono" },
          display_name: "JetBrains Mono",
          description: null,
        }),
        formula("prettier", {
          key: { instance_id: "npm:/opt/homebrew", kind: "Package", name: "prettier" },
          description: null,
        }),
        formula("llama3.2:3b", {
          key: { instance_id: OLLAMA, kind: "Model", name: "llama3.2:3b" },
          description: null,
        }),
      ],
      updates: [],
    };
    const { queryByText } = renderInstalled();

    expect(within(await findRow("iTerm2")).getByText("App installed with Homebrew")).toBeInTheDocument();
    expect(within(rowOf("JetBrains Mono")).getByText("Homebrew package")).toBeInTheDocument();
    expect(within(rowOf("prettier")).getByText("npm package")).toBeInTheDocument();
    expect(within(rowOf("llama3.2:3b")).getByText("Ollama model")).toBeInTheDocument();
    expect(queryByText("No description")).toBeNull();

    await act(async () => {
      await i18n.changeLanguage("zh-CN");
    });
    try {
      expect(within(rowOf("iTerm2")).getByText("用Homebrew安装的App")).toBeInTheDocument();
      expect(within(rowOf("prettier")).getByText("npm软件包")).toBeInTheDocument();
      expect(within(rowOf("llama3.2:3b")).getByText("Ollama模型")).toBeInTheDocument();
      expect(queryByText("暂无简介")).toBeNull();
    } finally {
      await act(async () => {
        await i18n.changeLanguage("en");
      });
    }
  });

  describe("the update chips", () => {
    // One snapshot with one package per reason the Updates page may list
    // an update and not offer it, plus the ones it does offer. The row
    // used to say "Update available" for every entry in `snapshot.updates`,
    // and then still for a source that did not answer (a stopped Ollama
    // whose update was carried forward), which has no Update button either.
    const artifact = (name: string, over: Partial<InstalledArtifact> = {}): InstalledArtifact =>
      formula(name, over);
    const update = (name: string, over: Partial<UpdateCandidate> = {}): UpdateCandidate => ({
      ...snapshot.updates[0],
      key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name },
      ...over,
    });
    // The registry digest a skipped model's row offered, which no page may
    // print.
    const DIGEST = "sha256:9f1c0b6d2e4a7c5b3d1f8a6e4c2b0d9f7e5c3a1b8d6f4e2c0a9b7d5f3e1c8a6b";
    const skippedModelKey = { instance_id: OLLAMA, kind: "Model", name: "skipped-model" } as const;
    // A Homebrew cask declared `version :latest`: every release of it is
    // offered as "latest", so no skip hides it.
    const latestCaskKey = { instance_id: "brew:/opt/homebrew", kind: "Cask", name: "chromium" } as const;
    const mixed: Snapshot = {
      ...snapshot,
      artifacts: [
        artifact("offered"),
        artifact("pinned-outdated", { uninstall_blocked: "Pinned" }),
        artifact("pipx-pinned", {
          key: { instance_id: "pipx", kind: "Tool", name: "pipx-pinned" },
        }),
        artifact("unchecked"),
        artifact("ignored"),
        artifact("skipped"),
        artifact("skipped-before"),
        artifact("pinned-current", { uninstall_blocked: "Pinned" }),
        artifact("current"),
        artifact("stopped-model", {
          key: { instance_id: OLLAMA, kind: "Model", name: "stopped-model" },
        }),
        artifact("skipped-model", { key: skippedModelKey, version: "5642e97495e1" }),
        artifact("chromium", { key: latestCaskKey, version: "latest" }),
      ],
      instances: [
        brew,
        { ...brew, id: "pipx", adapter_id: "pipx", exe_path: "/opt/homebrew/bin/pipx", prefix: "/Users/a/.local" },
        { ...ollama, status: { unavailable: "NotRunning", notes: [] } },
      ],
      updates: [
        update("offered"),
        update("pinned-outdated", { blocked: "Pinned" }),
        update("pipx-pinned", {
          key: { instance_id: "pipx", kind: "Tool", name: "pipx-pinned" },
          channel: "Registry",
          blocked: "Pinned",
        }),
        update("unchecked", { checkable: false, warnings: [{ Message: "timed out" }] }),
        update("ignored"),
        update("skipped"),
        update("skipped-before"),
        update("stopped-model", {
          key: { instance_id: OLLAMA, kind: "Model", name: "stopped-model" },
          channel: "Registry",
        }),
        update("skipped-model", {
          key: skippedModelKey,
          current: "5642e97495e1",
          target: DIGEST,
          channel: "Digest",
        }),
        update("chromium", { key: latestCaskKey, current: "latest", target: "latest" }),
      ],
    };
    const mixedSettings: Settings = {
      ...settings,
      ignored_updates: [{ instance_id: "brew:/opt/homebrew", kind: "Formula", name: "ignored" }],
      // `skipped` is skipped at the version its row offers (2.90.0);
      // `skipped-before` at a version its source no longer offers, so its
      // update is listed again. `chromium`'s skip of "latest" matches the
      // version its row offers and hides nothing all the same.
      skipped_versions: [
        {
          key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "skipped" },
          version: "2.90.0",
        },
        {
          key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "skipped-before" },
          version: "2.89.0",
        },
        { key: skippedModelKey, version: DIGEST },
        { key: latestCaskKey, version: "latest" },
      ],
    };

    beforeEach(() => {
      served = mixed;
      servedSettings = mixedSettings;
      // Twelve rows of 56px are taller than a 600px viewport, and these
      // tests read every row's chips.
      viewport = 1200;
    });

    it("says an update the Updates page offers with the version it moves to, and why every other row has none", async () => {
      const { container } = renderInstalled();
      await findRow("current");

      // An update to be had is a normal state: no word, the version column
      // says it (spec §3.4), as the Updates page's row does.
      expect(chipsOf(rowOf("offered"))).toEqual([]);
      expect(versionShown(rowOf("offered"))).toBe("2.88.3 → 2.90.0");
      expect(chipsOf(rowOf("pinned-outdated"))).toEqual(["Pinned"]);
      // Pinned, and a newer version is out: both said, the second by the version.
      expect(versionShown(rowOf("pinned-outdated"))).toBe("2.88.3 → 2.90.0");
      expect(chipsOf(rowOf("pipx-pinned"))).toEqual(["Pinned"]);
      expect(chipsOf(rowOf("unchecked"))).toEqual(["Can't check"]);
      // No version found to move to: the version installed.
      expect(versionShown(rowOf("unchecked"))).toBe("1.8.2");
      expect(chipsOf(rowOf("ignored"))).toEqual(["Reminders off"]);
      // Hidden, so the version installed, and no arrow.
      expect(versionShown(rowOf("ignored"))).toBe("1.8.2");
      // The version the skip is about, which is what brings the reminder
      // back once the source offers another.
      expect(chipsOf(rowOf("skipped"))).toEqual(["Skipped 2.90.0"]);
      expect(chipsOf(rowOf("skipped-before"))).toEqual([]);
      expect(versionShown(rowOf("skipped-before"))).toBe("2.88.3 → 2.90.0");
      // Pinned in Homebrew and up to date: still pinned, from the
      // inventory; up to date goes without saying.
      expect(chipsOf(rowOf("pinned-current"))).toEqual(["Pinned"]);
      expect(versionShown(rowOf("pinned-current"))).toBe("1.8.2");
      expect(chipsOf(rowOf("current"))).toEqual([]);
      // Its source is not running, so there is no Update button for it,
      // and its Uninstall waits too: the row says the second, the one its
      // own button is about.
      expect(chipsOf(rowOf("stopped-model"))).toEqual(["Can't uninstall now"]);
      // A model's skipped version is a digest, and no digest is printed.
      // Its Ollama is the stopped one too.
      expect(chipsOf(rowOf("skipped-model"))).toEqual(["Can't uninstall now"]);
      expect(container.textContent).not.toContain("sha256");
      // Not "Skipped latest": a skip of a cask every release of which is
      // offered as "latest" would never end, so it hides nothing.
      expect(chipsOf(rowOf("chromium"))).toEqual([]);
      expect(versionShown(rowOf("chromium"))).toBe("latest → latest");
      // Never "Update available" or "Up to date" on a row.
      expect(container.querySelector("[data-tool-row]")?.parentElement?.parentElement?.textContent).not.toMatch(
        /Update available|Up to date/,
      );
      expect(chipDetail(rowOf("ignored"), "Reminders off")).toHaveTextContent(
        "You won't be reminded about any update to this tool. Undo it in Settings.",
      );
      fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });

      // The inspector's 「状态」 says every word, each with its why behind
      // an ⓘ, and up to date in a word too -- no tick.
      expect(await drawerChips("offered")).toEqual(["Update available"]);
      expect(await drawerChips("pinned-current")).toEqual(["Pinned", "Up to date"]);
      expect(await drawerChips("current")).toEqual(["Up to date"]);
      expect(await drawerChips("skipped-model")).toEqual(["Can't uninstall now", "Skipped the new version"]);
      const drawer = await openDetails("stopped-model");
      expect([...(drawer.querySelector("[data-status-list]")?.children ?? [])].map((item) => item.firstElementChild?.textContent)).toEqual([
        "Can't uninstall now",
        "Can't update now",
      ]);
      expect(statusWhy(drawer, "Can't update now")).toHaveTextContent(
        "Ollama isn't running. Open it, then click Check Again.",
      );
      expect(drawer.querySelector(".text-success")).toBeNull();
    });

    it("agrees with the Updates page's buttons row for row", async () => {
      const installed = renderInstalled();
      await findRow("current");
      const badged: string[] = [];
      for (const name of mixed.artifacts.map((a) => a.display_name)) {
        if ((await drawerChips(name)).includes("Update available")) badged.push(name);
      }
      badged.sort();
      installed.unmount();

      const updates = renderWithProviders(<UpdatesPage />);
      await updates.findByText("offered");
      // Every row it lists on the page, the ones folded under "Can't update
      // here" too.
      fireEvent.click(updates.getByRole("button", { name: /^\d+ more can't be updated here$/ }));
      await updates.findByText("stopped-model", { selector: "[data-tool-row] p" });
      const rowNamed = (name: string) =>
        updates.queryByText(name, { selector: "[data-tool-row] p" })?.closest("[data-tool-row]") ?? null;
      const offered = mixed.updates
        .map((u) => u.key.name)
        .filter((name) => {
          const row = rowNamed(name);
          return row instanceof HTMLElement
            ? within(row).queryByRole("button", { name: /^Update (?!All$|Selected )/ }) !== null
            : false;
        });

      // The stopped source's update is listed there, with no button: it is
      // left out of `offered` for that, not for a missing row.
      expect(rowNamed("stopped-model")).not.toBeNull();
      // The two skipped at the version they offer are not listed there at
      // all; the one skipped at an older version is, with its button, and
      // so is the cask whose skip of "latest" hides nothing.
      expect(rowNamed("skipped")).toBeNull();
      expect(rowNamed("skipped-model")).toBeNull();
      expect(badged).toEqual(["chromium", "offered", "skipped-before"]);
      expect(offered.sort()).toEqual(badged);
    });

    it("undoes a skip in place: 取消跳过 beside 「已跳过」 saves the settings without it, and the row reads as any update", async () => {
      renderInstalled();
      const drawer = await openDetails("skipped");
      const status = drawer.querySelector("[data-status-list]") as HTMLElement;
      // Settings' own words, after the word and its ⓘ, small and grey.
      const undo = within(status).getByRole("button", { name: "Stop skipping 2.90.0 of skipped" });
      expect(undo).toHaveTextContent("Stop Skipping");
      expect(undo.className).toContain(BUTTON.small.grey);
      const item = undo.parentElement as HTMLElement;
      expect(item.firstElementChild).toHaveTextContent("Skipped 2.90.0");
      expect(within(item).getByRole("button", { name: "Details: Skipped 2.90.0" })).toBeInTheDocument();
      expect(item.lastElementChild).toBe(undo);
      // Nothing to update yet: the update is hidden.
      expect(within(drawer).queryByRole("button", { name: "Update" })).toBeNull();

      // Pressed from the keyboard: the focus on it.
      undo.focus();
      fireEvent.click(undo);

      await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("set_settings", expect.anything()));
      const saved = mockInvoke.mock.calls.filter(([cmd]) => cmd === "set_settings");
      expect(saved).toHaveLength(1);
      // Only that skip goes; the other skips and the never-remind stay.
      expect((saved[0][1] as { settings: Settings }).settings).toEqual({
        ...mixedSettings,
        skipped_versions: mixedSettings.skipped_versions.filter((skip) => skip.key.name !== "skipped"),
      });
      // The plain state: listed again, the version it moves to, Update --
      // which has the focus the button took with it.
      await waitFor(() =>
        expect([...status.children].map((item) => item.firstElementChild?.textContent)).toEqual(["Update available"]),
      );
      expect(within(drawer).queryByRole("button", { name: /Stop skipping/ })).toBeNull();
      const update = within(drawer).getByRole("button", { name: "Update" });
      expect(update).toHaveFocus();
      expect(chipsOf(rowOf("skipped"))).toEqual([]);
      expect(versionShown(rowOf("skipped"))).toBe("2.88.3 → 2.90.0");
    });

    it("undoes a never-remind in place: 恢复提醒 beside 「已关闭提醒」 saves the settings without it", async () => {
      renderInstalled();
      const drawer = await openDetails("ignored");
      const status = drawer.querySelector("[data-status-list]") as HTMLElement;
      const undo = within(status).getByRole("button", { name: "Remind me again about ignored" });
      expect(undo).toHaveTextContent("Remind Me Again");
      expect(undo.className).toContain(BUTTON.small.grey);

      fireEvent.click(undo);

      await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("set_settings", expect.anything()));
      const saved = mockInvoke.mock.calls.filter(([cmd]) => cmd === "set_settings");
      expect(saved).toHaveLength(1);
      expect((saved[0][1] as { settings: Settings }).settings).toEqual({ ...mixedSettings, ignored_updates: [] });
      await waitFor(() =>
        expect([...status.children].map((item) => item.firstElementChild?.textContent)).toEqual(["Update available"]),
      );
      expect(within(drawer).queryByRole("button", { name: /Remind me again/ })).toBeNull();
      expect(within(drawer).getByRole("button", { name: "Update" })).toBeInTheDocument();
      expect(chipsOf(rowOf("ignored"))).toEqual([]);
    });

    it("says a snoozed update is hidden until its date, and takes the snooze back in place", async () => {
      const until = Math.floor(Date.now() / 1000) + 20 * 24 * 60 * 60;
      const offeredKey = { instance_id: "brew:/opt/homebrew", kind: "Formula" as const, name: "offered" };
      servedSettings = { ...mixedSettings, snoozed_updates: [{ key: offeredKey, until }] };
      const date = new Intl.DateTimeFormat("en", { month: "short", day: "numeric" }).format(new Date(until * 1000));
      renderInstalled();
      await findRow("offered");
      expect(chipsOf(rowOf("offered"))).toEqual([`No reminders until ${date}`]);
      expect(versionShown(rowOf("offered"))).toBe("1.8.2");

      const drawer = await openDetails("offered");
      const status = drawer.querySelector("[data-status-list]") as HTMLElement;
      const undo = within(status).getByRole("button", { name: "Remind me again about offered" });
      fireEvent.click(undo);
      await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("set_settings", expect.anything()));
      const saved = mockInvoke.mock.calls.filter(([cmd]) => cmd === "set_settings");
      expect((saved[0][1] as { settings: Settings }).settings).toEqual({ ...mixedSettings, snoozed_updates: [] });
      await waitFor(() =>
        expect([...status.children].map((item) => item.firstElementChild?.textContent)).toEqual(["Update available"]),
      );
    });

    it("offers no way back where nothing was hidden, and says in Chinese what Settings says", async () => {
      await i18n.changeLanguage("zh-CN");
      // `openDetails`, in Chinese: the row's button is 「详情：…」.
      const openDetails = async (name: string) => {
        const row = await findRow(name);
        fireEvent.click(within(row).getByRole("button", { name: `详情：${name}` }));
        return screen.findByRole("complementary", { name });
      };
      try {
        renderInstalled();
        const skipped = await openDetails("skipped");
        expect(within(skipped).getByRole("button", { name: "取消跳过“skipped”的2.90.0" })).toHaveTextContent("取消跳过");
        fireEvent.keyDown(skipped, { key: "Escape" });
        await waitFor(() => expect(screen.queryByRole("complementary")).toBeNull());
        const ignored = await openDetails("ignored");
        expect(within(ignored).getByRole("button", { name: "恢复“ignored”的更新提醒" })).toHaveTextContent("恢复提醒");
        fireEvent.keyDown(ignored, { key: "Escape" });
        await waitFor(() => expect(screen.queryByRole("complementary")).toBeNull());
        // A model skipped at its new build: the version is a digest, never said.
        const model = await openDetails("skipped-model");
        expect(within(model).getByRole("button", { name: "取消跳过“skipped-model”的新版本" })).toBeInTheDocument();
        fireEvent.keyDown(model, { key: "Escape" });
        await waitFor(() => expect(screen.queryByRole("complementary")).toBeNull());
        const offered = await openDetails("offered");
        expect(within(offered).queryByRole("button", { name: /取消跳过|恢复/ })).toBeNull();
      } finally {
        await i18n.changeLanguage("en");
      }
    });

    it("says in the inspector why a way back could not be saved, and keeps the word", async () => {
      // Why in the backend's words: "Show technical details" is on.
      servedSettings = { ...servedSettings, show_technical_details: true };
      const answer = mockInvoke.getMockImplementation();
      mockInvoke.mockImplementation((cmd: string, args?: InvokeArgs) =>
        cmd === "set_settings" ? Promise.reject("settings.json is read-only") : answer!(cmd, args),
      );
      renderInstalled();
      const drawer = await openDetails("skipped");
      fireEvent.click(within(drawer).getByRole("button", { name: "Stop skipping 2.90.0 of skipped" }));

      expect(await within(drawer).findByRole("alert")).toHaveTextContent(
        "Couldn't save that choice: settings.json is read-only.",
      );
      const status = drawer.querySelector("[data-status-list]") as HTMLElement;
      expect([...status.children].map((item) => item.firstElementChild?.textContent)).toEqual(["Skipped 2.90.0"]);
      expect(within(drawer).getByRole("button", { name: "Stop skipping 2.90.0 of skipped" })).toBeInTheDocument();
      // Only this tool's inspector says it.
      fireEvent.keyDown(drawer, { key: "Escape" });
      await waitFor(() => expect(screen.queryByRole("complementary")).toBeNull());
      const other = await openDetails("ignored");
      expect(within(other).queryByRole("alert")).toBeNull();
    });

    it("says a way back could not be saved, and what to do, without the backend's words while technical details are off", async () => {
      const answer = mockInvoke.getMockImplementation();
      mockInvoke.mockImplementation((cmd: string, args?: InvokeArgs) =>
        cmd === "set_settings" ? Promise.reject("settings.json is read-only") : answer!(cmd, args),
      );
      renderInstalled();
      const drawer = await openDetails("skipped");
      fireEvent.click(within(drawer).getByRole("button", { name: "Stop skipping 2.90.0 of skipped" }));

      expect(await within(drawer).findByRole("alert")).toHaveTextContent("Couldn't save that choice. Try again later.");
      expect(within(drawer).queryByText(/settings\.json is read-only/)).toBeNull();
    });
  });

  describe("up to date (T1)", () => {
    // 「已是最新」 says this check found no newer version. A source that did
    // not answer, one whose check failed, and a Homebrew still updating its
    // list or unable to have kept last round's rows and updates, which
    // nobody checked this time: saying up to date over those is the lie
    // the Updates page stopped telling.
    const pipx: ManagerInstance = {
      ...brew,
      id: "pipx",
      adapter_id: "pipx",
      exe_path: "/opt/homebrew/bin/pipx",
      prefix: "/opt/homebrew/bin",
    };
    const cargo: ManagerInstance = {
      ...brew,
      id: "cargo:/Users/a/.cargo",
      adapter_id: "cargo",
      exe_path: "/Users/a/.cargo/bin/cargo",
      prefix: "/Users/a/.cargo",
    };
    const on = (instance: ManagerInstance, name: string): InstalledArtifact =>
      formula(name, { key: { instance_id: instance.id, kind: "Tool", name } });

    it("says it only where the row's source answered this check in full", async () => {
      served = {
        ...snapshot,
        instances: [
          brew,
          { ...pipx, status: { unavailable: "NotResponding", notes: [] } },
          cargo,
          { ...ollama, status: { unavailable: "NotRunning", notes: [] } },
        ],
        artifacts: [on(brew, "answered"), on(pipx, "silent"), on(cargo, "failed"), on(ollama, "stopped")],
        updates: [],
        stale: true,
        // Cargo answered, but reading what it has installed failed this
        // round: its rows are last round's.
        errors: [{ instance_id: cargo.id, message: "could not read ~/.cargo/.crates2.json" }],
      };
      renderInstalled();

      // A row does not say it -- up to date goes without saying (spec
      // §3.4) -- but its details do, where it is so.
      expect(chipsOf(await findRow("answered"))).toEqual([]);
      expect(await drawerChips("answered")).toEqual(["Up to date"]);
      // Nothing about updates; the two whose source did not answer say
      // only that their Uninstall waits.
      expect(chipsOf(rowOf("failed"))).toEqual([]);
      expect(await drawerChips("failed")).toEqual([]);
      for (const name of ["silent", "stopped"]) {
        expect(chipsOf(rowOf(name)), name).toEqual(["Can't uninstall now"]);
        expect(await drawerChips(name), name).toEqual(["Can't uninstall now"]);
      }
      // Cargo's check that did not finish is the list's first line; the
      // two that did not answer say so in their own lines, folded behind
      // it (`SourceNotices`).
      expect(screen.getByText("Some checks didn't finish")).toBeInTheDocument();
      expect(screen.queryByText("pipx isn't responding")).toBeNull();
      fireEvent.click(screen.getByRole("button", { name: "2 more notes" }));
      expect(screen.getByText("pipx isn't responding")).toBeInTheDocument();
      expect(screen.getByText("Ollama isn't running")).toBeInTheDocument();
    });

    it.each([
      // Its only chip is about its Uninstall, held while the list updates.
      ["IndexUpdating", "Homebrew is checking online for new versions", ["Can't uninstall now"]],
      ["IndexMayBeStale", "Couldn't reach Homebrew, so updates for its tools weren't fully checked", []],
    ] as const)("says nothing about updates while Homebrew's list is %s, and the line says why", async (note, line, chips) => {
      served = { ...snapshot, instances: [{ ...brew, status: { unavailable: null, notes: [note] } }], updates: [] };
      const { queryByText } = renderInstalled();

      expect(chipsOf(await findRow("jq"))).toEqual(chips);
      expect(await drawerChips("jq")).toEqual(chips);
      expect(queryByText("Up to date")).toBeNull();
      expect(screen.getByText(line)).toBeInTheDocument();
    });

    it("selects a launcher left without its program from its notice's Show, the search cleared, and leaves Show out of its inspector", async () => {
      served = {
        ...snapshot,
        instances: [brew, { ...claudeInstance, status: { unavailable: null, notes: ["LauncherOnly"] } }],
        artifacts: [...snapshot.artifacts, { ...claudeArtifact, version: "", path: null }],
        updates: [],
      };
      // A search that hides its row: Show brings it back into the list.
      useUiStore.setState({ query: "jq" });
      renderInstalled();

      await findRow("jq");
      expect(screen.queryByText("Claude Code", { selector: "[data-tool-row] p" })).toBeNull();
      // The list's first line: the notice, whose one button is Show.
      const title = screen.getByText("Claude Code's program files are missing");
      const line = title.closest("[data-notice-line]") as HTMLElement;
      expect(within(line).queryByRole("button", { name: "Check Again" })).toBeNull();
      fireEvent.click(within(line).getByRole("button", { name: "Show Tool" }));

      // Selected, its inspector open, the focus on its row.
      const inspector = await screen.findByRole("complementary", { name: "Claude Code" });
      expect(useUiStore.getState().query).toBe("");
      expect(useUiStore.getState().inspectRequested).toBeNull();
      const row = rowOf("Claude Code");
      expect(row).toHaveAttribute("data-selected");
      await waitFor(() => expect(document.activeElement).toBe(row));
      // Its Uninstall… there, and the notice once more under it -- without
      // a Show, which would show what the inspector shows already.
      expect(within(inspector).getByRole("button", { name: "Uninstall…" })).toBeEnabled();
      expect(within(inspector).getByText("Claude Code's program files are missing")).toBeInTheDocument();
      expect(within(inspector).queryByRole("button", { name: "Show Tool" })).toBeNull();
    });

    it("says nothing about updates for a launcher left without its program: there was no version to check", async () => {
      served = {
        ...snapshot,
        instances: [{ ...claudeInstance, status: { unavailable: null, notes: ["LauncherOnly"] } }],
        artifacts: [{ ...claudeArtifact, version: "", path: null }],
        updates: [],
      };
      renderInstalled();

      expect(chipsOf(await findRow("Claude Code"))).toEqual([]);
      expect(await drawerChips("Claude Code")).toEqual([]);
      expect(screen.getByText("Claude Code's program files are missing")).toBeInTheDocument();
    });

    // Without --greedy, `brew outdated` leaves out a cask that updates
    // itself and one declared `version :latest`, so no update listed for
    // either is no news.
    it.each([
      [false, []],
      [true, ["Up to date"]],
    ] as const)(
      "with Show Homebrew apps that have their own updater %s, says it over a self-updating or always-latest cask only when Homebrew checked it",
      async (includeSelfUpdating, leftOut) => {
        const cask = (name: string, over: Partial<InstalledArtifact> = {}) =>
          formula(name, { key: { instance_id: brew.id, kind: "Cask", name }, ...over });
        servedSettings = { ...settings, include_self_updating: includeSelfUpdating };
        served = {
          ...snapshot,
          artifacts: [cask("onyx"), cask("zoom", { auto_updates: true }), cask("chromium", { version: "latest" })],
          updates: [],
        };
        renderInstalled();

        // Never on the rows; in the details, where Homebrew checked it.
        for (const name of ["onyx", "zoom", "chromium"]) expect(chipsOf(await findRow(name)), name).toEqual([]);
        expect(await drawerChips("onyx")).toEqual(["Up to date"]);
        expect(await drawerChips("zoom")).toEqual(leftOut);
        expect(await drawerChips("chromium")).toEqual(leftOut);
      },
    );
  });

  it("marks a read-only source's rows View only, offers no Uninstall on them, and keeps pip's way out behind the chip", async () => {
    served = pipSnapshot;
    const { queryByRole, queryAllByText } = renderInstalled();

    const requests = await findRow("requests");
    expect(queryByRole("button", { name: ANY_UNINSTALL })).not.toBeInTheDocument();
    expect(chipsOf(requests)).toEqual(["View only"]);
    expect(chipDetail(requests, "View only")).toHaveTextContent(
      "You can only view pip installs here. If one of them is a command-line tool you use in Terminal, reinstall it with pipx or uv to update and uninstall it here.",
    );
    // Its row says it; no line of its own at the top: the words are on
    // its chip and nowhere else.
    expect(queryAllByText("View only").filter((text) => text.closest("button") === null)).toEqual([]);
  });

  it("gives a root-owned npm prefix's rows npm's own way out, and no Uninstall", async () => {
    // Read-only, like pip, but for a reason pip's copy would misdescribe:
    // the tool can install and uninstall perfectly well, it just cannot
    // write where this machine put it. The fix is a Node from Homebrew,
    // and only for what is installed with it (T5); telling this user about
    // pipx or uv is noise.
    served = {
      ...snapshot,
      instances: [
        {
          ...brew,
          id: "npm:/usr/local",
          adapter_id: "npm",
          exe_path: "/usr/local/bin/npm",
          prefix: "/usr/local",
          version: "12.0.2",
          read_only_reason: "PrefixNotWritable",
        },
      ],
      artifacts: [
        formula("typescript", {
          key: { instance_id: "npm:/usr/local", kind: "Package", name: "typescript" },
          description: "TypeScript is a language for application scale JavaScript development",
        }),
      ],
      updates: [],
    };
    const { queryAllByRole } = renderInstalled();

    const detail = chipDetail(await findRow("typescript"), "View only");
    expect(detail).toHaveTextContent(
      "npm keeps these in a folder your account can't change, so you can only view them. After you install Node with Homebrew, you can manage the npm packages you install with it here.",
    );
    expect(detail.textContent).not.toMatch(/pipx|uv/);
    expect(queryAllByRole("button", { name: ANY_UNINSTALL })).toHaveLength(0);
  });

  it("lets each slot measure itself, so a heading or a row is never overlapped by the one below it", async () => {
    // A slot's real height is only known once it is drawn: a heading, a
    // row, a row whose chip wraps. jsdom has no layout engine, so the
    // height comes from the mock above; what this test checks is that the
    // virtualizer *reads* it. The next slot's offset follows the measured
    // size, and no slot carries a fixed inline height -- with one, a taller
    // slot would overflow and the next one, later in DOM order and so
    // painted on top, would cover its tail.
    rowHeights[0] = 128;
    const { container } = renderInstalled();

    await findRow("jq");
    const slotAt = (index: number) => container.querySelector<HTMLElement>(`[data-index="${index}"]`);

    await waitFor(() => expect(slotAt(1)?.style.transform).toBe("translateY(128px)"));
    expect(slotAt(0)?.style.height).toBe("");
    expect(slotAt(1)?.style.height).toBe("");
  });

  it("names a silent source in the list's first line, and says it can't show what it has when it has no rows", async () => {
    // brew, npm, uv, pipx and cargo can all report `NotResponding`,
    // and it means the same thing for all five: the CLI is on PATH but
    // Banager could not talk to it. The backend keeps such an instance in
    // `snapshot.instances` precisely so the UI can say so -- it pushes no
    // error, so this line is the only place the user can learn that their
    // global npm packages are missing from the list rather than gone.
    served = {
      ...snapshot,
      instances: [
        brew,
        {
          ...brew,
          id: "npm:/opt/homebrew/lib",
          adapter_id: "npm",
          exe_path: "/opt/homebrew/bin/npm",
          prefix: "/opt/homebrew/lib",
          version: "11.2.0",
          status: { unavailable: "NotResponding", notes: [] },
        },
      ],
    };
    const { findByText, getByRole, queryByText } = renderInstalled();

    await findByText("jq");
    const line = await findByText("npm isn't responding");
    // The list's first line, which scrolls away with it (spec §3.8), 20
    // in as the rows' content is -- not a band over the list.
    const slot = line.closest("[data-index]") as HTMLElement;
    expect(slot).toHaveAttribute("data-index", "0");
    expect(slot.firstElementChild?.className).toBe("px-5");
    expect(slot.nextElementSibling?.querySelector("[data-tool-row]")).not.toBeNull();
    const details = getByRole("button", { name: "Details: npm isn't responding" });
    fireEvent.click(details);
    // Nothing was carried forward for npm -- and nothing ever is on the
    // first refresh after a launch, because the snapshot is in memory
    // only (`Session::new` starts from `Snapshot::empty()`).
    expect(document.getElementById(details.getAttribute("aria-controls") ?? "")).toHaveTextContent(
      "The tools installed with it can't be listed this time.",
    );
    expect(queryByText(/What's listed/)).not.toBeInTheDocument();
    // And no promise of a recovery that may never come.
    expect(queryByText(/Reopening Banager/)).not.toBeInTheDocument();
  });

  it("says what is listed for a silent source is from the last time it responded, when it has rows, a search or not", async () => {
    // `refresh` keeps an unavailable source's last known artifacts, so
    // once there has been a good refresh these rows are real and the user
    // needs telling how old they are. A search that hides them does not
    // make the source have none.
    served = {
      ...snapshot,
      instances: [{ ...brew, status: { unavailable: "NotResponding", notes: [] } }],
    };
    const { getByRole } = renderInstalled();

    await findRow("jq");
    fireEvent.change(getByRole("searchbox", { name: "Search installed tools" }), {
      target: { value: "nothing-like-it" },
    });
    const details = await screen.findByRole("button", { name: "Details: Homebrew isn't responding" });
    fireEvent.click(details);
    // How many tools Homebrew has installed, and that they are its last
    // answer (W2-10): the same number whatever the search leaves listed --
    // here none -- so the sentence claims nothing about what is on screen.
    const sentence =
      "2 tools were installed with Homebrew. It didn't respond this time, so they're shown as they were when it last responded. Check again later.";
    expect(rowNames()).toEqual([]);
    expect(document.getElementById(details.getAttribute("aria-controls") ?? "")).toHaveTextContent(sentence);
    expect(document.getElementById(details.getAttribute("aria-controls") ?? "")?.textContent).not.toMatch(/listed/);
    // Its next step's button on its own line, after its ⓘ, which checks again.
    const line = details.closest("[data-notice-line]") as HTMLElement;
    const again = within(line).getByRole("button", { name: "Check Again" });
    expect(again).toBeEnabled();
    expect(again.className).toContain(BUTTON.small.grey);
    mockInvoke.mockClear();
    fireEvent.click(again);
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("refresh"));

    // The 「显示」 popup on AI tools, none of which is Homebrew's: the same
    // number again.
    fireEvent.change(getByRole("searchbox", { name: "Search installed tools" }), { target: { value: "" } });
    act(() => useUiStore.getState().setInstalledShow("ai"));
    await waitFor(() => expect(rowNames()).toEqual([]));
    const aiDetails = await screen.findByRole("button", { name: "Details: Homebrew isn't responding" });
    fireEvent.click(aiDetails);
    expect(document.getElementById(aiDetails.getAttribute("aria-controls") ?? "")).toHaveTextContent(sentence);
  });

  it("shows every row's version, and never a model's digest, technical details on or off", async () => {
    // An Ollama model's `version` is the local manifest digest, not a
    // version number. Printing it turned every model row into
    // "qwen3:8b · 5642e97495e1a0888838…", a 64-hex string shown to someone
    // who does not write code. Every other row shows its version now, as
    // the Updates page's rows do.
    served = {
      ...snapshot,
      instances: [brew, ollama],
      artifacts: [
        ...snapshot.artifacts,
        formula("qwen3:8b", {
          key: { instance_id: OLLAMA, kind: "Model", name: "qwen3:8b" },
          version: "5642e97495e1a0888838ee1b3b1a0b1c6a0f0f5e6c2d4a8b9e7c3d1f0a2b4c6d",
          description: null,
        }),
      ],
    };
    for (const technical of [false, true]) {
      servedSettings = { ...settings, show_technical_details: technical };
      const { unmount, queryByText } = renderInstalled();

      const jq = await findRow("jq");
      expect(within(jq).getByText("1.8.2").className).toContain("tabular-nums");
      await findRow("qwen3:8b");
      expect(queryByText(/5642e97495e1a0888838/)).not.toBeInTheDocument();
      unmount();
    }
  });

  it("names a model pulled from another registry by the model itself, where it is from on its line, and whole in its inspector", async () => {
    // Cut in its middle, the pretend Mac's model read
    // "modelscope.cn/Q…-GGUF:Q4_K_M": the host and the quantisation, the
    // model gone.
    served = {
      ...snapshot,
      instances: [brew, ollama],
      artifacts: [
        ...snapshot.artifacts,
        formula(MODELS.coder, {
          key: { instance_id: OLLAMA, kind: "Model", name: MODELS.coder },
          version: "52e05d4a30959ae2542932b2c473f476dca0ce371aaf9a2227badf4e3eeec4f4",
          description: null,
        }),
        formula(MODELS.llama, {
          key: { instance_id: OLLAMA, kind: "Model", name: MODELS.llama },
          version: "8e4cdead7463ce276b20d4e33341950d7bb40847f70a9882567a188e24ec1f66",
          description: null,
        }),
        // Between the registry's m and the model's Q.
        formula("node"),
      ],
    };
    renderInstalled();

    await findRow("jq");
    const name = document.querySelector(`[data-tool-row] p[title="${MODELS.coder}"]`) as HTMLElement;
    const row = name.closest("[data-tool-row]") as HTMLElement;
    const [shown, spoken] = [...name.children] as HTMLElement[];
    expect(shown.textContent).toBe("Qwen2.5-Coder-7B-Instruct-GGUF:Q4_K_M");
    expect(spoken.textContent).toBe(MODELS.coder);
    const line = row.querySelector("[data-description]") as HTMLElement;
    expect(line.textContent).toBe("modelscope.cn/Qwen · Ollama model");
    // A model named without a path is named as it is.
    expect(rowOf(MODELS.llama).querySelector("[data-description]")?.textContent).toBe("Ollama model");
    // And each is in the list where its name, as shown, puts it: Qwen
    // under Q, after node -- not under the registry's m, before it.
    const shownNames = [...document.querySelectorAll("[data-tool-row] p[title]")].map(
      (p) => p.firstElementChild?.getAttribute("aria-hidden") === "true" ? p.firstElementChild.textContent : p.textContent,
    );
    expect(shownNames).toEqual([...shownNames].sort((a, b) => (a ?? "").localeCompare(b ?? "", "en", { numeric: true, sensitivity: "base" })));
    expect(shownNames.indexOf("Qwen2.5-Coder-7B-Instruct-GGUF:Q4_K_M")).toBeGreaterThan(shownNames.indexOf("node"));

    // Its inspector, opened from the row, has it whole.
    fireEvent.click(within(row).getByRole("button", { name: `Details: ${MODELS.coder}` }));
    const inspector = await screen.findByRole("complementary", { name: MODELS.coder });
    expect(within(inspector).getByRole("heading", { level: 2 })).toHaveTextContent(MODELS.coder);
  });

  // Rendered through SnapshotStatus, exactly as App.tsx does. Rendering
  // InstalledPage on its own would bypass the gate the real app always goes
  // through, and this snapshot -- a stopped Ollama and nothing installed
  // anywhere -- is precisely the one that gate used to swallow.
  // r24 W2's skeptic: the list's first line, a source's notice, goes once
  // its own button has fixed what it said -- Check Again once the source
  // answers -- and with it the button the focus was on: to the window's
  // body, from where the next Tab starts over at the sidebar.
  it("puts the focus on the page's title once a notice's Check Again has done its work, not on the window's body", async () => {
    const silent = { ...ollama, status: { unavailable: "NotResponding" as const, notes: [] } };
    served = { ...snapshot, instances: [brew, silent] };
    const { getByRole, queryClient } = renderWithProviders(
      <WithToolbarSlot>
        <PageHeader title="Installed" actions={null} />
        <InstalledPage />
      </WithToolbarSlot>,
    );
    await findRow("jq");
    const line = (await screen.findByText("Ollama isn't responding")).closest("[data-list-slot]") as HTMLElement;
    const again = within(line).getByRole("button", { name: "Check Again" });
    act(() => again.focus());
    fireEvent.click(again);
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("refresh"));

    // The check: Ollama answers, and its line goes.
    act(() => {
      queryClient.setQueryData(queryKeys.snapshot, {
        ...served,
        generation: served.generation + 1,
        instances: [brew, ollama],
      });
    });

    await waitFor(() => expect(again.isConnected).toBe(false));
    await waitFor(() => expect(document.activeElement).toBe(getByRole("heading", { name: "Installed" })));
  });

  it("shows a not-running line with an Open Ollama button when the daemon is not running", async () => {
    served = {
      ...snapshot,
      instances: [{ ...ollama, version: null, status: { unavailable: "NotRunning", notes: [] } }],
      artifacts: [],
      updates: [],
    };
    const { findByText, getByRole } = renderWithProviders(
      <SnapshotStatus>
        <InstalledPage />
      </SnapshotStatus>,
    );

    await findByText("Ollama isn't running");
    // Nothing is installed anywhere else, and the page says so under it.
    expect(await findByText("No installed tools found")).toBeInTheDocument();
    fireEvent.click(getByRole("button", { name: "Open Ollama" }));

    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("open_ollama_app"));
  });

  it("says why Open Ollama did nothing when there is no Ollama app to open", async () => {
    // The button used to be able to fail in silence: the backend never
    // read `open`'s exit status, and even once it did, its structured
    // rejection had nothing on this side to turn it into words. A snapshot
    // taken before the app was removed still shows the button, so this is
    // the path a real person can reach.
    served = {
      ...snapshot,
      instances: [{ ...ollama, version: null, status: { unavailable: "NotRunning", notes: [] } }],
      artifacts: [],
      updates: [],
    };
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_snapshot") return Promise.resolve(served);
      if (cmd === "get_settings") return Promise.resolve(settings);
      if (cmd === "open_ollama_app")
        return Promise.reject('{"kind":"ollama_open_failed","reason":"not_installed"}');
      return Promise.resolve(undefined);
    });

    const { findByText, getByRole, queryByText } = renderWithProviders(
      <SnapshotStatus>
        <InstalledPage />
      </SnapshotStatus>,
    );

    await findByText("Ollama isn't running");
    fireEvent.click(getByRole("button", { name: "Open Ollama" }));

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent(/There's no Ollama app in Applications/);
    // Why Homebrew's ollama has none is the failure's Details.
    const details = within(alert).getByRole("button", { name: /^Details: There's no Ollama app/ });
    fireEvent.click(details);
    expect(document.getElementById(details.getAttribute("aria-controls") ?? "")).toHaveTextContent(
      "The ollama command from Homebrew doesn't include the app.",
    );
    expect(queryByText(/ollama_open_failed/)).not.toBeInTheDocument();
  });

  it("keeps a stopped source's rows on screen with Uninstall disabled, and says why behind a chip", async () => {
    // `refresh` carries an unavailable source's last known artifacts
    // forward, which is what makes the line's "showing what Ollama
    // reported last time" true instead of a sentence over no rows. `ollama
    // rm` against a daemon that is not listening cannot succeed -- spec
    // §2.5's conjunction, on the button rather than only in
    // the backend's refusal -- so each row's Uninstall stays, disabled, as
    // it does while Homebrew updates its list, with the same chip saying
    // why and what to do.
    served = {
      ...snapshot,
      generation: 4,
      instances: [{ ...ollama, status: { unavailable: "NotRunning", notes: [] } }],
      artifacts: [
        formula("qwen3:8b", {
          key: { instance_id: OLLAMA, kind: "Model", name: "qwen3:8b" },
          version: "5642e97495e1",
          description: null,
        }),
      ],
      updates: [],
      stale: true,
    };
    const { findByText, queryByRole } = renderWithProviders(
      <SnapshotStatus>
        <InstalledPage />
      </SnapshotStatus>,
    );

    await findByText("Ollama isn't running");
    const model = await findRow("qwen3:8b");
    const held = within(model).getByRole("button", { name: ROW_UNINSTALL });
    expect(held).toBeDisabled();
    fireEvent.click(held);
    expect(queryByRole("alertdialog")).toBeNull();
    expect(mockInvoke).not.toHaveBeenCalledWith("plan_operation", expect.anything());
    expect(chipsOf(model)).toEqual(["Can't uninstall now"]);
    expect(chipDetail(model, "Can't uninstall now")).toHaveTextContent(
      "Ollama isn't running. Open it, then click Check Again.",
    );
  });

  it("holds a silent source's Uninstall the same way, and says to check again later", async () => {
    // pre-commit, from a uv that did not answer: the row used to show a
    // name and a line and nothing else, beside a Homebrew row whose
    // disabled Uninstall said why.
    const uv: ManagerInstance = {
      ...brew,
      id: "uv:/Users/someone/.local/share/uv/tools",
      adapter_id: "uv",
      exe_path: "/opt/homebrew/bin/uv",
      prefix: "/Users/someone/.local/share/uv/tools",
      status: { unavailable: "NotResponding", notes: [] },
    };
    served = {
      ...snapshot,
      instances: [uv],
      artifacts: [formula("pre-commit", { key: { instance_id: uv.id, kind: "Tool", name: "pre-commit" } })],
      updates: [],
    };
    renderInstalled();

    const row = await findRow("pre-commit");
    expect(within(row).getByRole("button", { name: ROW_UNINSTALL })).toBeDisabled();
    expect(chipDetail(row, "Can't uninstall now")).toHaveTextContent(
      "uv isn't responding. Click Check Again later.",
    );
    fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });

    // The inspector says the same: Uninstall disabled, and why behind its
    // status word's ⓘ.
    const drawer = await openDetails("pre-commit");
    expect(within(drawer).getByRole("button", { name: "Uninstall…" })).toBeDisabled();
    expect(statusWhy(drawer, "Can't uninstall now")).toHaveTextContent("uv isn't responding. Click Check Again later.");
  });

  it("keeps Homebrew's Uninstall disabled while it updates its list, says why behind a chip, and gives it back after", async () => {
    // Homebrew's uninstall preview is refused while its list is being
    // rewritten (`AdapterError::IndexUpdating` in
    // crates/banager-core/src/adapters/brew/mod.rs), so the button stays,
    // disabled, rather than open a dialog that could only refuse. Another
    // source's Uninstall is untouched, and the core's own refresh clears
    // the note when the update ends.
    const updating: Snapshot = {
      ...snapshot,
      instances: [{ ...brew, status: { unavailable: null, notes: ["IndexUpdating"] } }, claudeInstance],
      artifacts: [snapshot.artifacts[0], claudeArtifact],
      updates: [],
    };
    served = updating;
    const { queryClient } = renderInstalled();

    const jq = await findRow("jq");
    const held = within(jq).getByRole("button", { name: ROW_UNINSTALL });
    expect(held).toBeDisabled();
    fireEvent.click(held);
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(mockInvoke).not.toHaveBeenCalledWith("plan_operation", expect.anything());
    expect(chipsOf(jq)).toEqual(["Can't uninstall now"]);
    expect(chipDetail(jq, "Can't uninstall now")).toHaveTextContent(
      "Homebrew is checking online for new versions. You can uninstall once it's done.",
    );
    // The word is the same on every row it holds: its button and the
    // row's Uninstall say whose, the words first; the row says the word.
    expect(within(jq).getByRole("button", { name: "Can't uninstall jq now" })).toHaveTextContent("Can't uninstall now");
    expect(held).toHaveAccessibleName("Uninstall jq…");
    expect(jq).toHaveAccessibleName("jq, Can't uninstall now");
    // The page's own line says what Homebrew is doing.
    expect(screen.getByText("Homebrew is checking online for new versions")).toBeInTheDocument();
    expect(within(rowOf("Claude Code")).getByRole("button", { name: ROW_UNINSTALL })).toBeEnabled();
    expect(chipsOf(rowOf("Claude Code"))).not.toContain("Can't uninstall now");

    // The inspector says the same: Uninstall disabled, and why behind its
    // status word's ⓘ.
    const drawer = await openDetails("jq");
    expect(within(drawer).getByRole("button", { name: "Uninstall…" })).toBeDisabled();
    expect(statusWhy(drawer, "Can't uninstall now")).toHaveTextContent(
      "Homebrew is checking online for new versions. You can uninstall once it's done.",
    );
    fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });
    fireEvent.click(within(drawer).getByRole("button", { name: "Close Details" }));
    await waitFor(() => expect(screen.queryByRole("complementary", { name: "jq" })).toBeNull());

    served = { ...updating, instances: [brew, claudeInstance] };
    await act(() => queryClient.invalidateQueries());
    await waitFor(() => expect(within(rowOf("jq")).getByRole("button", { name: ROW_UNINSTALL })).toBeEnabled());
    // Its word goes with the hold; up to date goes without saying.
    expect(chipsOf(rowOf("jq"))).toEqual([]);
    fireEvent.click(within(rowOf("jq")).getByRole("button", { name: ROW_UNINSTALL }));
    expect(await screen.findByRole("alertdialog", { name: "Uninstall “jq”?" })).toBeInTheDocument();
  });

  it("shows a tool's homepage as text while the first check's list is shown, and as a link once the check is done", async () => {
    // The backend opens only a homepage its committed snapshot lists
    // (src-tauri/src/homepage.rs), and the list shown while the first check
    // still checks for updates is not committed: a link there could only
    // say 「无法打开」. As Uninstall waits for the check, so does the link.
    served = { ...snapshot, generation: 0, round: 0, detect: "Missing", instances: [], artifacts: [], updates: [], refreshed_at: null };
    const { queryClient } = renderInstalled();
    act(() => writeInventoryPreview(queryClient, { round: 1, instances: [brew], artifacts: [snapshot.artifacts[0]] }));
    const drawer = await openDetails("jq");
    expect(within(drawer).queryByRole("link")).toBeNull();
    const shown = drawer.querySelector("[data-homepage]") as HTMLElement;
    expect(shown.textContent).toBe("jqlang.github.io");
    expect(shown).toHaveAttribute("title", "https://jqlang.github.io/jq/");
    fireEvent.click(shown);
    expect(mockInvoke).not.toHaveBeenCalledWith("open_homepage", expect.anything());
    expect(within(drawer).getByRole("button", { name: "Copy Link" })).toBeInTheDocument();

    served = snapshot;
    await act(() => queryClient.invalidateQueries());
    const link = await screen.findByRole("link", { name: "jqlang.github.io" });
    fireEvent.click(link);
    expect(mockInvoke).toHaveBeenCalledWith("open_homepage", { address: "https://jqlang.github.io/jq/" });
  });

  it("unfolds one source's components without unfolding another's, each line naming its source", async () => {
    // One global flag meant clicking pip's "N components" also unfolded
    // Homebrew's, on any Mac that has both. The line is per source, names
    // it where the list mixes sources, and folds back up again.
    served = {
      ...snapshot,
      instances: [brew, pip],
      artifacts: [
        ...snapshot.artifacts,
        ...pipSnapshot.artifacts,
        {
          ...pipSnapshot.artifacts[0],
          key: { instance_id: "pip:/usr/bin/python3", kind: "Package", name: "charset-normalizer" },
          display_name: "charset-normalizer",
          version: "3.4.0",
          reason: "Dependency",
          description: "The Real First Universal Charset Detector.",
        },
      ],
    };
    const { findByRole, getByRole, queryByText } = renderInstalled();

    await findRow("requests");
    expect(queryByText("glib")).not.toBeInTheDocument();
    expect(queryByText("charset-normalizer")).not.toBeInTheDocument();

    const pipFold = getByRole("button", { name: "1 more package was installed for other software to use pip" });
    expect(getByRole("button", { name: "1 more package was installed for other software to use Homebrew" })).toBeInTheDocument();
    fireEvent.click(pipFold);

    await findRow("charset-normalizer");
    expect(queryByText("glib")).not.toBeInTheDocument();

    fireEvent.click(await findByRole("button", { name: "Hide 1 package pip" }));
    await waitFor(() => expect(queryByText("charset-normalizer")).not.toBeInTheDocument());
  });

  describe("filters", () => {
    const twoSources = (): Snapshot => ({
      ...snapshot,
      // A stopped Ollama with nothing installed: its line, but no filter.
      instances: [brew, pip, { ...ollama, status: { unavailable: "NotRunning", notes: [] } }],
      artifacts: [...snapshot.artifacts, ...pipSnapshot.artifacts],
    });

    it("has no filter chips: the sidebar's rows choose the source, and the page shows it alone", async () => {
      served = twoSources();
      const { queryByRole, findByRole } = renderInstalled();

      await findRow("requests");
      // The sidebar's 「来源」 took the chips' place (spec §3.3, R8).
      expect(queryByRole("group", { name: "Filter by source" })).toBeNull();
      expect(document.querySelector("[aria-pressed='true']")).toBeNull();
      // Every source's rows, each naming its source where it is not its own:
      // to a screen reader, and in its avatar's tooltip -- in sight only
      // where two sources list the same name (R3).
      expect(rowNames()).toEqual(["jq", "requests"]);
      expect(within(rowOf("jq")).getByText("Homebrew")).toHaveClass("sr-only");
      expect(within(rowOf("jq")).getByTitle("Homebrew")).toBeInTheDocument();

      act(() => useUiStore.getState().openInstalled(pip.id));
      await waitFor(() => expect(rowNames()).toEqual(["requests"]));
      // One source: nothing in sight about it on its rows, and only its own lines.
      expect(within(rowOf("requests")).getByText("pip", { selector: "span" })).toHaveClass("sr-only");
      expect(screen.queryByText("Ollama isn't running")).toBeNull();

      act(() => useUiStore.getState().openInstalled(null));
      await waitFor(() => expect(rowNames()).toEqual(["jq", "requests"]));
      expect(await findByRole("button", { name: "Open Ollama" })).toBeInTheDocument();
    });

    it("opens on the source whatever opened the page asked for, as the sidebar's row does", async () => {
      served = twoSources();
      useUiStore.getState().openInstalled(brew.id);
      renderInstalled();

      await findRow("jq");
      expect(rowNames()).toEqual(["jq"]);
      expect(useUiStore.getState().page).toBe("installed");
    });

    it("stays on a source with nothing to list, and says why in its notice's words, with Check Again", async () => {
      // Opened from the sidebar's row for a stopped Ollama: never reset
      // to every source's list behind the user's back (spec R8).
      served = twoSources();
      useUiStore.getState().openInstalled(OLLAMA);
      const { findByText, getByText, getByRole } = renderInstalled();

      expect(await findByText("Ollama isn't running")).toBeInTheDocument();
      expect(getByText("Open Ollama to see what it has and check for updates.")).toBeInTheDocument();
      expect(rowNames()).toEqual([]);
      // Said once, where the list would be: no notice line over it too.
      expect(screen.getAllByText("Ollama isn't running")).toHaveLength(1);
      fireEvent.click(getByRole("button", { name: "Check Again" }));
      await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("refresh"));
      expect(useUiStore.getState().installedFilter).toBe(OLLAMA);
    });

    it("says under a silent source's name only what its title does not", async () => {
      // A Homebrew left in /usr/local by an Intel Mac, not answering: the
      // title names it and says it is not responding; the sentence under
      // it goes on from there, not 「Homebrew（Intel）没有响应」 twice.
      const intel: ManagerInstance = {
        ...brew,
        id: "brew:/usr/local",
        exe_path: "/usr/local/bin/brew",
        prefix: "/usr/local",
        status: { unavailable: "NotResponding", notes: [] },
      };
      served = { ...snapshot, instances: [brew, intel] };
      useUiStore.getState().openInstalled(intel.id);
      const { findByText, getByRole } = renderInstalled();

      const title = await findByText("Homebrew (Intel) isn't responding");
      const sentence = screen.getByText("The tools installed with it can't be listed this time. Check again later.");
      expect(sentence).not.toHaveTextContent(/isn't responding|didn't respond/);
      expect(title.compareDocumentPosition(sentence) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
      expect(getByRole("button", { name: "Check Again" })).toBeEnabled();
      expect(rowNames()).toEqual([]);

      await act(async () => {
        await i18n.changeLanguage("zh-CN");
      });
      try {
        expect(await findByText("Homebrew（Intel）没有响应")).toBeInTheDocument();
        const zh = screen.getByText("这次无法列出用它安装的工具。请稍后重新检查。");
        expect(zh).not.toHaveTextContent("没有响应");
      } finally {
        await act(async () => {
          await i18n.changeLanguage("en");
        });
      }
    });

    it("says when a silent source with nothing to list last answered, and that it had nothing then (R12)", async () => {
      vi.useFakeTimers({ toFake: ["Date"] });
      vi.setSystemTime(new Date(2026, 9, 2, 10, 0));
      const at = new Date(2026, 9, 1, 21, 40);
      const intel: ManagerInstance = {
        ...brew,
        id: "brew:/usr/local",
        exe_path: "/usr/local/bin/brew",
        prefix: "/usr/local",
        answered_at: at.getTime() / 1000,
        status: { unavailable: "NotResponding", notes: [] },
      };
      served = { ...snapshot, instances: [brew, intel] };
      useUiStore.getState().openInstalled(intel.id);
      try {
        const { findByText } = renderInstalled();
        await findByText("Homebrew (Intel) isn't responding");
        const nine = new Intl.DateTimeFormat("en", { timeStyle: "short" }).format(at);
        expect(
          screen.getByText(
            `The tools installed with it can't be listed this time. When it last responded at ${nine} yesterday, it had no tools. Check again later.`,
          ),
        ).toBeVisible();
        expect(rowNames()).toEqual([]);

        await act(async () => {
          await i18n.changeLanguage("zh-CN");
        });
        expect(
          await findByText("这次无法列出用它安装的工具。它昨天21:40响应时，没有任何工具。请稍后重新检查。"),
        ).toBeVisible();
      } finally {
        vi.useRealTimers();
        await act(async () => {
          await i18n.changeLanguage("en");
        });
      }
    });

    it("says a source that answered has nothing installed with it", async () => {
      const npm: ManagerInstance = { ...brew, id: "npm:/opt/homebrew", adapter_id: "npm" };
      served = { ...snapshot, instances: [brew, npm] };
      useUiStore.getState().openInstalled(npm.id);
      const { findByText, getByText, getByRole } = renderInstalled();

      expect(await findByText("Nothing installed with npm")).toBeInTheDocument();
      expect(getByText("Tools you install with npm show up here.")).toBeInTheDocument();
      expect(getByRole("button", { name: "Check Again" })).toBeEnabled();
      expect(rowNames()).toEqual([]);
      expect(useUiStore.getState().installedFilter).toBe(npm.id);
    });

    it("says a Python with no pip has none, not that what is installed with it shows up here", async () => {
      // `NoPip` is news, not a warning, so it is not the sidebar's ⚠︎ --
      // but the empty page still says why there is nothing, not the
      // sentence that promises rows once something is installed.
      const pip: ManagerInstance = {
        ...brew,
        id: "pip:/opt/local/bin/python3.13",
        adapter_id: "pip",
        exe_path: "/opt/local/bin/python3.13",
        prefix: "/opt/local/bin",
        version: null,
        read_only_reason: "ByDesign",
        status: { unavailable: "NoPip", notes: [] },
      };
      served = { ...snapshot, instances: [brew, pip] };
      useUiStore.getState().openInstalled(pip.id);
      const { findByText, getByText, queryByText } = renderInstalled();

      expect(await findByText("python3.13 doesn't include pip")).toBeInTheDocument();
      expect(getByText("This Python doesn't include pip, so there's nothing to list.")).toBeInTheDocument();
      expect(queryByText("Nothing installed with pip")).toBeNull();
      // Checking again cannot change this answer, so no button offers to.
      expect(screen.queryByRole("button", { name: "Check Again" })).toBeNull();
      expect(rowNames()).toEqual([]);
    });

    it("offers no Check Again on an https Ollama, which only a new address and a reopened app fix", async () => {
      const https: ManagerInstance = {
        ...ollama,
        id: "ollama:https://ollama.example:11434",
        status: { unavailable: "HttpsHostRefused", notes: [] },
      };
      served = { ...snapshot, instances: [brew, https] };
      useUiStore.getState().openInstalled(https.id);
      const { findByText } = renderInstalled();

      expect(await findByText("Connecting to Ollama over https isn't supported")).toBeInTheDocument();
      expect(screen.queryByRole("button", { name: "Check Again" })).toBeNull();
      expect(rowNames()).toEqual([]);
    });

    it("says a source whose check did not finish may not be empty, and names it alone in the lines over its rows", async () => {
      // npm lists nothing, and its check timed out: nothing listed is only
      // what the check did not get to, not "Nothing installed with npm".
      const npm: ManagerInstance = { ...brew, id: "npm:/opt/homebrew", adapter_id: "npm" };
      const pipx: ManagerInstance = { ...brew, id: "pipx:/Users/you/.local/pipx", adapter_id: "pipx" };
      served = {
        ...snapshot,
        instances: [brew, npm, pipx],
        stale: true,
        errors: [
          { instance_id: npm.id, message: "npm ls timed out" },
          { instance_id: brew.id, message: "brew outdated exited 1" },
        ],
      };
      useUiStore.getState().openInstalled(npm.id);
      const { findByText, getByText, getByRole, queryByText, unmount } = renderInstalled();

      expect(await findByText("Some checks didn't finish")).toBeInTheDocument();
      expect(getByText("npm didn't finish checking this time; some updates may not be listed yet.")).toBeInTheDocument();
      expect(queryByText("Nothing installed with npm")).toBeNull();
      expect(getByRole("button", { name: "Check Again" })).toBeEnabled();
      unmount();

      // On Homebrew, which lists its rows: the list's first line names
      // Homebrew and no other.
      useUiStore.getState().openInstalled(brew.id);
      renderInstalled();
      const title = await screen.findByText("Some checks didn't finish");
      const line = title.closest("[data-notice-line]") as HTMLElement;
      const details = within(line).getByRole("button", { name: "Details: Some checks didn't finish" });
      fireEvent.click(details);
      expect(document.getElementById(details.getAttribute("aria-controls") ?? "")).toHaveTextContent(
        /^Homebrew didn't finish checking this time; some updates may not be listed yet\.$/,
      );
      expect(within(line).getByRole("button", { name: "Check Again" })).toBeEnabled();
    });

    it("drops the filter, and shows everything, only once its source is gone from this Mac", async () => {
      served = twoSources();
      useUiStore.getState().openInstalled("cargo:/Users/you/.cargo");

      renderInstalled();

      await findRow("requests");
      expect(rowNames()).toEqual(["jq", "requests"]);
      await waitFor(() => expect(useUiStore.getState().installedFilter).toBeNull());
    });
  });

  it("sorts by name across sources, and by source under a heading for each", async () => {
    served = {
      ...snapshot,
      instances: [brew, pip],
      artifacts: [
        formula("Zstd"),
        formula("wget"),
        ...pipSnapshot.artifacts,
        formula("aria2"),
        { ...pipSnapshot.artifacts[0], key: { ...pipSnapshot.artifacts[0].key, name: "black" }, display_name: "black" },
      ],
    };
    const { getByRole, queryAllByRole } = renderInstalled();

    await findRow("aria2");
    // The sort is a popup button in the toolbar: its value, and ⌄.
    const sortBy = getByRole("combobox", { name: "Sort Order" });
    expect(sortBy.closest("[data-toolbar-slot]")).not.toBeNull();
    expect(sortBy).toHaveValue("name");
    expect(sortBy.parentElement?.firstElementChild).toHaveTextContent(/^By Name$/);
    // By name, case aside, whichever source a row is from; no headings.
    expect(rowNames()).toEqual(["aria2", "black", "requests", "wget", "Zstd"]);
    expect(queryAllByRole("heading", { level: 2 })).toHaveLength(0);

    fireEvent.change(sortBy, { target: { value: "source" } });
    await waitFor(() => expect(rowNames()).toEqual(["aria2", "wget", "Zstd", "black", "requests"]));
    expect(sortBy.parentElement?.firstElementChild).toHaveTextContent(/^By Source$/);
    const [homebrewHeading, pipHeading, ...more] = queryAllByRole("heading", { level: 2 });
    expect(more).toEqual([]);
    expect(homebrewHeading).toBe(getByRole("heading", { level: 2, name: "Homebrew · 3 tools" }));
    expect(pipHeading).toBe(getByRole("heading", { level: 2, name: "pip · 2 tools" }));
    // A group's heading: 13 bold, its count 13 in the secondary colour,
    // the source's mark at 16 -- no pill.
    expect(homebrewHeading.className.split(" ")).toEqual(expect.arrayContaining(["text-title", "text-foreground"]));
    const count = within(homebrewHeading).getByText("· 3 tools");
    expect(count.className.split(" ")).toEqual(expect.arrayContaining(["text-body", "font-normal", "text-muted"]));
    expect(homebrewHeading.querySelector("[aria-hidden='true']")?.className).toContain("h-4 w-4");
    expect(homebrewHeading.className).not.toMatch(/rounded|border|bg-/);
    // A heading says the source; the rows under it say it in sight only
    // for a name another source lists too.
    expect(within(rowOf("wget")).getByText("Homebrew")).toHaveClass("sr-only");
    expect(useUiStore.getState().installedSort).toBe("source");
  });

  it("says the source after a name two sources list, and only there (R3)", async () => {
    // black from Homebrew and from pip: two rows of one name, told apart
    // by the source's name after it. jq is Homebrew's alone.
    served = {
      ...snapshot,
      instances: [brew, pip],
      artifacts: [
        formula("black"),
        formula("jq"),
        { ...pipSnapshot.artifacts[0], key: { ...pipSnapshot.artifacts[0].key, name: "black" }, display_name: "black" },
      ],
    };
    renderInstalled();
    await findRow("jq");

    const blacks = screen
      .getAllByText("black", { selector: "[data-tool-row] p" })
      .map((name) => name.closest("[data-tool-row]") as HTMLElement);
    expect(blacks).toHaveLength(2);
    const sources = blacks.map((row) => within(row).getByText(/^(Homebrew|pip)$/));
    expect(sources.map((source) => source.textContent).sort()).toEqual(["Homebrew", "pip"]);
    for (const source of sources) {
      expect(source).not.toHaveClass("sr-only");
      expect(source).toHaveClass("text-small", "text-muted");
    }
    expect(within(rowOf("jq")).getByText("Homebrew")).toHaveClass("sr-only");

    // Filtered to one source, the name is that source's alone: out of sight again.
    act(() => useUiStore.getState().openInstalled(pip.id));
    await waitFor(() => expect(rowNames()).toEqual(["black"]));
    expect(within(rowOf("black")).getByText("pip")).toHaveClass("sr-only");
  });

  describe("the inspector", () => {
    it("opens from the row itself, not from its buttons, with all the row had no room for", async () => {
      served = {
        ...snapshot,
        artifacts: [{ ...snapshot.artifacts[0], uninstall_blocked: "Pinned" }],
        updates: [{ ...snapshot.updates[0], key: snapshot.artifacts[0].key, current: "1.8.2", target: "1.8.3", blocked: "Pinned" }],
      };
      renderInstalled();

      const jq = await findRow("jq");
      // The chip opens its own detail, not the inspector.
      chipDetail(jq, "Pinned");
      expect(screen.queryByRole("complementary")).toBeNull();

      const drawer = await openDetails("jq");
      // A pane beside the list, no dialog: its name, 15 semibold, over its
      // source, 11 in the secondary colour, by the tool's icon at 48.
      expect(screen.queryByRole("dialog")).toBeNull();
      expect(within(drawer).getByRole("heading", { level: 2, name: "jq" }).className.split(" ")).toEqual(
        expect.arrayContaining(["text-section", "text-foreground"]),
      );
      expect(within(drawer).getByText("Homebrew").className.split(" ")).toEqual(
        expect.arrayContaining(["text-small", "text-muted"]),
      );
      expect(drawer.querySelector(".h-12.w-12")).not.toBeNull();
      expect(within(drawer).getByText("Lightweight and flexible command-line JSON processor")).toHaveClass(
        "text-body-long",
      );
      expect(within(drawer).getByText("Version").nextElementSibling).toHaveTextContent("1.8.2");
      expect(within(drawer).getByText("New version").nextElementSibling).toHaveTextContent("1.8.3");
      // Its status word in the facts, and its why, in full, with the
      // command set as code, behind the word's ⓘ.
      expect(within(drawer).getByText("Status").nextElementSibling).toHaveTextContent("Pinned");
      expect(
        within(statusWhy(drawer, "Pinned")).getByText(
          wholeSentence("It's pinned in Homebrew. To uninstall it, first run /opt/homebrew/bin/brew unpin jq in Terminal."),
        ),
      ).toBeInTheDocument();
      fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });
      // Pinned against its update too: said once.
      expect(within(drawer).getAllByText("Pinned")).toHaveLength(1);
      // Neither uninstalled nor updated from here.
      expect(within(drawer).queryByRole("button", { name: "Uninstall…" })).toBeNull();
      expect(within(drawer).queryByRole("button", { name: "Update" })).toBeNull();
    });

    it("lays out as a Mac's info pane: name, description, one group of facts with the status in it, the buttons under it", async () => {
      served = {
        ...snapshot,
        artifacts: [{ ...snapshot.artifacts[0], installed_at: 1783762037, size_bytes: 1_400_000 }],
        updates: [{ ...snapshot.updates[0], key: snapshot.artifacts[0].key, current: "1.8.2", target: "1.8.3" }],
      };
      renderInstalled();

      const inspector = await openDetails("jq");
      const content = inspector.querySelector("[data-inspector-content]") as HTMLElement;
      expect(content.className.split(" ")).toEqual(expect.arrayContaining(["px-5", "pt-5", "pb-5"]));
      const [header, description, facts, actions] = [...content.children] as HTMLElement[];
      // The name 15/20 semibold, as a pane's title; the source under it.
      expect(within(header).getByRole("heading", { name: "jq" })).toHaveClass("text-section", "break-words");
      expect(header.querySelector(".h-12.w-12")).not.toBeNull();
      // The description, 13/18, wrapping: a pane's text, not a row's line.
      expect(description).toHaveAttribute("data-description");
      expect(description).toHaveClass("text-body-long", "whitespace-normal", "break-words", "mt-4");
      expect(description.className).not.toMatch(/truncate|nowrap|line-clamp/);
      // One grouped container: the group fill, corners of 10, hairlines
      // 10 in between its rows.
      expect(facts).toHaveAttribute("data-facts");
      expect(facts.tagName).toBe("DL");
      expect(facts.className.split(" ")).toEqual(
        expect.arrayContaining(["mt-4", "rounded-group", "bg-group", "[&>*+*]:before:left-2.5", "[&>*+*]:before:right-2.5"]),
      );
      const rows = [...facts.children] as HTMLElement[];
      // As a Mac's info pane orders them: versions, size, date, site, state.
      expect(rows.map((row) => row.firstElementChild?.textContent)).toEqual([
        "Version",
        "New version",
        "Space used",
        "Date installed",
        "Homepage",
        "Status",
      ]);
      for (const row of rows) {
        // 28 high, 10 in; the label 13 muted on the left, the value 13 on
        // the right, figures of one width.
        expect(row.className.split(" ")).toEqual(
          expect.arrayContaining(["flex", "min-h-7", "py-1.5", "px-2.5", "justify-between", "text-body"]),
        );
        expect(row.firstElementChild).toHaveClass("text-muted", "whitespace-nowrap");
        expect(row.lastElementChild).toHaveClass("text-right", "tabular-nums", "text-foreground");
      }
      // The status a row of the group, in words: no line of its own, no tick.
      const status = rows[5].lastElementChild as HTMLElement;
      expect(status.querySelector("[data-status-list]")).not.toBeNull();
      expect(status).toHaveTextContent("Update available");
      expect(status).not.toHaveClass("select-text");
      expect(inspector.querySelector(".text-success")).toBeNull();
      expect(rows[0].lastElementChild).toHaveClass("select-text");
      // The buttons 16 under the group, at its right: Update, the default,
      // rightmost; Uninstall…, grey, before it. Nothing pushed to the foot.
      expect(actions).toHaveAttribute("data-inspector-actions");
      expect(actions.className.split(" ")).toEqual(expect.arrayContaining(["mt-4", "flex", "justify-end", "gap-2"]));
      const buttons = within(actions).getAllByRole("button");
      expect(buttons.map((button) => button.textContent)).toEqual(["Uninstall…", "Update"]);
      expect(buttons[1].className).toContain("bg-accent");
      expect(buttons[0].className).toContain("bg-fill");
      expect(actions.parentElement).toBe(content);
      expect(content.parentElement?.parentElement).toBe(inspector);
    });

    it("scrolls only when its content is taller than the pane, so a status word's ⓘ panel may stand over the list's edge", async () => {
      // jsdom lays nothing out: the pane (the scroller, the aside's child)
      // is 500 high, and the content the viewport's 600 (`offsetHeight`
      // above), then 400.
      let paneHeight = 500;
      vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockImplementation(function (this: HTMLElement) {
        return this.parentElement?.tagName === "ASIDE" ? paneHeight : 0;
      });
      const { unmount } = renderInstalled();

      let inspector = await openDetails("jq");
      let scroller = inspector.firstElementChild as HTMLElement;
      expect(scroller).toHaveAttribute("data-inspector-scroll");
      expect(scroller.className.split(" ")).toEqual(expect.arrayContaining(["min-h-0", "flex-1", "overflow-y-auto"]));
      unmount();

      paneHeight = 700;
      renderInstalled();
      inspector = await openDetails("jq");
      scroller = inspector.firstElementChild as HTMLElement;
      expect(scroller).not.toHaveAttribute("data-inspector-scroll");
      expect(scroller.className).not.toContain("overflow");
    });

    it("lets a long English description wrap, whole", async () => {
      const long =
        "A very long description of a tool that goes on well past the width of the pane beside the list, as an npm package's may";
      served = { ...snapshot, artifacts: [{ ...snapshot.artifacts[0], description: long }] };
      renderInstalled();

      const inspector = await openDetails("jq");
      const description = within(inspector).getByText(long);
      expect(description.textContent).toBe(long);
      expect(description).toHaveClass("whitespace-normal", "break-words");
      expect(description.className).not.toMatch(/truncate|nowrap|line-clamp/);
      expect(description.getAttribute("title")).toBeNull();
    });

    it("opens on Enter from a row and puts the focus on its heading, and Escape hands it back to the row", async () => {
      served = { ...snapshot, artifacts: [snapshot.artifacts[0], formula("wget")] };
      renderInstalled();

      const jq = await findRow("jq");
      jq.focus();
      expect(fireEvent.keyDown(jq, { key: "Enter" })).toBe(false);
      const inspector = await screen.findByRole("complementary", { name: "jq" });
      const heading = within(inspector).getByRole("heading", { name: "jq" });
      await waitFor(() => expect(document.activeElement).toBe(heading));
      // Focused by script only: not a stop of its own for Tab, and no ring
      // round it, as the page's title has none.
      expect(heading).toHaveAttribute("tabindex", "-1");
      expect(heading.className.split(" ")).toContain("outline-none");

      fireEvent.keyDown(heading, { key: "Escape" });
      await waitFor(() => expect(screen.queryByRole("complementary")).toBeNull());
      await waitFor(() => expect(document.activeElement).toBe(rowOf("jq")));
    });

    it("keeps the inspector open on Enter from its own row, only putting the focus in it, and shows another row's on that row's Enter", async () => {
      served = { ...snapshot, artifacts: [snapshot.artifacts[0], formula("wget")] };
      renderInstalled();

      await openDetails("jq");
      const jq = rowOf("jq");
      jq.focus();
      fireEvent.keyDown(jq, { key: "Enter" });
      const inspector = screen.getByRole("complementary", { name: "jq" });
      expect(document.activeElement).toBe(within(inspector).getByRole("heading", { name: "jq" }));

      const wget = rowOf("wget");
      wget.focus();
      fireEvent.keyDown(wget, { key: "Enter" });
      const other = await screen.findByRole("complementary", { name: "wget" });
      await waitFor(() => expect(document.activeElement).toBe(within(other).getByRole("heading", { name: "wget" })));
    });

    it("closes with Escape, from the row or from inside it, and with its close button, the focus back on the row", async () => {
      served = { ...snapshot, artifacts: [snapshot.artifacts[0], formula("wget")] };
      renderInstalled();

      const jq = await findRow("jq");
      const rowButton = within(jq).getByRole("button", { name: "Details: jq" });
      let inspector = await openDetails("jq");
      // Pressed, the row keeps the focus: nothing moves into the pane.
      expect(document.activeElement).toBe(rowButton);

      fireEvent.keyDown(rowButton, { key: "Escape" });
      await waitFor(() => expect(screen.queryByRole("complementary")).toBeNull());
      // The row itself, where ↑ ↓ go on from.
      await waitFor(() => expect(document.activeElement).toBe(rowOf("jq")));

      inspector = await openDetails("jq");
      const close = within(inspector).getByRole("button", { name: "Close Details" });
      act(() => close.focus());
      fireEvent.keyDown(close, { key: "Escape" });
      await waitFor(() => expect(screen.queryByRole("complementary")).toBeNull());
      await waitFor(() => expect(document.activeElement).toBe(rowOf("jq")));

      inspector = await openDetails("jq");
      fireEvent.click(within(inspector).getByRole("button", { name: "Close Details" }));
      await waitFor(() => expect(screen.queryByRole("complementary")).toBeNull());
      await waitFor(() => expect(document.activeElement).toBe(rowOf("jq")));
    });

    it("closes when its row is pressed again, and shows another row when that one is pressed", async () => {
      served = { ...snapshot, artifacts: [snapshot.artifacts[0], formula("wget")] };
      renderInstalled();

      await openDetails("jq");
      const jqButton = within(rowOf("jq")).getByRole("button", { name: "Details: jq" });
      expect(jqButton).toHaveAttribute("aria-pressed", "true");
      expect(rowOf("jq")).toHaveAttribute("data-selected");
      // Said on the row the keyboard reaches too, not only on the pointer's button.
      expect(rowOf("jq")).toHaveAttribute("aria-current", "true");
      expect(rowOf("wget")).not.toHaveAttribute("aria-current");

      // The list stays in reach beside it.
      fireEvent.click(within(rowOf("wget")).getByRole("button", { name: "Details: wget" }));
      expect(await screen.findByRole("complementary", { name: "wget" })).toBeInTheDocument();
      expect(screen.queryByRole("complementary", { name: "jq" })).toBeNull();
      expect(rowOf("jq")).not.toHaveAttribute("data-selected");
      expect(rowOf("jq")).not.toHaveAttribute("aria-current");
      expect(rowOf("wget")).toHaveAttribute("data-selected");
      expect(rowOf("wget")).toHaveAttribute("aria-current", "true");

      fireEvent.click(within(rowOf("wget")).getByRole("button", { name: "Details: wget" }));
      await waitFor(() => expect(screen.queryByRole("complementary")).toBeNull());
      expect(rowOf("wget")).not.toHaveAttribute("data-selected");
      expect(within(rowOf("wget")).getByRole("button", { name: "Details: wget" })).toHaveAttribute("aria-pressed", "false");
    });

    it("is no dialog: nothing dimmed, nothing kept from the keyboard, the list narrowed beside it", async () => {
      renderInstalled();

      const inspector = await openDetails("jq");
      expect(screen.queryByRole("dialog")).toBeNull();
      // The page's own controls are still there, and still work.
      const search = screen.getByRole("searchbox", { name: "Search installed tools" });
      act(() => search.focus());
      expect(document.activeElement).toBe(search);
      // 300 wide, a hairline at its left, the window's own background, the
      // page's full height; the list beside it, not under it.
      expect(inspector.className.split(" ")).toEqual(
        expect.arrayContaining(["w-75", "shrink-0", "border-l", "border-separator", "bg-content"]),
      );
      expect(inspector).toHaveAttribute("data-inspector", "wide");
      expect(inspector.className).not.toMatch(/absolute|shadow/);
      const list = document.querySelector("[data-list]") as HTMLElement;
      expect(list.parentElement?.nextElementSibling).toBe(inspector);
    });

    it("follows ↑ and ↓ through the rows, as a Mac list's selection does, the lines between them passed over", async () => {
      served = { ...snapshot, artifacts: [...snapshot.artifacts, formula("wget"), formula("zlib")] };
      renderInstalled();

      await openDetails("jq");
      const rowButton = within(rowOf("jq")).getByRole("button", { name: "Details: jq" });
      fireEvent.keyDown(rowButton, { key: "ArrowDown" });
      expect(await screen.findByRole("complementary", { name: "wget" })).toBeInTheDocument();
      await waitFor(() => expect(document.activeElement).toBe(rowOf("wget")));
      expect(rowOf("wget")).toHaveAttribute("data-selected");

      fireEvent.keyDown(rowOf("wget"), { key: "ArrowDown" });
      expect(await screen.findByRole("complementary", { name: "zlib" })).toBeInTheDocument();
      // The line under the rows unfolds, and selects nothing: the inspector
      // stays on the last row.
      fireEvent.keyDown(rowOf("zlib"), { key: "ArrowDown" });
      const fold = screen.getByRole("button", { name: /^1 more package was installed for other software to use/ });
      await waitFor(() => expect(document.activeElement).toBe(fold));
      expect(screen.getByRole("complementary", { name: "zlib" })).toBeInTheDocument();

      fireEvent.keyDown(fold, { key: "ArrowUp" });
      await waitFor(() => expect(document.activeElement).toBe(rowOf("zlib")));
      fireEvent.keyDown(rowOf("zlib"), { key: "ArrowUp" });
      expect(await screen.findByRole("complementary", { name: "wget" })).toBeInTheDocument();
    });

    it("opens with ↓ from the list, and with Space on a row, as Quick Look opens a Finder selection", async () => {
      // Space ticks a row that can be uninstalled (InstalledPage.batch.test.tsx):
      // wget is pinned, so it has no box, and Space opens it.
      served = { ...snapshot, artifacts: [snapshot.artifacts[0], { ...formula("wget"), uninstall_blocked: "Pinned" }] };
      renderInstalled();

      await findRow("jq");
      act(() => rowOf("jq").focus());
      fireEvent.keyDown(rowOf("jq"), { key: "ArrowDown" });
      expect(await screen.findByRole("complementary", { name: "wget" })).toBeInTheDocument();

      fireEvent.keyDown(rowOf("wget"), { key: " " });
      await waitFor(() => expect(screen.queryByRole("complementary")).toBeNull());
      fireEvent.keyDown(rowOf("wget"), { key: " " });
      expect(await screen.findByRole("complementary", { name: "wget" })).toBeInTheDocument();
    });

    it("keeps its row, and where the list is scrolled, through a check that adds a row above it", async () => {
      served = { ...snapshot, artifacts: [snapshot.artifacts[0], formula("wget")] };
      // Rows as high as the list first guesses, so the new one's place is
      // known before it measures itself.
      rowHeights = { 0: 52, 1: 52, 2: 52 };
      const { queryClient } = renderInstalled();

      await openDetails("wget");
      // Where the list is scrolled, read and written as a browser would.
      const list = document.querySelector("[data-list]") as HTMLElement;
      let scrollTop = 100;
      Object.defineProperty(list, "scrollTop", {
        configurable: true,
        get: () => scrollTop,
        set: (value: number) => {
          scrollTop = value;
        },
      });

      // The next check finds a tool whose name sorts before wget's.
      act(() => {
        queryClient.setQueryData(queryKeys.snapshot, {
          ...served,
          generation: served.generation + 1,
          artifacts: [...served.artifacts, formula("curl")],
        });
      });
      await findRow("curl");
      expect(rowNames()).toEqual(["curl", "jq", "wget"]);
      expect(screen.getByRole("complementary", { name: "wget" })).toBeInTheDocument();
      expect(rowOf("wget")).toHaveAttribute("data-selected");
      // One row more above it: the list moved by one row's height, so wget
      // stays where it was on screen.
      await waitFor(() => expect(scrollTop).toBe(100 + 52));
    });

    it("closes for good once its tool is gone", async () => {
      served = { ...snapshot, artifacts: [snapshot.artifacts[0], formula("wget")] };
      const { queryClient } = renderInstalled();

      await openDetails("wget");
      act(() => {
        queryClient.setQueryData(queryKeys.snapshot, { ...served, generation: 2, artifacts: [snapshot.artifacts[0]] });
      });
      await waitFor(() => expect(screen.queryByRole("complementary")).toBeNull());
      // Back again later: listed, not selected.
      act(() => {
        queryClient.setQueryData(queryKeys.snapshot, { ...served, generation: 3 });
      });
      await findRow("wget");
      expect(screen.queryByRole("complementary")).toBeNull();
      expect(rowOf("wget")).not.toHaveAttribute("data-selected");
    });

    it("starts with nothing selected on another source", async () => {
      served = { ...snapshot, instances: [brew, pip], artifacts: [...snapshot.artifacts, ...pipSnapshot.artifacts] };
      renderInstalled();

      await openDetails("jq");
      act(() => useUiStore.getState().openInstalled(pip.id));
      await findRow("requests");
      expect(screen.queryByRole("complementary")).toBeNull();
      act(() => useUiStore.getState().openInstalled(null));
      await findRow("jq");
      expect(screen.queryByRole("complementary")).toBeNull();
    });

    it("says when a tool was installed and how big it is, where its source said", async () => {
      served = {
        ...snapshot,
        artifacts: [{ ...snapshot.artifacts[0], installed_at: 1783762037, size_bytes: 1_450_000 }, formula("wget")],
      };
      renderInstalled();

      let inspector = await openDetails("jq");
      expect(within(inspector).getByText("Date installed").nextElementSibling).toHaveTextContent(
        new Intl.DateTimeFormat("en", { dateStyle: "medium" }).format(new Date(1783762037 * 1000)),
      );
      expect(within(inspector).getByText("Space used").nextElementSibling).toHaveTextContent("About 1.4 MB");
      // A row of the facts' group: the label in the secondary colour, the
      // value at the right, tabular.
      const group = within(inspector).getByText("Space used").parentElement?.parentElement as HTMLElement;
      expect(group).toHaveAttribute("data-facts");
      expect(within(inspector).getByText("Space used")).toHaveClass("text-muted", "whitespace-nowrap");
      expect(within(inspector).getByText("Space used").nextElementSibling).toHaveClass("tabular-nums", "text-right");

      fireEvent.click(within(rowOf("wget")).getByRole("button", { name: "Details: wget" }));
      inspector = await screen.findByRole("complementary", { name: "wget" });
      expect(within(inspector).queryByText("Space used")).toBeNull();
      expect(within(inspector).getByText("Date installed")).toBeInTheDocument();
    });

    it("updates through the Updates page's own confirmation, where that page would, and shows the progress there", async () => {
      renderInstalled();

      await findRow("jq");
      fireEvent.click(screen.getByRole("button", { name: /^1 more package was installed for other software to use/ }));
      const drawer = await openDetails("glib");
      expect(within(drawer).getByText("Update available")).toBeInTheDocument();
      expect(within(drawer).getByText("New version").nextElementSibling).toHaveTextContent("2.90.0");

      fireEvent.click(within(drawer).getByRole("button", { name: "Update" }));
      const confirm = await screen.findByRole("alertdialog", { name: "Update “glib”?" });
      expect(mockInvoke).toHaveBeenCalledWith("plan_operation", {
        request: { kind: "Upgrade", instance_id: "brew:/opt/homebrew", artifact_kind: "Formula", name: "glib" },
      });
      showCommand(confirm);
      expect(await within(confirm).findByText(command("/opt/homebrew/bin/brew upgrade --formula glib"))).toBeInTheDocument();
      expect(within(confirm).getByText("2.88.3 → 2.90.0")).toBeInTheDocument();

      operations = [
        {
          id: 7,
          kind: "Upgrade",
          instance_id: "brew:/opt/homebrew",
          artifact_kind: "Formula",
          name: "glib",
          status: "Running",
          outcome: null,
          argv_preview: ["/opt/homebrew/bin/brew", "upgrade", "glib"],
          cancel_policy: "KillThenReconcile",
        },
      ];
      fireEvent.click(within(confirm).getByRole("button", { name: "Update" }));
      await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("submit_operation", { planId: "1" }));
      await waitFor(() => expect(screen.queryByRole("alertdialog", { name: "Update “glib”?" })).toBeNull());
      // Where the button was, as on the Updates page's row.
      const open = screen.getByRole("complementary", { name: "glib" });
      expect(await within(open).findByText("Updating…")).toBeInTheDocument();
      expect(within(open).queryByRole("button", { name: "Update" })).toBeNull();
      expect(useUiStore.getState().updateTargets).toEqual({ 7: "2.90.0" });
    });

    it("offers Retry beside a failed update, into the same confirmation", async () => {
      operations = [
        {
          id: 9,
          kind: "Upgrade",
          instance_id: "brew:/opt/homebrew",
          artifact_kind: "Formula",
          name: "glib",
          status: "Done",
          outcome: { Failed: { exit_code: 1, summary: "Error: glib: no bottle", cause: failureCause("Error: glib: no bottle") } },
          argv_preview: ["/opt/homebrew/bin/brew", "upgrade", "glib"],
          cancel_policy: "KillThenReconcile",
        },
      ];
      useUiStore.setState({ updateTargets: { 9: "2.90.0" } });
      renderInstalled();

      await findRow("jq");
      fireEvent.click(screen.getByRole("button", { name: /^1 more package was installed for other software to use/ }));
      const drawer = await openDetails("glib");
      expect(await within(drawer).findByText("Couldn't update")).toBeInTheDocument();
      expect(within(drawer).getByRole("button", { name: "View log: glib" })).toBeInTheDocument();
      expect(within(drawer).queryByRole("button", { name: "Update" })).toBeNull();

      fireEvent.click(within(drawer).getByRole("button", { name: "Retry" }));
      await screen.findByRole("alertdialog", { name: "Update “glib”?" });
      expect(mockInvoke).toHaveBeenCalledWith("plan_operation", {
        request: { kind: "Upgrade", instance_id: "brew:/opt/homebrew", artifact_kind: "Formula", name: "glib" },
      });
    });

    // r22 W1: also once Clear in 「最近的更新记录」 has dismissed the record,
    // as Rust sends it since f17: Clear does not resolve the stop.
    it.each([
      ["", false],
      [", also once Clear has dismissed it", true],
    ] as const)("offers View Steps, not Update, for an update an earlier launch kept as stopped for the password%s", async (_when, dismissed) => {
      // As `get_history` answers after a restart: the stop is the newest
      // record of glib, from another run, and the check still offers glib.
      const answer = mockInvoke.getMockImplementation()!;
      mockInvoke.mockImplementation((cmd, args) =>
        cmd === "get_history"
          ? Promise.resolve({
              run: "this-launch",
              cleared_before: dismissed ? Date.now() : null,
              records: [
                {
                  run: "earlier-launch", op_id: 4, finished_at: Date.now() - 60_000,
                  key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "glib" }, display_name: "glib",
                  adapter_id: "brew", kind: "Update", from_version: "2.88.3", to_version: null,
                  result: { Failed: { cause: "needsPassword" } }, verified: false, dismissed,
                },
              ],
            })
          : answer(cmd, args),
      );
      renderInstalled();

      await findRow("jq");
      fireEvent.click(screen.getByRole("button", { name: /^1 more package was installed for other software to use/ }));
      const drawer = await openDetails("glib");
      const steps = await within(drawer).findByRole("button", { name: "View steps: glib" });
      expect(within(drawer).queryByRole("button", { name: "Update" })).toBeNull();
      expect(within(drawer).queryByRole("button", { name: "Retry" })).toBeNull();

      fireEvent.click(steps);
      const dialog = await screen.findByRole("dialog", { name: "glib" });
      await within(dialog).findByRole("button", { name: "Copy Command" });
      expect(dialog.querySelector("[data-command-argv]")?.textContent).toBe("/opt/homebrew/bin/brew upgrade --formula glib");
      expect(mockInvoke).toHaveBeenCalledWith("plan_operation", {
        request: { kind: "Upgrade", instance_id: "brew:/opt/homebrew", artifact_kind: "Formula", name: "glib" },
      });
      expect(mockInvoke.mock.calls.some(([cmd]) => cmd === "submit_operation")).toBe(false);
    });

    it("shows an update that could not start in its own tool's inspector, not in another's", async () => {
      const answer = mockInvoke.getMockImplementation()!;
      mockInvoke.mockImplementation((cmd, args) =>
        cmd === "plan_operation" ? Promise.reject("brew is busy") : answer(cmd, args),
      );
      renderInstalled();

      await findRow("jq");
      fireEvent.click(screen.getByRole("button", { name: /^1 more package was installed for other software to use/ }));
      const glib = await openDetails("glib");
      fireEvent.click(within(glib).getByRole("button", { name: "Update" }));
      // Without the backend's words: "Show technical details" is off.
      expect(await within(glib).findByText("Couldn't prepare the update. Try again later.")).toBeInTheDocument();
      expect(within(glib).queryByText(/brew is busy/)).toBeNull();

      fireEvent.click(within(glib).getByRole("button", { name: "Close Details" }));
      await waitFor(() => expect(screen.queryByRole("complementary", { name: "glib" })).toBeNull());
      const jq = await openDetails("jq");
      expect(within(jq).queryByText(/^Couldn't prepare the update/)).toBeNull();
    });

    it("says why an update could not start in the backend's words with technical details on", async () => {
      servedSettings = { ...settings, show_technical_details: true };
      const answer = mockInvoke.getMockImplementation()!;
      mockInvoke.mockImplementation((cmd, args) =>
        cmd === "plan_operation" ? Promise.reject("brew is busy") : answer(cmd, args),
      );
      renderInstalled();

      await findRow("jq");
      fireEvent.click(screen.getByRole("button", { name: /^1 more package was installed for other software to use/ }));
      const glib = await openDetails("glib");
      fireEvent.click(within(glib).getByRole("button", { name: "Update" }));
      expect(await within(glib).findByText("Couldn't prepare the update: brew is busy")).toBeInTheDocument();
    });

    it.each([
      ["Queued", "Queued", "Queued: uninstall jq"],
      ["Running", "Uninstalling…", "Uninstalling jq…"],
    ] as const)("offers no second Uninstall while one is %s, on the row and in the inspector", async (status, label, rowName) => {
      operations = [
        {
          id: 11,
          kind: "Uninstall",
          instance_id: "brew:/opt/homebrew",
          artifact_kind: "Formula",
          name: "jq",
          status,
          outcome: null,
          argv_preview: ["/opt/homebrew/bin/brew", "uninstall", "jq"],
          cancel_policy: "KillThenReconcile",
        },
      ];
      renderInstalled();

      const row = await findRow("jq");
      const held = await within(row).findByRole("button", { name: rowName });
      expect(held).toBeDisabled();
      expect(held).toHaveTextContent(label);
      expect(within(row).queryByRole("button", { name: ROW_UNINSTALL })).toBeNull();
      const drawer = await openDetails("jq");
      expect(within(drawer).getByRole("button", { name: label })).toBeDisabled();
      expect(within(drawer).queryByRole("button", { name: "Uninstall…" })).toBeNull();
    });

    it.each([
      ["Queued", "Queued"],
      ["Running", "Uninstalling…"],
    ] as const)("offers no Update of a tool whose uninstall is %s, and says the uninstall once", async (status, label) => {
      operations = [
        {
          id: 12,
          kind: "Uninstall",
          instance_id: "brew:/opt/homebrew",
          artifact_kind: "Formula",
          name: "glib",
          status,
          outcome: null,
          argv_preview: ["/opt/homebrew/bin/brew", "uninstall", "glib"],
          cancel_policy: "KillThenReconcile",
        },
      ];
      renderInstalled();

      await findRow("jq");
      fireEvent.click(screen.getByRole("button", { name: /^1 more package was installed for other software to use/ }));
      const drawer = await openDetails("glib");
      expect(within(drawer).getByRole("button", { name: label })).toBeDisabled();
      // Its upgrade would only queue behind the uninstall, and fail.
      expect(within(drawer).queryByRole("button", { name: "Update" })).toBeNull();
      // The held Uninstall says it; nothing beside it says it again.
      const actions = drawer.querySelector("[data-inspector-actions]")!;
      expect(actions.textContent).toBe(label);
    });

    it("offers no Update for an update the Updates page does not offer", async () => {
      served = {
        ...snapshot,
        instances: [{ ...brew, status: { unavailable: "NotResponding", notes: [] } }],
      };
      renderInstalled();

      await findRow("jq");
      fireEvent.click(screen.getByRole("button", { name: /^1 more package was installed for other software to use/ }));
      const drawer = await openDetails("glib");
      expect(within(drawer).getByText("Can't update now")).toBeInTheDocument();
      expect(within(drawer).getByText("Can't uninstall now")).toBeInTheDocument();
      // What to do, behind each of the two words' ⓘ.
      expect(statusWhy(drawer, "Can't update now")).toHaveTextContent("Homebrew isn't responding. Click Check Again later.");
      fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });
      expect(statusWhy(drawer, "Can't uninstall now")).toHaveTextContent(
        "Homebrew isn't responding. Click Check Again later.",
      );
      fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });
      // Its source's own line, whole.
      expect(within(drawer).getByText("Homebrew isn't responding")).toBeInTheDocument();
      expect(
        within(drawer).getByText(
          "2 tools were installed with Homebrew. It didn't respond this time, so they're shown as they were when it last responded. Check again later.",
        ),
      ).toBeInTheDocument();
      // And the button its next step needs, under its sentence.
      expect(within(drawer).getByRole("button", { name: "Check Again" })).toBeEnabled();
      expect(within(drawer).queryByRole("button", { name: "Update" })).toBeNull();
      expect(within(drawer).getByRole("button", { name: "Uninstall…" })).toBeDisabled();
    });

    it("uninstalls through the row's own dialog, then opens the log beside it", async () => {
      renderInstalled();

      const drawer = await openDetails("jq");
      const uninstall = within(drawer).getByRole("button", { name: "Uninstall…" });
      fireEvent.click(uninstall);
      const dialog = await screen.findByRole("alertdialog", { name: "Uninstall “jq”?" });
      await within(dialog).findByRole("button", { name: "Show Command" });
      showCommand(dialog);
      expect(within(dialog).getByText(command("/opt/homebrew/bin/brew uninstall --formula jq"))).toBeInTheDocument();

      fireEvent.click(within(dialog).getByRole("button", { name: "Uninstall" }));
      await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("submit_operation", { planId: "1" }));
      await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
      await waitFor(() => expect(useUiStore.getState().drawerOpen).toBe(true));
      expect(useUiStore.getState().focusedOpId).toBe(7);
      // No dialog to get out of the log's way: the inspector stays until
      // its tool is gone, and the focus is on its heading -- not on the
      // button that asked, off from now on as 「正在卸载…」 -- which the log
      // gives it back to when it closes.
      expect(screen.getByRole("complementary", { name: "jq" })).toBe(drawer);
      await waitFor(() => expect(document.activeElement).toBe(within(drawer).getByRole("heading", { name: "jq" })));
    });

    it("gives the focus back to its own Uninstall and Update when their confirmations are dismissed", async () => {
      renderInstalled();

      await findRow("jq");
      fireEvent.click(screen.getByRole("button", { name: /^1 more package was installed for other software to use/ }));
      const drawer = await openDetails("glib");
      const uninstall = within(drawer).getByRole("button", { name: "Uninstall…" });
      const update = within(drawer).getByRole("button", { name: "Update" });

      fireEvent.click(uninstall);
      await screen.findByRole("alertdialog", { name: "Uninstall “glib”?" });
      fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });
      await waitFor(() => expect(screen.queryByRole("alertdialog", { name: "Uninstall “glib”?" })).toBeNull());
      await waitFor(() => expect(document.activeElement).toBe(uninstall));
      // The inspector stays: only the question went.
      expect(screen.getByRole("complementary", { name: "glib" })).toBe(drawer);

      fireEvent.click(update);
      const confirm = await screen.findByRole("alertdialog", { name: "Update “glib”?" });
      fireEvent.click(within(confirm).getByRole("button", { name: "Cancel" }));
      await waitFor(() => expect(screen.queryByRole("alertdialog", { name: "Update “glib”?" })).toBeNull());
      await waitFor(() => expect(document.activeElement).toBe(update));
    });

    it("says where a tool is only with technical details on", async () => {
      served = { ...snapshot, instances: [claudeInstance], artifacts: [claudeArtifact], updates: [] };
      const first = renderInstalled();
      let drawer = await openDetails("Claude Code");
      expect(within(drawer).queryByText("Location")).toBeNull();
      first.unmount();

      servedSettings = { ...settings, show_technical_details: true };
      renderInstalled();
      drawer = await openDetails("Claude Code");
      expect(within(drawer).getByText("Location").nextElementSibling).toHaveTextContent(
        "/Users/someone/.local/share/claude/versions/2.1.281",
      );
    });

    it("lets its versions and where it is be selected, to be copied, and nothing else", async () => {
      served = {
        ...snapshot,
        instances: [claudeInstance],
        artifacts: [claudeArtifact],
        updates: [
          {
            key: claudeArtifact.key,
            current: "2.1.281",
            target: "2.1.290",
            channel: "Native",
            checkable: true,
            warnings: [],
            blocked: null,
          },
        ],
      };
      servedSettings = { ...settings, show_technical_details: true };
      renderInstalled();

      const drawer = await openDetails("Claude Code");
      const values = ["Version", "New version", "Location"].map(
        (term) => within(drawer).getByText(term).nextElementSibling,
      );
      expect(values.map((value) => value?.textContent)).toEqual([
        "2.1.281",
        "2.1.290",
        "/Users/someone/.local/share/claude/versions/2.1.281",
      ]);
      for (const value of values) expect(value).toHaveClass("select-text");
      // Not its name, its description, what each value is, nor its chips.
      expect([...drawer.querySelectorAll(".select-text")]).toEqual(values);
    });
  });

  describe("the ⋯ menu", () => {
    it("opens the details, and copies the command a chip talks about only with technical details on", async () => {
      const writeText = vi.fn(() => Promise.resolve());
      Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
      try {
        served = {
          ...snapshot,
          artifacts: [{ ...snapshot.artifacts[0], uninstall_blocked: "Pinned" }, formula("wget")],
          updates: [],
        };
        const first = renderInstalled();
        fireEvent.click(within(await findRow("jq")).getByRole("button", { name: "More actions for jq" }));
        let menu = screen.getByRole("menu");
        expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Details"]);
        fireEvent.click(within(menu).getByRole("menuitem", { name: "Details" }));
        expect(await screen.findByRole("complementary", { name: "jq" })).toBeInTheDocument();
        expect(rowOf("jq")).toHaveAttribute("data-selected");
        first.unmount();

        servedSettings = { ...settings, show_technical_details: true };
        renderInstalled();
        // A plain row has no command it could copy without a plan.
        fireEvent.click(within(await findRow("wget")).getByRole("button", { name: "More actions for wget" }));
        expect(within(screen.getByRole("menu")).getAllByRole("menuitem").map((item) => item.textContent)).toEqual([
          "Details",
        ]);
        fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });

        fireEvent.click(within(rowOf("jq")).getByRole("button", { name: "More actions for jq" }));
        menu = screen.getByRole("menu");
        // Copy Command in a group of its own, a hairline over it.
        const copyItem = within(menu).getByRole("menuitem", { name: "Copy Command" });
        const separator = within(menu).getByRole("separator");
        expect(
          separator.compareDocumentPosition(copyItem) & Node.DOCUMENT_POSITION_FOLLOWING,
        ).toBeTruthy();
        expect(
          separator.compareDocumentPosition(within(menu).getByRole("menuitem", { name: "Details" })) &
            Node.DOCUMENT_POSITION_PRECEDING,
        ).toBeTruthy();
        fireEvent.click(copyItem);
        expect(writeText).toHaveBeenCalledWith("/opt/homebrew/bin/brew unpin jq");
        expect(await screen.findByText("Copied", { selector: "[role=status]" })).toBeInTheDocument();
      } finally {
        Object.defineProperty(navigator, "clipboard", { value: undefined, configurable: true });
      }
    });
  });

  describe("lines in the window's language", () => {
    // Tables of this test's own. The Chinese: jq's line, prettier's --
    // npm's inventory gives no description -- and one under Claude Code's
    // key, which its summary keeps off its row; none for corepack. The
    // English: prettier's, and one under Claude Code's key; none for
    // corepack, nor, as in the built-in one, for jq, whose source gives
    // its own words.
    const JQ = "Lightweight and flexible command-line JSON processor";
    const PRETTIER = "Opinionated code formatter";
    const chinese = () =>
      lazyDescriptionTable(async () => ({
        "brew:jq": "命令行 JSON 处理工具",
        "npm:prettier": "代码格式化工具",
        "standalone:claude": "不该出现的一行",
      }));
    const english = () =>
      lazyDescriptionTable(async () => ({
        "npm:prettier": PRETTIER,
        "standalone:claude": "A line that should not show",
      }));
    const npm: ManagerInstance = { ...brew, id: "npm:/opt/homebrew", adapter_id: "npm", exe_path: "/opt/homebrew/bin/npm" };
    const npmPackage = (name: string): InstalledArtifact =>
      formula(name, { key: { instance_id: "npm:/opt/homebrew", kind: "Package", name }, description: null });

    beforeEach(() => {
      served = {
        ...snapshot,
        instances: [brew, npm, claudeInstance],
        artifacts: [snapshot.artifacts[0], npmPackage("prettier"), npmPackage("corepack"), claudeArtifact],
        updates: [],
      };
    });

    async function inChinese(check: () => Promise<void>) {
      await act(async () => {
        await i18n.changeLanguage("zh-CN");
      });
      try {
        await check();
      } finally {
        await act(async () => {
          await i18n.changeLanguage("en");
        });
      }
    }

    it("gives a row its line in Chinese where the table has one, and what it said where the table has none", async () => {
      renderInstalled({ toolDescriptions: { "zh-CN": chinese() } });

      expect(within(await findRow("jq")).getByText(JQ)).toBeInTheDocument();
      expect(within(rowOf("prettier")).getByText("npm package")).toBeInTheDocument();
      expect(within(rowOf("Claude Code")).getByText("Anthropic's AI coding assistant")).toBeInTheDocument();

      await inChinese(async () => {
        expect(await within(rowOf("jq")).findByText("命令行 JSON 处理工具")).toBeInTheDocument();
        expect(within(rowOf("jq")).queryByText(JQ)).toBeNull();
        expect(within(rowOf("prettier")).getByText("代码格式化工具")).toBeInTheDocument();
        expect(within(rowOf("corepack")).getByText("npm软件包")).toBeInTheDocument();
        expect(within(rowOf("Claude Code")).getByText("Anthropic的AI编程助手")).toBeInTheDocument();
        expect(screen.queryByText("不该出现的一行")).toBeNull();
      });

      // English again: the source's words, as before.
      expect(within(rowOf("jq")).getByText(JQ)).toBeInTheDocument();
      expect(within(rowOf("prettier")).getByText("npm package")).toBeInTheDocument();
    });

    it("shows only the Chinese line in a tool's details where there is one, not the source's English under it", async () => {
      renderInstalled({ toolDescriptions: { "zh-CN": chinese() } });
      await findRow("jq");

      await inChinese(async () => {
        await within(rowOf("jq")).findByText("命令行 JSON 处理工具");
        fireEvent.click(within(rowOf("jq")).getByRole("button", { name: "详情：jq" }));
        const drawer = await screen.findByRole("complementary", { name: "jq" });
        const line = within(drawer).getByText("命令行 JSON 处理工具");
        expect(line).toHaveAttribute("data-description");
        expect(within(drawer).queryByText(JQ)).toBeNull();
        expect(drawer.textContent).not.toContain(JQ);
        fireEvent.click(within(drawer).getByRole("button", { name: "关闭详情" }));
        await waitFor(() => expect(screen.queryByRole("complementary")).toBeNull());

        // prettier's source said nothing: its line alone.
        fireEvent.click(within(rowOf("prettier")).getByRole("button", { name: "详情：prettier" }));
        const prettier = await screen.findByRole("complementary", { name: "prettier" });
        expect(within(prettier).getAllByText("代码格式化工具")).toHaveLength(1);
        fireEvent.click(within(prettier).getByRole("button", { name: "关闭详情" }));
        await waitFor(() => expect(screen.queryByRole("complementary")).toBeNull());
      });

      // In English: the source's words, once.
      const drawer = await openDetails("jq");
      expect(within(drawer).getAllByText(JQ)).toHaveLength(1);
      expect(within(drawer).queryByText("命令行 JSON 处理工具")).toBeNull();
    });

    it("shows the source's own words in a tool's details in Chinese where there is no Chinese line", async () => {
      // A table with no line for jq: its source's English is all there is.
      renderInstalled({
        toolDescriptions: { "zh-CN": lazyDescriptionTable(async () => ({ "npm:prettier": "代码格式化工具" })) },
      });
      await findRow("jq");

      await inChinese(async () => {
        await within(rowOf("prettier")).findByText("代码格式化工具");
        fireEvent.click(within(rowOf("jq")).getByRole("button", { name: "详情：jq" }));
        const drawer = await screen.findByRole("complementary", { name: "jq" });
        const own = within(drawer).getByText(JQ);
        expect(own).toHaveAttribute("data-description");
        expect(within(drawer).getAllByText(JQ)).toHaveLength(1);
      });
    });

    it("gives a package's row its line in English where the English table has one, and switches it with the language", async () => {
      renderInstalled({ toolDescriptions: { en: english(), "zh-CN": chinese() } });

      // prettier's line, where the row said what npm lists; corepack, which
      // the table has no line for, still that; jq its source's own words,
      // and Claude Code its summary.
      expect(await within(await findRow("prettier")).findByText(PRETTIER)).toBeInTheDocument();
      expect(within(rowOf("prettier")).queryByText("npm package")).toBeNull();
      expect(within(rowOf("corepack")).getByText("npm package")).toBeInTheDocument();
      expect(within(rowOf("jq")).getByText(JQ)).toBeInTheDocument();
      expect(within(rowOf("Claude Code")).getByText("Anthropic's AI coding assistant")).toBeInTheDocument();
      expect(screen.queryByText("A line that should not show")).toBeNull();

      await inChinese(async () => {
        expect(await within(rowOf("prettier")).findByText("代码格式化工具")).toBeInTheDocument();
        expect(within(rowOf("prettier")).queryByText(PRETTIER)).toBeNull();
        expect(within(rowOf("corepack")).getByText("npm软件包")).toBeInTheDocument();
      });

      // English again: its line, at once.
      expect(within(rowOf("prettier")).getByText(PRETTIER)).toBeInTheDocument();
      expect(within(rowOf("corepack")).getByText("npm package")).toBeInTheDocument();
    });

    it("finds a row by a word of its line, in the window's language or the other, reading the other's lines only for a search", async () => {
      const read = vi.fn(async () => ({ "brew:jq": "命令行JSON处理工具", "npm:prettier": "代码格式化工具" }));
      renderInstalled({ toolDescriptions: { en: english(), "zh-CN": lazyDescriptionTable(read) } });
      await within(await findRow("prettier")).findByText(PRETTIER);
      // A window in English, not searched: the Chinese lines are not read.
      expect(read).not.toHaveBeenCalled();
      const search = screen.getByRole("searchbox", { name: "Search installed tools" });

      // A word of the line the row shows.
      fireEvent.change(search, { target: { value: "formatter" } });
      await waitFor(() => expect(rowNames()).toEqual(["prettier"]));
      expect(read).toHaveBeenCalledTimes(1);
      fireEvent.change(search, { target: { value: "JSON" } });
      await waitFor(() => expect(rowNames()).toEqual(["jq"]));
      // What matched is on the row: no word about a command.
      expect(within(rowOf("jq")).queryByText(/^Command: /)).toBeNull();
      // A word of its line in Chinese, once the search has read them.
      fireEvent.change(search, { target: { value: "格式化" } });
      await waitFor(() => expect(rowNames()).toEqual(["prettier"]));
      fireEvent.change(search, { target: { value: "" } });

      await inChinese(async () => {
        const field = screen.getByRole("searchbox", { name: "搜索已安装的工具" });
        fireEvent.change(field, { target: { value: "编程" } });
        await waitFor(() => expect(rowNames()).toEqual(["Claude Code"]));
        fireEvent.change(field, { target: { value: "anthropic" } });
        await waitFor(() => expect(rowNames()).toEqual(["Claude Code"]));
        fireEvent.change(field, { target: { value: "json" } });
        await waitFor(() => expect(rowNames()).toEqual(["jq"]));
        // The English lines too: Claude Code's summary, prettier's line.
        fireEvent.change(field, { target: { value: "coding" } });
        await waitFor(() => expect(rowNames()).toEqual(["Claude Code"]));
        fireEvent.change(field, { target: { value: "opinionated" } });
        await waitFor(() => expect(rowNames()).toEqual(["prettier"]));
        fireEvent.change(field, { target: { value: "" } });
      });
    });

    it("keeps finding by either language's line when the language changes during a search", async () => {
      renderInstalled({ toolDescriptions: { en: english(), "zh-CN": chinese() } });
      await within(await findRow("prettier")).findByText(PRETTIER);

      await inChinese(async () => {
        const field = screen.getByRole("searchbox", { name: "搜索已安装的工具" });
        fireEvent.change(field, { target: { value: "格式化" } });
        await waitFor(() => expect(rowNames()).toEqual(["prettier"]));
        // Left as it is when the window turns English below.
      });

      // In English, the field still says 「格式化」: prettier's Chinese line, now the other language's, finds it.
      const search = screen.getByRole("searchbox", { name: "Search installed tools" });
      expect(search).toHaveValue("格式化");
      await waitFor(() => expect(rowNames()).toEqual(["prettier"]));
      // And the English lines, now the row's own.
      fireEvent.change(search, { target: { value: "formatter" } });
      await waitFor(() => expect(rowNames()).toEqual(["prettier"]));
      fireEvent.change(search, { target: { value: "JSON" } });
      await waitFor(() => expect(rowNames()).toEqual(["jq"]));
      fireEvent.change(search, { target: { value: "" } });
    });

    it("shows a package's line in English alone in its details: its source said nothing", async () => {
      renderInstalled({ toolDescriptions: { en: english() } });
      await within(await findRow("prettier")).findByText(PRETTIER);

      const drawer = await openDetails("prettier");
      expect(within(drawer).getByText(PRETTIER)).toBeInTheDocument();
      expect(within(drawer).queryByText("npm package")).toBeNull();
      expect(within(drawer).getAllByText(PRETTIER)).toHaveLength(1);
    });
  });

  describe("logos", () => {
    // A pack of this test's own: jq's logo and Homebrew's, and nothing for
    // glib or pip.
    const JQ = "M1 1h22v22H1z";
    const HOMEBREW = "M3 3h18v18H3z";
    const toolIcons = loadToolIcons(
      {
        version: 1,
        generated: "2026-09-28",
        glyphs: {
          "si-jq": { path: JQ, hex: "181717", title: "jq" },
          "si-homebrew": { path: HOMEBREW, hex: "FBB040", title: "Homebrew" },
        },
        rasters: {},
        tools: { "brew:jq": "si-jq" },
        sources: { brew: "si-homebrew" },
      },
      new Map(),
    );
    const glyph = (path: string) => `path[d="${path}"]`;

    it("shows a tool's logo with its source's on the corner, on its row and in its details", async () => {
      renderInstalled({ toolIcons });
      // jq's logo, not on the corner, and Homebrew's, on it.
      const expectLogos = (avatarHolder: Element) => {
        const logo = avatarHolder.querySelector(glyph(JQ));
        expect(logo).toBeInstanceOf(Element);
        expect(logo?.closest("[data-source-badge]")).toBeNull();
        expect(avatarHolder.querySelector(`[data-source-badge] ${glyph(HOMEBREW)}`)).toBeInstanceOf(Element);
      };

      expectLogos(await findRow("jq"));
      expectLogos(await openDetails("jq"));
    });

    it("shows the program tile on a tool with no logo of its own, its source's logo on the corner and over its group, and the initial where the source has none", async () => {
      served = {
        ...snapshot,
        instances: [brew, pip],
        artifacts: [...snapshot.artifacts, ...pipSnapshot.artifacts],
      };
      renderInstalled({ toolIcons });

      fireEvent.click(await screen.findByRole("button", { name: /^1 more package was installed for other software to use/ }));
      // Not its source's logo, which would make it look like Homebrew
      // itself (I8): the program tile, with Homebrew's on its corner.
      const glib = await findRow("glib");
      expect(glib.querySelector("[data-program-tile]")).not.toBeNull();
      expect(glib.querySelector(`[data-source-badge] ${glyph(HOMEBREW)}`)).not.toBeNull();
      expect(glib.querySelectorAll(glyph(HOMEBREW))).toHaveLength(1);
      const requests = await findRow("requests");
      expect(requests.querySelector("[data-program-tile]")).not.toBeNull();
      expect(within(requests).getByText("P").closest("[data-source-badge]")).not.toBeNull();

      fireEvent.change(screen.getByRole("combobox", { name: "Sort Order" }), { target: { value: "source" } });
      const heading = await screen.findByRole("heading", { level: 2, name: "Homebrew · 2 tools" });
      expect(heading.querySelector(glyph(HOMEBREW))).not.toBeNull();
      const pipHeading = screen.getByRole("heading", { level: 2, name: "pip · 1 tool" });
      expect(within(pipHeading).getByText("P")).toHaveAttribute("aria-hidden", "true");
    });
  });
});

describe("which copy of a command runs (advantages round, item 4)", () => {
  const npm: ManagerInstance = {
    ...brew,
    id: "npm:/opt/homebrew",
    adapter_id: "npm",
    exe_path: "/opt/homebrew/bin/npm",
  };
  const npmClaude: InstalledArtifact = {
    ...formula("@anthropic-ai/claude-code"),
    key: { instance_id: "npm:/opt/homebrew", kind: "Package", name: "@anthropic-ai/claude-code" },
    description: null,
    installed_at: null,
    facts: { ...NO_FACTS, family: "claude-code", commands: [{ name: "claude", state: "Runs" }] },
  };
  const nativeClaude: InstalledArtifact = {
    ...claudeArtifact,
    facts: {
      ...NO_FACTS,
      family: "claude-code",
      commands: [{ name: "claude", state: { ShadowedBy: { by: npmClaude.key } } }],
    },
  };

  function serveBoth(native: InstalledArtifact, other: InstalledArtifact) {
    served = {
      ...snapshot,
      instances: [brew, npm, claudeInstance],
      artifacts: [...snapshot.artifacts, other, native],
      updates: [],
    };
  }

  it("marks both copies of one tool on their rows, and says in the details which one typing it runs", async () => {
    serveBoth(nativeClaude, npmClaude);
    renderInstalled();

    const native = await findRow("Claude Code");
    expect(chipsOf(native)).toEqual(["Installed twice"]);
    expect(chipsOf(rowOf("@anthropic-ai/claude-code"))).toEqual(["Installed twice"]);
    const why = chipDetail(native, "Installed twice");
    expect(why).toHaveTextContent("npm has a copy too.");
    expect(why).toHaveTextContent("Typing claude in Terminal runs the copy from npm.");

    const inspector = await openDetails("Claude Code");
    const group = inspector.querySelector("[data-commands]") as HTMLElement;
    expect(within(group).getByRole("heading", { name: /In Terminal/ })).toBeInTheDocument();
    expect(group.querySelector("[data-command-line]")).toHaveTextContent(/^claudeRuns the copy from npm/);
    // Nothing to press but the ⓘs: no button fixes anything.
    expect(within(group).getAllByRole("button").map((button) => button.getAttribute("aria-label"))).toEqual([
      "Details: In Terminal",
      "Details: claude",
    ]);
    // What to make of it, under the description: which copy Terminal
    // runs, that it does not use this one, and that this one may go. The
    // facts' 「状态」 leaves the word out: said once, up there.
    const callout = inspector.querySelector("[data-description] + [data-inspector-callout]") as HTMLElement;
    expect([...callout.querySelectorAll("[data-twin-advice]")].map((line) => line.textContent)).toEqual([
      `Typing claude in Terminal runs the copy from npm, version ${npmClaude.version}, so Terminal doesn't use this one.`,
      "If you don't need it, you can uninstall this copy.",
    ]);
    expect(within(inspector).queryByText("Installed twice")).toBeNull();

    // The copy Terminal runs says which one it does not use.
    const npmDetails = await openDetails("@anthropic-ai/claude-code");
    expect(npmDetails.querySelector("[data-twin-advice]")?.textContent).toBe(
      `Typing claude in Terminal runs this copy; Terminal doesn't use the one from Claude Code's own installer, version ${nativeClaude.version}.`,
    );
  });

  it("still tells two copies apart when nothing was said about which runs, and shows no group", async () => {
    // `Session::note_login_path(false)`: the names, and no verdicts.
    serveBoth(
      { ...nativeClaude, facts: { ...NO_FACTS, family: "claude-code", commands: [{ name: "claude", state: null }] } },
      { ...npmClaude, facts: { ...NO_FACTS, family: "claude-code", commands: [{ name: "claude", state: null }] } },
    );
    renderInstalled();

    const native = await findRow("Claude Code");
    expect(chipsOf(native)).toEqual(["Installed twice"]);
    expect(chipDetail(native, "Installed twice")).toHaveTextContent(/^npm has a copy too\.$/);
    const inspector = await openDetails("Claude Code");
    expect(inspector.querySelector("[data-commands]")).toBeNull();
  });

  it("says a tool is installed twice before where its update stands, after what its source allows", async () => {
    serveBoth(nativeClaude, npmClaude);
    served = {
      ...served,
      updates: [
        { ...snapshot.updates[0], key: npmClaude.key, checkable: false, warnings: [{ Message: "timed out" }] },
      ],
    };
    renderInstalled();

    // Not "Can't check": the Updates page says that one too.
    expect(chipsOf(await findRow("@anthropic-ai/claude-code"))).toEqual(["Installed twice"]);
  });

  it("lists the two rows a command's notice is about when its Show is pressed, and only those", async () => {
    // W2-9: the program npm put on the Mac under the name `claude` is found
    // by the command its facts name, not only by its package's name: here
    // a wrapper whose name says nothing of Claude.
    const wrapper: InstalledArtifact = {
      ...npmClaude,
      display_name: "cc-wrapper",
      key: { ...npmClaude.key, name: "cc-wrapper" },
      facts: { ...NO_FACTS, family: null, commands: [{ name: "claude", state: "Runs" }] },
    };
    const shadowed = { ...claudeInstance, status: { unavailable: null, notes: ["ShadowedByNpm" as const] } };
    serveBoth(nativeClaude, wrapper);
    served = { ...served, instances: [brew, npm, shadowed] };
    renderInstalled();

    await findRow("Claude Code");
    expect(rowNames().length).toBeGreaterThan(2);
    fireEvent.click(await screen.findByRole("button", { name: "Show “claude”" }));

    expect(useUiStore.getState().query).toBe("claude");
    await waitFor(() => expect(rowNames().sort()).toEqual(["Claude Code", "cc-wrapper"]));
  });

  it("leaves out the source's own PATH notice where the group says which copy runs", async () => {
    const shadowed = { ...claudeInstance, status: { unavailable: null, notes: ["ShadowedByNpm" as const] } };
    serveBoth(nativeClaude, npmClaude);
    served = { ...served, instances: [brew, npm, shadowed] };
    const { unmount } = renderInstalled();
    let inspector = await openDetails("Claude Code");
    expect(inspector.querySelector("[data-commands]")).toHaveTextContent("Runs the copy from npm");
    expect(inspector).not.toHaveTextContent("Typing claude in Terminal runs a program with that name from npm");
    unmount();

    // Nothing said about `claude`: the notice is all there is, and stays.
    serveBoth(
      { ...nativeClaude, facts: { ...NO_FACTS, family: "claude-code", commands: [{ name: "claude", state: null }] } },
      npmClaude,
    );
    served = { ...served, instances: [brew, npm, shadowed] };
    renderInstalled();
    inspector = await openDetails("Claude Code");
    expect(inspector).toHaveTextContent("Typing claude in Terminal runs a program with that name from npm");
  });

  it("leaves the note out of the list's notices too, where the rows say Installed twice (r24 W8)", async () => {
    const shadowed = { ...claudeInstance, status: { unavailable: null, notes: ["ShadowedByNpm" as const] } };
    serveBoth(nativeClaude, npmClaude);
    served = { ...served, instances: [brew, npm, shadowed] };
    renderInstalled();

    const native = await findRow("Claude Code");
    expect(chipsOf(native)).toEqual(["Installed twice"]);
    expect(screen.queryByText("Typing claude in Terminal runs a program with that name from npm")).toBeNull();
    expect(screen.queryByText(/Couldn't confirm whether it's another copy/)).toBeNull();
  });

  it("pairs an AI tool's formula Homebrew didn't link with npm's copy, and says it isn't linked (r36 V5)", async () => {
    // `npm i -g @google/gemini-cli` under Homebrew's node, then `brew
    // install gemini-cli`: its link step stopped at npm's `bin/gemini`, so
    // its commands come from its keg, with no verdict.
    const npmGemini: InstalledArtifact = {
      ...npmClaude,
      display_name: "@google/gemini-cli",
      key: { instance_id: npm.id, kind: "Package", name: "@google/gemini-cli" },
      facts: { ...NO_FACTS, family: "gemini-cli", commands: [{ name: "gemini", state: "Runs" }] },
    };
    const formulaGemini: InstalledArtifact = {
      ...formula("gemini-cli"),
      facts: {
        ...NO_FACTS,
        family: "gemini-cli",
        commands: [{ name: "gemini", state: null }],
        unlinked: true,
      },
    };
    served = {
      ...snapshot,
      instances: [brew, npm],
      artifacts: [...snapshot.artifacts, npmGemini, formulaGemini],
      updates: [],
    };
    renderInstalled();

    const row = await findRow("gemini-cli");
    expect(chipsOf(row)).toEqual(["Installed twice"]);
    expect(chipsOf(rowOf("@google/gemini-cli"))).toEqual(["Installed twice"]);
    expect(chipDetail(row, "Installed twice")).toHaveTextContent(
      "npm has a copy too.Homebrew didn't link this copy where Terminal looks.",
    );
    expect(chipDetail(rowOf("@google/gemini-cli"), "Installed twice")).toHaveTextContent(
      "Homebrew has a copy too.Typing gemini in Terminal runs this copy.",
    );
    const inspector = await openDetails("gemini-cli");
    expect([...inspector.querySelectorAll("[data-command-line]")].map((line) => line.textContent?.trim())).toEqual([
      "geminiHomebrew didn't link it where Terminal looks",
    ]);
  });

  it("leaves the note on grok out where its rows say Installed twice, though Cursor's agent comes first (r36 V3)", async () => {
    // Grok Build from Homebrew's cask (`grok` and `agent`) and from its own
    // installer, with Cursor's `~/.local/bin/agent` first on PATH: `grok`
    // runs the cask's copy, `agent` Cursor's, for both copies.
    const caskGrok: InstalledArtifact = {
      ...formula("grok-build"),
      key: { instance_id: brew.id, kind: "Cask", name: "grok-build" },
      facts: {
        ...NO_FACTS,
        family: "grok-build",
        commands: [
          { name: "agent", state: { ShadowedBy: { by: null } } },
          { name: "grok", state: "Runs" },
        ],
      },
    };
    const grokInstance: ManagerInstance = {
      ...claudeInstance,
      id: "standalone-grok",
      adapter_id: "standalone-grok",
      exe_path: "/Users/someone/.grok/bin/grok",
      prefix: "/Users/someone/.grok",
      version: "1.0.41",
      status: { unavailable: null, notes: ["ShadowedByHomebrew"] },
    };
    const ownGrok: InstalledArtifact = {
      ...claudeArtifact,
      key: { instance_id: grokInstance.id, kind: "Binary", name: "grok" },
      display_name: "Grok Build",
      version: "1.0.41",
      homepage: null,
      path: "/Users/someone/.grok/downloads/grok-1.0.41-macos-aarch64",
      facts: {
        ...NO_FACTS,
        family: "grok-build",
        commands: [
          { name: "agent", state: { ShadowedBy: { by: null } } },
          { name: "grok", state: { ShadowedBy: { by: caskGrok.key } } },
        ],
      },
    };
    served = {
      ...snapshot,
      instances: [brew, grokInstance],
      artifacts: [...snapshot.artifacts, caskGrok, ownGrok],
      updates: [],
    };
    renderInstalled();

    expect(chipsOf(await findRow("Grok Build"))).toEqual(["Installed twice"]);
    expect(chipsOf(rowOf("grok-build"))).toEqual(["Installed twice"]);
    expect(screen.queryByText("Typing grok in Terminal runs a program with that name from Homebrew")).toBeNull();
    expect(screen.queryByText(/Couldn't confirm whether it's another copy/)).toBeNull();
    // What the details say instead, a line per verdict.
    const inspector = await openDetails("Grok Build");
    expect([...inspector.querySelectorAll("[data-command-line]")].map((line) => line.textContent?.trim())).toEqual([
      "agentRuns another program with this name",
      "grokRuns the copy from Homebrew",
    ]);
  });

  it("names the folder of a copy Terminal cannot find, and marks no tool with only one copy", async () => {
    const formulaGrok: InstalledArtifact = {
      ...formula("grok"),
      facts: { ...NO_FACTS, family: null, commands: [{ name: "grok", state: "Runs" }] },
    };
    served = {
      ...snapshot,
      instances: [brew, claudeInstance],
      artifacts: [
        formulaGrok,
        {
          ...claudeArtifact,
          facts: {
            ...NO_FACTS,
            family: "claude-code",
            commands: [{ name: "claude", state: { NotOnPath: { dir: "~/.local/bin" } } }],
          },
        },
      ],
      updates: [],
    };
    renderInstalled();

    // No 「装了两份」 for a tool with one copy; its word is that Terminal
    // does not find it (`notOnPathChip`).
    expect(chipsOf(await findRow("Claude Code"))).toEqual(["Not Found in Terminal"]);
    expect(chipsOf(rowOf("grok"))).toEqual([]);
    const inspector = await openDetails("Claude Code");
    expect(inspector.querySelector("[data-command-line]")).toHaveTextContent(
      "Terminal can't find it: it's in ~/.local/bin, a folder Terminal doesn't search",
    );
    expect(within(inspector).getByRole("button", { name: "Copy path: ~/.local/bin" })).toBeInTheDocument();
  });
});
