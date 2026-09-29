import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { invoke, type InvokeArgs } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { InstalledPage } from "./InstalledPage";
import { BUTTON } from "../components/ui/controls";
import { UpdatesPage } from "./UpdatesPage";
import { SnapshotStatus } from "../components/SnapshotStatus";
import { useUiStore } from "../store/ui";
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

const mockInvoke = vi.mocked(invoke);

const brew: ManagerInstance = {
  id: "brew:/opt/homebrew",
  adapter_id: "brew",
  exe_path: "/opt/homebrew/bin/brew",
  prefix: "/opt/homebrew",
  scope: "User",
  version: "7.0.3",
  status: { unavailable: null, notes: [] },
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
// first that is not a normal state, and the details every one -- read
// from its drawer, which is closed again.
async function drawerChips(name: string): Promise<string[]> {
  const drawer = await openDetails(name);
  const list = drawer.querySelector("[data-status-list]");
  const words = list === null ? [] : [...list.children].map((item) => item.firstElementChild?.textContent ?? "");
  fireEvent.keyDown(drawer, { key: "Escape" });
  await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  return words;
}

// Opens the chip called `label` on `row`, and returns what it shows.
function chipDetail(row: HTMLElement, label: string): HTMLElement {
  const chip = within(row).getByRole("button", { name: label });
  fireEvent.click(chip);
  const panel = document.getElementById(chip.getAttribute("aria-controls") ?? "");
  if (panel === null) throw new Error(`the ${label} chip opened nothing`);
  return panel;
}

// A paragraph whose whole text, across the `<code>` a command is set in,
// is `text`.
function wholeSentence(text: string) {
  return (_content: string, element: Element | null) =>
    element?.tagName === "P" && element.textContent === text;
}

// Opens a confirmation's "Show Command": it is one press away.
function showCommand(dialog: HTMLElement) {
  const disclosure = within(dialog).getByRole("button", { name: /^Show Command/ });
  if (disclosure.getAttribute("aria-expanded") !== "true") fireEvent.click(disclosure);
}

// Presses `name`'s row itself, and returns the drawer that opens.
async function openDetails(name: string): Promise<HTMLElement> {
  const row = await findRow(name);
  fireEvent.click(within(row).getByRole("button", { name: `Details: ${name}` }));
  return screen.findByRole("dialog", { name });
}

describe("InstalledPage", () => {
  it("shows the first check's spinner and why it takes a while until the backend has answered, not Loading…", async () => {
    // At launch `get_snapshot` has not answered yet: the first check is
    // under way, as the Overview says in the same words.
    const answer = mockInvoke.getMockImplementation();
    mockInvoke.mockImplementation((cmd: string, args?: InvokeArgs) =>
      cmd === "get_snapshot" ? new Promise(() => {}) : answer!(cmd, args),
    );
    const { findByRole, getByText, queryByText, container } = renderWithProviders(<InstalledPage />);

    expect(await findByRole("heading", { level: 2, name: "Checking…" })).toBeInTheDocument();
    expect(
      getByText("The first check looks up every tool's newest version online, and sometimes takes a minute or two."),
    ).toBeInTheDocument();
    expect(container.querySelector("[data-first-check] svg")).toHaveAttribute("width", "32");
    expect(queryByText("Loading…")).not.toBeInTheDocument();
  });

  it("shows the requested artifact and folds the one other software brought in into a line", async () => {
    const { findByText, queryByText, getByRole } = renderWithProviders(<InstalledPage />);

    await findByText("jq");
    expect(queryByText("glib")).not.toBeInTheDocument();
    expect(getByRole("button", { name: /^1 more component came with other software/ })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
  });

  it("unfolds the component under its line, and folds it back", async () => {
    const { findByText, getByRole, queryByText } = renderWithProviders(<InstalledPage />);

    await findByText("jq");
    fireEvent.click(getByRole("button", { name: /^1 more component came with other software/ }));
    await findByText("glib");
    // Under its line, which stays to fold it back up.
    const fold = getByRole("button", { name: /^Hide 1 component/ });
    expect(fold).toHaveAttribute("aria-expanded", "true");
    expect(rowNames()).toEqual(["jq", "glib"]);
    expect(
      fold.compareDocumentPosition(rowOf("glib")) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();

    fireEvent.click(fold);
    await waitFor(() => expect(queryByText("glib")).not.toBeInTheDocument());
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
    const { findByText, queryByText, getByRole, getByText } = renderWithProviders(<InstalledPage />);

    await findByText("jq");
    const search = getByRole("searchbox", { name: "Search installed tools" });
    expect(search).toHaveAttribute("placeholder", "Search");

    fireEvent.change(search, { target: { value: "VISUAL-studio" } });
    await waitFor(() => expect(rowNames()).toEqual(["Microsoft Visual Studio Code"]));

    fireEvent.change(search, { target: { value: "nonexistent" } });
    await waitFor(() => expect(queryByText("jq")).not.toBeInTheDocument());
    expect(getByText("Nothing matches “nonexistent”")).toBeInTheDocument();
  });

  it("checks no spelling in the search box: a tool's name is no word", async () => {
    const { findByRole } = renderWithProviders(<InstalledPage />);

    expect(await findByRole("searchbox", { name: "Search installed tools" })).toHaveAttribute("spellcheck", "false");
  });

  it("opens the uninstall dialog and plans it when the row's Uninstall is pressed", async () => {
    const { findByRole } = renderWithProviders(<InstalledPage />);

    const jq = await findRow("jq");
    fireEvent.click(within(jq).getByRole("button", { name: "Uninstall…" }));

    const dialog = await findByRole("dialog", { name: "Uninstall “jq”?" });
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
    expect(within(dialog).getByText("/opt/homebrew/bin/brew uninstall --formula jq")).toBeInTheDocument();
    // The row's button, not the row: no details drawer under the dialog.
    expect(screen.queryByRole("dialog", { name: "jq" })).toBeNull();
  });

  it("offers Uninstall as a grey button, on the row and in the drawer, where the drawer's Update is the default", async () => {
    // Uninstall must not look like the thing to do, nor like a warning: a
    // grey button with nothing red about it, under the pointer or not
    // (`RowAction`).
    renderWithProviders(<InstalledPage />);

    const rowUninstall = within(await findRow("jq")).getByRole("button", { name: "Uninstall…" });
    expect(rowUninstall.className).toBe(BUTTON.regular.grey);
    expect(rowUninstall.className).not.toMatch(/danger|accent/);

    fireEvent.click(screen.getByRole("button", { name: /^1 more component came with other software/ }));
    const drawer = await openDetails("glib");
    const drawerUninstall = within(drawer).getByRole("button", { name: "Uninstall…" });
    expect(drawerUninstall.className).toBe(BUTTON.large.grey);
    expect(drawerUninstall.className).not.toMatch(/danger|accent/);
    expect(within(drawer).getByRole("button", { name: "Update" }).className).toBe(BUTTON.large.default);
  });

  it("gives the focus back to the row's Uninstall when its confirmation is cancelled", async () => {
    renderWithProviders(<InstalledPage />);

    const uninstall = within(await findRow("jq")).getByRole("button", { name: "Uninstall…" });
    fireEvent.click(uninstall);
    const dialog = await screen.findByRole("dialog", { name: "Uninstall “jq”?" });
    await waitFor(() => expect(document.activeElement).toBe(within(dialog).getByRole("button", { name: "Cancel" })));

    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await waitFor(() => expect(document.activeElement).toBe(uninstall));
    expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "submit_operation")).toHaveLength(0);
  });

  it("opens the log once an uninstall has started and the focus is back on the row's Uninstall", async () => {
    // The log drawer gives the focus back to what had it as it opened: so
    // it opens after the sheet has handed the focus back, and closing it
    // lands on the row's Uninstall, where the user began.
    renderWithProviders(<InstalledPage />);

    const uninstall = within(await findRow("jq")).getByRole("button", { name: "Uninstall…" });
    fireEvent.click(uninstall);
    const dialog = await screen.findByRole("dialog", { name: "Uninstall “jq”?" });
    const confirm = within(dialog).getByRole("button", { name: "Uninstall" });
    await waitFor(() => expect(confirm).toBeEnabled());
    fireEvent.click(confirm);

    await waitFor(() => expect(useUiStore.getState().drawerOpen).toBe(true));
    expect(useUiStore.getState().focusedOpId).toBe(7);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(uninstall);
  });

  it("disables the dialog's confirm button when the plan reports dependents", async () => {
    planAffected = ["jq-cli-wrapper"];
    const { findByRole } = renderWithProviders(<InstalledPage />);

    fireEvent.click(within(await findRow("jq")).getByRole("button", { name: "Uninstall…" }));

    const dialog = await findByRole("dialog", { name: "Uninstall “jq”?" });
    await within(dialog).findByText("jq-cli-wrapper");
    expect(within(dialog).getByRole("button", { name: "Uninstall" })).toBeDisabled();
  });

  it("says in a line that a source's version has not been tested, with why behind Details", async () => {
    served = { ...snapshot, instances: [{ ...brew, unverified_version: "99.9.9" }] };
    const { findByText, getByRole } = renderWithProviders(<InstalledPage />);

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
    renderWithProviders(<InstalledPage />);

    await findRow("Claude Code");
    expect(screen.getByText("uv isn't responding")).toBeInTheDocument();
    expect(screen.queryByText("Typing claude runs a same-named program from npm first")).toBeNull();
    expect(screen.queryByText("Claude Code 2.1.290 not tested")).toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "2 More Issues" }));

    const fewer = screen.getByRole("button", { name: "Show Fewer" });
    const lines = document.getElementById(fewer.getAttribute("aria-controls") ?? "");
    if (lines === null) throw new Error("Show fewer controls nothing");
    expect(
      within(lines)
        .getAllByRole("button", { name: /^Details: / })
        .map((button) => button.getAttribute("aria-label")),
    ).toEqual([
      "Details: Typing claude runs a same-named program from npm first",
      "Details: uv isn't responding",
      "Details: Claude Code 2.1.290 not tested",
    ]);
  });

  it("offers no Uninstall on a pinned package, keeps its description, and says how to release the pin behind its chip", async () => {
    // `brew uninstall jq` refuses a pinned formula without `--force` and
    // still exits 0 (`UninstallBlocked::Pinned` in
    // crates/canager-core/src/model.rs), so the row must not offer it. jq
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
        }),
        formula("wget", { description: "Internet file retriever" }),
      ],
      updates: [],
    };
    const { getAllByRole } = renderWithProviders(<InstalledPage />);

    await findRow("wget");
    // Only wget's.
    expect(getAllByRole("button", { name: "Uninstall…" })).toHaveLength(1);
    expect(within(rowOf("wget")).getByRole("button", { name: "Uninstall…" })).toBeInTheDocument();
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
    const { queryAllByRole } = renderWithProviders(<InstalledPage />);

    const jq = await findRow("jq");
    expect(queryAllByRole("button", { name: "Uninstall…" })).toHaveLength(0);
    const detail = chipDetail(jq, "Pinned");
    expect(detail).toHaveTextContent(
      "It's pinned in Homebrew. To uninstall it, first run /opt/homebrew/bin/brew unpin jq in Terminal.",
    );
    expect(detail.textContent).not.toMatch(/next time|at the latest|answers/);
  });

  it("offers no Uninstall on a tool with no safe uninstall method, and says so behind its chip without a command", async () => {
    // `UninstallBlocked::NoSafeMethod` (phase 4): the tool has no
    // uninstall command and Canager has no safe way yet to remove its
    // files, so the row hides the button and its chip says why -- and,
    // unlike a pin, sets no command as code, because there is nothing to
    // run first. `Session::issue_plan` refuses it in Rust too.
    served = {
      ...snapshot,
      instances: [claudeInstance],
      artifacts: [{ ...claudeArtifact, uninstall_blocked: "NoSafeMethod" }],
      updates: [],
    };
    const { queryAllByRole, container } = renderWithProviders(<InstalledPage />);

    const claude = await findRow("Claude Code");
    expect(queryAllByRole("button", { name: "Uninstall…" })).toHaveLength(0);
    expect(chipDetail(claude, "Uninstall by hand")).toHaveTextContent(
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
        }),
      ],
      updates: [],
    };
    const { queryAllByRole, container } = renderWithProviders(<InstalledPage />);

    const ruff = await findRow("ruff");
    expect(queryAllByRole("button", { name: "Uninstall…" })).toHaveLength(0);
    expect(chipDetail(ruff, "Can't uninstall here")).toHaveTextContent(
      "With UV_TOOL_DIR set, when uv uninstalls its last tool it also deletes the folder above UV_TOOL_DIR and everything in it, if that folder holds no other folder. So no uv tool can be uninstalled here while it's set.",
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
    const { queryAllByRole, queryByText } = renderWithProviders(<InstalledPage />);

    const claude = await findRow("Claude Code");
    expect(within(claude).getByText("Anthropic's AI coding assistant")).toBeInTheDocument();
    expect(chipsOf(claude)).toContain("Uninstall by hand");
    expect(within(rowOf("jq")).getByText("Homebrew package")).toBeInTheDocument();
    expect(queryByText(/No description/)).toBeNull();
    // Only the Homebrew artifact may offer Uninstall.
    expect(queryAllByRole("button", { name: "Uninstall…" })).toHaveLength(1);
  });

  it("offers Uninstall on Claude Code, beside its summary, now that it has a path list", async () => {
    // Phase 4 step C: the standalone artifact carries no `uninstall_blocked`
    // once its recipe lists the paths to move, so the row shows its
    // summary and an Uninstall button like any other package's, and
    // `Session::issue_plan` lets the plan through (`blocked_uninstall` in
    // session/plans.rs refuses only an artifact that carries one).
    served = { ...snapshot, instances: [claudeInstance], artifacts: [claudeArtifact], updates: [] };
    const { getAllByRole, queryByText } = renderWithProviders(<InstalledPage />);

    const claude = await findRow("Claude Code");
    expect(within(claude).getByText("Anthropic's AI coding assistant")).toBeInTheDocument();
    expect(getAllByRole("button", { name: "Uninstall…" })).toHaveLength(1);
    expect(queryByText("Uninstall by hand")).toBeNull();
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
    const { queryByText } = renderWithProviders(<InstalledPage />);

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
      const { container } = renderWithProviders(<InstalledPage />);
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

      // The details say every word, each with its why.
      expect(await drawerChips("offered")).toEqual(["Update available"]);
      expect(await drawerChips("pinned-current")).toEqual(["Pinned", "Up to date"]);
      expect(await drawerChips("current")).toEqual(["Up to date"]);
      expect(await drawerChips("skipped-model")).toEqual(["Can't uninstall now", "Skipped the new version"]);
      const drawer = await openDetails("stopped-model");
      expect([...(drawer.querySelector("[data-status-list]")?.children ?? [])].map((item) => item.firstElementChild?.textContent)).toEqual([
        "Can't uninstall now",
        "Can't update now",
      ]);
      expect(drawer).toHaveTextContent("Ollama isn't running. Open it, then click Check Again.");
    });

    it("agrees with the Updates page's buttons row for row", async () => {
      const installed = renderWithProviders(<InstalledPage />);
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
            ? within(row).queryByRole("button", { name: "Update" }) !== null
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
      renderWithProviders(<InstalledPage />);

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
      // The two that did not answer say so in their own line, the second
      // folded behind the first (`SourceNotices`).
      expect(screen.getByText("pipx isn't responding")).toBeInTheDocument();
      fireEvent.click(screen.getByRole("button", { name: "1 More Issue" }));
      expect(screen.getByText("Ollama isn't running")).toBeInTheDocument();
    });

    it.each([
      // Its only chip is about its Uninstall, held while the list updates.
      ["IndexUpdating", "Homebrew is updating its software list", ["Can't uninstall now"]],
      ["IndexMayBeStale", "Couldn't update Homebrew's software list", []],
    ] as const)("says nothing about updates while Homebrew's list is %s, and the line says why", async (note, line, chips) => {
      served = { ...snapshot, instances: [{ ...brew, status: { unavailable: null, notes: [note] } }], updates: [] };
      const { queryByText } = renderWithProviders(<InstalledPage />);

      expect(chipsOf(await findRow("jq"))).toEqual(chips);
      expect(await drawerChips("jq")).toEqual(chips);
      expect(queryByText("Up to date")).toBeNull();
      expect(screen.getByText(line)).toBeInTheDocument();
    });

    it("says nothing about updates for a launcher left without its program: there was no version to check", async () => {
      served = {
        ...snapshot,
        instances: [{ ...claudeInstance, status: { unavailable: null, notes: ["LauncherOnly"] } }],
        artifacts: [{ ...claudeArtifact, version: "", path: null }],
        updates: [],
      };
      renderWithProviders(<InstalledPage />);

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
      "with Show apps that update themselves %s, says it over a self-updating or always-latest cask only when Homebrew checked it",
      async (includeSelfUpdating, leftOut) => {
        const cask = (name: string, over: Partial<InstalledArtifact> = {}) =>
          formula(name, { key: { instance_id: brew.id, kind: "Cask", name }, ...over });
        servedSettings = { ...settings, include_self_updating: includeSelfUpdating };
        served = {
          ...snapshot,
          artifacts: [cask("onyx"), cask("zoom", { auto_updates: true }), cask("chromium", { version: "latest" })],
          updates: [],
        };
        renderWithProviders(<InstalledPage />);

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
    const { queryByRole, queryAllByText } = renderWithProviders(<InstalledPage />);

    const requests = await findRow("requests");
    expect(queryByRole("button", { name: "Uninstall…" })).not.toBeInTheDocument();
    expect(chipsOf(requests)).toEqual(["View only"]);
    expect(chipDetail(requests, "View only")).toHaveTextContent(
      "You can only view pip installs here. Install Python tools with pipx or uv to update and uninstall them here.",
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
    const { queryAllByRole } = renderWithProviders(<InstalledPage />);

    const detail = chipDetail(await findRow("typescript"), "View only");
    expect(detail).toHaveTextContent(
      "npm keeps these in a folder your account can't change, so you can only view them. After you install Node with Homebrew, you can manage the npm packages you install with it here.",
    );
    expect(detail.textContent).not.toMatch(/pipx|uv/);
    expect(queryAllByRole("button", { name: "Uninstall…" })).toHaveLength(0);
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
    const { container } = renderWithProviders(<InstalledPage />);

    await findRow("jq");
    const slotAt = (index: number) => container.querySelector<HTMLElement>(`[data-index="${index}"]`);

    await waitFor(() => expect(slotAt(1)?.style.transform).toBe("translateY(128px)"));
    expect(slotAt(0)?.style.height).toBe("");
    expect(slotAt(1)?.style.height).toBe("");
  });

  it("names a silent source in a line at the top, and says it can't show what it has when it has no rows", async () => {
    // brew, npm, uv, pipx and cargo can all report `NotResponding`,
    // and it means the same thing for all five: the CLI is on PATH but
    // Canager could not talk to it. The backend keeps such an instance in
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
    const { findByText, getByRole, queryByText } = renderWithProviders(<InstalledPage />);

    await findByText("jq");
    const line = await findByText("npm isn't responding");
    // Above the list, not in it.
    expect(line.closest("[data-index]")).toBeNull();
    const details = getByRole("button", { name: "Details: npm isn't responding" });
    fireEvent.click(details);
    // Nothing was carried forward for npm -- and nothing ever is on the
    // first refresh after a launch, because the snapshot is in memory
    // only (`Session::new` starts from `Snapshot::empty()`).
    expect(document.getElementById(details.getAttribute("aria-controls") ?? "")).toHaveTextContent(
      "npm didn't respond, so what it has installed can't be shown.",
    );
    expect(queryByText(/What's listed/)).not.toBeInTheDocument();
    // And no promise of a recovery that may never come.
    expect(queryByText(/Reopening Canager/)).not.toBeInTheDocument();
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
    const { getByRole } = renderWithProviders(<InstalledPage />);

    await findRow("jq");
    fireEvent.change(getByRole("searchbox", { name: "Search installed tools" }), {
      target: { value: "nothing-like-it" },
    });
    const details = await screen.findByRole("button", { name: "Details: Homebrew isn't responding" });
    fireEvent.click(details);
    expect(document.getElementById(details.getAttribute("aria-controls") ?? "")).toHaveTextContent(
      "What's listed for Homebrew is from the last time it responded, and later changes aren't shown. Click Check Again later.",
    );
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
      const { unmount, queryByText } = renderWithProviders(<InstalledPage />);

      const jq = await findRow("jq");
      expect(within(jq).getByText("1.8.2").className).toContain("tabular-nums");
      await findRow("qwen3:8b");
      expect(queryByText(/5642e97495e1a0888838/)).not.toBeInTheDocument();
      unmount();
    }
  });

  // Rendered through SnapshotStatus, exactly as App.tsx does. Rendering
  // InstalledPage on its own would bypass the gate the real app always goes
  // through, and this snapshot -- a stopped Ollama and nothing installed
  // anywhere -- is precisely the one that gate used to swallow.
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
    // forward, which is what makes the line's "what's listed for Ollama is
    // from the last time it responded" true instead of a sentence over no
    // rows. `ollama rm` against a daemon that is not listening cannot
    // succeed -- spec §2.5's conjunction, on the button rather than only in
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
    const held = within(model).getByRole("button", { name: "Uninstall…" });
    expect(held).toBeDisabled();
    fireEvent.click(held);
    expect(queryByRole("dialog")).toBeNull();
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
    renderWithProviders(<InstalledPage />);

    const row = await findRow("pre-commit");
    expect(within(row).getByRole("button", { name: "Uninstall…" })).toBeDisabled();
    expect(chipDetail(row, "Can't uninstall now")).toHaveTextContent(
      "uv isn't responding. Click Check Again later.",
    );
    fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });

    // The drawer says the same: Uninstall disabled, and why under the chip.
    const drawer = await openDetails("pre-commit");
    expect(within(drawer).getByRole("button", { name: "Uninstall…" })).toBeDisabled();
    expect(within(drawer).getByText("uv isn't responding. Click Check Again later.")).toBeInTheDocument();
  });

  it("keeps Homebrew's Uninstall disabled while it updates its list, says why behind a chip, and gives it back after", async () => {
    // Homebrew's uninstall preview is refused while its list is being
    // rewritten (`AdapterError::IndexUpdating` in
    // crates/canager-core/src/adapters/brew/mod.rs), so the button stays,
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
    const { queryClient } = renderWithProviders(<InstalledPage />);

    const jq = await findRow("jq");
    const held = within(jq).getByRole("button", { name: "Uninstall…" });
    expect(held).toBeDisabled();
    fireEvent.click(held);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(mockInvoke).not.toHaveBeenCalledWith("plan_operation", expect.anything());
    expect(chipsOf(jq)).toEqual(["Can't uninstall now"]);
    expect(chipDetail(jq, "Can't uninstall now")).toHaveTextContent(
      "Homebrew is updating its software list. Uninstall once it's done.",
    );
    // The page's own line says what Homebrew is doing.
    expect(screen.getByText("Homebrew is updating its software list")).toBeInTheDocument();
    expect(within(rowOf("Claude Code")).getByRole("button", { name: "Uninstall…" })).toBeEnabled();
    expect(chipsOf(rowOf("Claude Code"))).not.toContain("Can't uninstall now");

    // The drawer says the same: Uninstall disabled, and why under the chip.
    const drawer = await openDetails("jq");
    expect(within(drawer).getByRole("button", { name: "Uninstall…" })).toBeDisabled();
    expect(
      within(drawer).getByText("Homebrew is updating its software list. Uninstall once it's done."),
    ).toBeInTheDocument();
    fireEvent.click(within(drawer).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "jq" })).toBeNull());

    served = { ...updating, instances: [brew, claudeInstance] };
    await act(() => queryClient.invalidateQueries());
    await waitFor(() => expect(within(rowOf("jq")).getByRole("button", { name: "Uninstall…" })).toBeEnabled());
    // Its word goes with the hold; up to date goes without saying.
    expect(chipsOf(rowOf("jq"))).toEqual([]);
    fireEvent.click(within(rowOf("jq")).getByRole("button", { name: "Uninstall…" }));
    expect(await screen.findByRole("dialog", { name: "Uninstall “jq”?" })).toBeInTheDocument();
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
    const { findByRole, getByRole, queryByText } = renderWithProviders(<InstalledPage />);

    await findRow("requests");
    expect(queryByText("glib")).not.toBeInTheDocument();
    expect(queryByText("charset-normalizer")).not.toBeInTheDocument();

    const pipFold = getByRole("button", { name: "1 more component came with other software pip" });
    expect(getByRole("button", { name: "1 more component came with other software Homebrew" })).toBeInTheDocument();
    fireEvent.click(pipFold);

    await findRow("charset-normalizer");
    expect(queryByText("glib")).not.toBeInTheDocument();

    fireEvent.click(await findByRole("button", { name: "Hide 1 component pip" }));
    await waitFor(() => expect(queryByText("charset-normalizer")).not.toBeInTheDocument());
  });

  describe("filters", () => {
    const twoSources = (): Snapshot => ({
      ...snapshot,
      // A stopped Ollama with nothing installed: its line, but no filter.
      instances: [brew, pip, { ...ollama, status: { unavailable: "NotRunning", notes: [] } }],
      artifacts: [...snapshot.artifacts, ...pipSnapshot.artifacts],
    });

    it("offers All and one filter per source with something installed, each with how much, and shows one source at a time", async () => {
      served = twoSources();
      const { getByRole, queryByRole, findByRole } = renderWithProviders(<InstalledPage />);

      await findRow("requests");
      const group = getByRole("group", { name: "Filter by source" });
      // Named by their words and counts; the avatar's letter is decoration.
      const [all, homebrew, pipChip, ...rest] = within(group).getAllByRole("button");
      expect(rest).toEqual([]);
      expect(all).toBe(within(group).getByRole("button", { name: "All 3" }));
      expect(homebrew).toBe(within(group).getByRole("button", { name: "Homebrew 2" }));
      expect(pipChip).toBe(within(group).getByRole("button", { name: "pip 1" }));
      expect(within(group).getByRole("button", { name: "All 3" })).toHaveAttribute("aria-pressed", "true");
      expect(queryByRole("button", { name: /^Ollama/ })).toBeNull();
      // Every source's rows, each naming its source where it is not its own:
      // to a screen reader, and in its avatar's tooltip -- in sight only
      // where two sources list the same name (R3).
      expect(rowNames()).toEqual(["jq", "requests"]);
      expect(within(rowOf("jq")).getByText("Homebrew")).toHaveClass("sr-only");
      expect(within(rowOf("jq")).getByTitle("Homebrew")).toBeInTheDocument();

      fireEvent.click(within(group).getByRole("button", { name: "pip 1" }));
      await waitFor(() => expect(rowNames()).toEqual(["requests"]));
      expect(within(group).getByRole("button", { name: "pip 1" })).toHaveAttribute("aria-pressed", "true");
      expect(useUiStore.getState().installedFilter).toBe(pip.id);
      // One source: nothing in sight about it on its rows, and only its own lines.
      expect(within(rowOf("requests")).getByText("pip", { selector: "span" })).toHaveClass("sr-only");
      expect(screen.queryByText("Ollama isn't running")).toBeNull();

      fireEvent.click(within(group).getByRole("button", { name: "All 3" }));
      await waitFor(() => expect(rowNames()).toEqual(["jq", "requests"]));
      expect(await findByRole("button", { name: "Open Ollama" })).toBeInTheDocument();
    });

    it("opens on the source whatever opened the page asked for, as an Overview tile does", async () => {
      served = twoSources();
      useUiStore.getState().openInstalled(brew.id);
      const { getByRole } = renderWithProviders(<InstalledPage />);

      await findRow("jq");
      expect(rowNames()).toEqual(["jq"]);
      expect(getByRole("button", { name: "Homebrew 2" })).toHaveAttribute("aria-pressed", "true");
      expect(useUiStore.getState().page).toBe("installed");
    });

    describe("as one line", () => {
      // jsdom lays nothing out and has no `scrollIntoView`: the row's
      // widths are given, and each chip scrolled into view is noted.
      let scrolledIntoView: Element[];
      beforeEach(() => {
        scrolledIntoView = [];
        Element.prototype.scrollIntoView = function (this: Element, options?: boolean | ScrollIntoViewOptions) {
          expect(options).toEqual({ block: "nearest", inline: "nearest" });
          scrolledIntoView.push(this);
        };
      });
      afterEach(() => {
        delete (Element.prototype as Partial<Element>).scrollIntoView;
      });

      // The row as a browser would measure it: `scrollWidth` of chips in a
      // `clientWidth`-wide window onto them.
      function measureAs(row: HTMLElement, scrollWidth: number, clientWidth: number) {
        Object.defineProperty(row, "scrollWidth", { configurable: true, get: () => scrollWidth });
        Object.defineProperty(row, "clientWidth", { configurable: true, get: () => clientWidth });
      }

      it("keeps the filters on one line that scrolls sideways, with its scrollbar hidden and every chip whole", async () => {
        served = twoSources();
        const { getByRole } = renderWithProviders(<InstalledPage />);

        await findRow("requests");
        const group = getByRole("group", { name: "Filter by source" });
        expect(group.className).toContain("flex-nowrap");
        expect(group.className).not.toMatch(/(^|\s)flex-wrap(\s|$)/);
        expect(group.className).toContain("overflow-x-auto");
        expect(group.className).toContain("[scrollbar-width:none]");
        expect(group.className).toContain("[&::-webkit-scrollbar]:hidden");
        for (const chip of within(group).getAllByRole("button")) {
          expect(chip.className).toContain("shrink-0");
          expect(chip.className).toContain("whitespace-nowrap");
        }
        // Everything fits here: no fade either side.
        expect(group).not.toHaveAttribute("data-more-before");
        expect(group).not.toHaveAttribute("data-more-after");
      });

      it("fades the edge past which chips are hidden, and follows the row as it scrolls", async () => {
        served = twoSources();
        const { getByRole } = renderWithProviders(<InstalledPage />);

        await findRow("requests");
        const group = getByRole("group", { name: "Filter by source" });
        measureAs(group, 900, 500);

        fireEvent.scroll(group);
        expect(group).toHaveAttribute("data-more-after");
        expect(group).not.toHaveAttribute("data-more-before");

        group.scrollLeft = 200;
        fireEvent.scroll(group);
        expect(group).toHaveAttribute("data-more-before");
        expect(group).toHaveAttribute("data-more-after");

        group.scrollLeft = 400;
        fireEvent.scroll(group);
        expect(group).toHaveAttribute("data-more-before");
        expect(group).not.toHaveAttribute("data-more-after");
      });

      it("keeps every chip in the tab order, and scrolls one into view as it takes the focus", async () => {
        served = twoSources();
        const { getByRole } = renderWithProviders(<InstalledPage />);

        await findRow("requests");
        const group = getByRole("group", { name: "Filter by source" });
        const pipChip = within(group).getByRole("button", { name: "pip 1" });
        for (const chip of within(group).getAllByRole("button")) {
          expect(chip).not.toHaveAttribute("tabindex");
        }
        scrolledIntoView = [];

        act(() => pipChip.focus());
        expect(document.activeElement).toBe(pipChip);
        expect(scrolledIntoView).toEqual([pipChip]);
      });

      it("scrolls the chosen chip into view when the choice changes, as when a tile opens the page on it", async () => {
        served = twoSources();
        useUiStore.getState().openInstalled(pip.id);
        const { getByRole } = renderWithProviders(<InstalledPage />);

        await findRow("requests");
        const group = getByRole("group", { name: "Filter by source" });
        const pipChip = within(group).getByRole("button", { name: "pip 1" });
        expect(pipChip).toHaveAttribute("aria-pressed", "true");
        expect(scrolledIntoView).toContain(pipChip);

        scrolledIntoView = [];
        fireEvent.click(within(group).getByRole("button", { name: "All 3" }));
        await waitFor(() => expect(scrolledIntoView).toEqual([within(group).getByRole("button", { name: "All 3" })]));
      });

      it("moves sideways under a mouse's up-and-down wheel only while it has chips hidden", async () => {
        served = twoSources();
        const { getByRole } = renderWithProviders(<InstalledPage />);

        await findRow("requests");
        const group = getByRole("group", { name: "Filter by source" });
        measureAs(group, 500, 500);
        // Nothing hidden: the wheel is the page's.
        expect(fireEvent.wheel(group, { deltaY: 120 })).toBe(true);
        expect(group.scrollLeft).toBe(0);

        measureAs(group, 900, 500);
        expect(fireEvent.wheel(group, { deltaY: 120 })).toBe(false);
        expect(group.scrollLeft).toBe(120);
        // A sideways swipe is the trackpad's own.
        expect(fireEvent.wheel(group, { deltaX: 40, deltaY: 5 })).toBe(true);
        expect(group.scrollLeft).toBe(120);
      });
    });

    it("stays on a source with nothing to list, and says why in its notice's words, with Check Again", async () => {
      // Opened from the sidebar's row for a stopped Ollama: never reset
      // to every source's list behind the user's back (spec R8).
      served = twoSources();
      useUiStore.getState().openInstalled(OLLAMA);
      const { findByText, getByText, getByRole } = renderWithProviders(<InstalledPage />);

      expect(await findByText("Ollama isn't running")).toBeInTheDocument();
      expect(getByText("Open Ollama to see what it has and check for updates.")).toBeInTheDocument();
      expect(rowNames()).toEqual([]);
      // Said once, where the list would be: no notice line over it too.
      expect(screen.getAllByText("Ollama isn't running")).toHaveLength(1);
      fireEvent.click(getByRole("button", { name: "Check Again" }));
      await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("refresh"));
      expect(useUiStore.getState().installedFilter).toBe(OLLAMA);
    });

    it("says a source that answered has nothing installed with it", async () => {
      const npm: ManagerInstance = { ...brew, id: "npm:/opt/homebrew", adapter_id: "npm" };
      served = { ...snapshot, instances: [brew, npm] };
      useUiStore.getState().openInstalled(npm.id);
      const { findByText, getByText, getByRole } = renderWithProviders(<InstalledPage />);

      expect(await findByText("Nothing installed with npm")).toBeInTheDocument();
      expect(getByText("Tools you install with npm show up here.")).toBeInTheDocument();
      expect(getByRole("button", { name: "Check Again" })).toBeEnabled();
      expect(rowNames()).toEqual([]);
      expect(useUiStore.getState().installedFilter).toBe(npm.id);
    });

    it("drops the filter, and shows everything, only once its source is gone from this Mac", async () => {
      served = twoSources();
      useUiStore.getState().openInstalled("cargo:/Users/you/.cargo");

      renderWithProviders(<InstalledPage />);

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
    const { getByRole, queryAllByRole } = renderWithProviders(<InstalledPage />);

    await findRow("aria2");
    const sortBy = getByRole("group", { name: "Sort by" });
    expect(within(sortBy).getByRole("button", { name: "Name" })).toHaveAttribute("aria-pressed", "true");
    // By name, case aside, whichever source a row is from; no headings.
    expect(rowNames()).toEqual(["aria2", "black", "requests", "wget", "Zstd"]);
    expect(queryAllByRole("heading", { level: 2 })).toHaveLength(0);

    fireEvent.click(within(sortBy).getByRole("button", { name: "Source" }));
    await waitFor(() => expect(rowNames()).toEqual(["aria2", "wget", "Zstd", "black", "requests"]));
    expect(within(sortBy).getByRole("button", { name: "Source" })).toHaveAttribute("aria-pressed", "true");
    const [homebrewHeading, pipHeading, ...more] = queryAllByRole("heading", { level: 2 });
    expect(more).toEqual([]);
    expect(homebrewHeading).toBe(getByRole("heading", { level: 2, name: "Homebrew 3" }));
    expect(pipHeading).toBe(getByRole("heading", { level: 2, name: "pip 2" }));
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
    renderWithProviders(<InstalledPage />);
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
    fireEvent.click(screen.getByRole("button", { name: "pip 1" }));
    await waitFor(() => expect(rowNames()).toEqual(["black"]));
    expect(within(rowOf("black")).getByText("pip")).toHaveClass("sr-only");
  });

  describe("the details drawer", () => {
    it("opens from the row itself, not from its buttons, with all the row had no room for", async () => {
      served = {
        ...snapshot,
        artifacts: [{ ...snapshot.artifacts[0], uninstall_blocked: "Pinned" }],
        updates: [{ ...snapshot.updates[0], key: snapshot.artifacts[0].key, current: "1.8.2", target: "1.8.3", blocked: "Pinned" }],
      };
      renderWithProviders(<InstalledPage />);

      const jq = await findRow("jq");
      // The chip opens its own detail, not the drawer.
      chipDetail(jq, "Pinned");
      expect(screen.queryByRole("dialog")).toBeNull();

      const drawer = await openDetails("jq");
      expect(within(drawer).getByText("Homebrew")).toBeInTheDocument();
      expect(within(drawer).getByText("Lightweight and flexible command-line JSON processor")).toBeInTheDocument();
      expect(within(drawer).getByText("Version").nextElementSibling).toHaveTextContent("1.8.2");
      expect(within(drawer).getByText("Newer version").nextElementSibling).toHaveTextContent("1.8.3");
      // The chip's why, in full, with the command set as code.
      expect(
        within(drawer).getByText(
          wholeSentence("It's pinned in Homebrew. To uninstall it, first run /opt/homebrew/bin/brew unpin jq in Terminal."),
        ),
      ).toBeInTheDocument();
      // Pinned against its update too: said once.
      expect(within(drawer).getAllByText("Pinned")).toHaveLength(1);
      // Neither uninstalled nor updated from here.
      expect(within(drawer).queryByRole("button", { name: "Uninstall…" })).toBeNull();
      expect(within(drawer).queryByRole("button", { name: "Update" })).toBeNull();
    });

    it("closes with Escape and with its close button, giving the focus back to the row", async () => {
      renderWithProviders(<InstalledPage />);

      const jq = await findRow("jq");
      const rowButton = within(jq).getByRole("button", { name: "Details: jq" });
      let drawer = await openDetails("jq");
      await waitFor(() => expect(drawer.contains(document.activeElement)).toBe(true));

      fireEvent.keyDown(document.activeElement ?? drawer, { key: "Escape" });
      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
      await waitFor(() => expect(document.activeElement).toBe(rowButton));

      drawer = await openDetails("jq");
      fireEvent.click(within(drawer).getByRole("button", { name: "Close" }));
      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
      await waitFor(() => expect(document.activeElement).toBe(rowButton));
    });

    it("keeps Tab inside itself while it is open", async () => {
      renderWithProviders(<InstalledPage />);

      const drawer = await openDetails("jq");
      const close = within(drawer).getByRole("button", { name: "Close" });
      const uninstall = within(drawer).getByRole("button", { name: "Uninstall…" });
      await waitFor(() => expect(document.activeElement).toBe(close));
      // The page under it is out of reach.
      expect(screen.queryByRole("searchbox")).toBeNull();

      uninstall.focus();
      fireEvent.keyDown(uninstall, { key: "Tab" });
      expect(document.activeElement).toBe(close);
      fireEvent.keyDown(close, { key: "Tab", shiftKey: true });
      expect(document.activeElement).toBe(uninstall);
    });

    it("updates through the Updates page's own confirmation, where that page would, and shows the progress there", async () => {
      renderWithProviders(<InstalledPage />);

      await findRow("jq");
      fireEvent.click(screen.getByRole("button", { name: /^1 more component came with other software/ }));
      const drawer = await openDetails("glib");
      expect(within(drawer).getByText("Update available")).toBeInTheDocument();
      expect(within(drawer).getByText("Newer version").nextElementSibling).toHaveTextContent("2.90.0");

      fireEvent.click(within(drawer).getByRole("button", { name: "Update" }));
      const confirm = await screen.findByRole("dialog", { name: "Update “glib”?" });
      expect(mockInvoke).toHaveBeenCalledWith("plan_operation", {
        request: { kind: "Upgrade", instance_id: "brew:/opt/homebrew", artifact_kind: "Formula", name: "glib" },
      });
      showCommand(confirm);
      expect(await within(confirm).findByText("/opt/homebrew/bin/brew upgrade --formula glib")).toBeInTheDocument();
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
      await waitFor(() => expect(screen.queryByRole("dialog", { name: "Update “glib”?" })).toBeNull());
      // Where the button was, as on the Updates page's row.
      const open = screen.getByRole("dialog", { name: "glib" });
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
          outcome: { Failed: { exit_code: 1, summary: "Error: glib: no bottle" } },
          argv_preview: ["/opt/homebrew/bin/brew", "upgrade", "glib"],
          cancel_policy: "KillThenReconcile",
        },
      ];
      useUiStore.setState({ updateTargets: { 9: "2.90.0" } });
      renderWithProviders(<InstalledPage />);

      await findRow("jq");
      fireEvent.click(screen.getByRole("button", { name: /^1 more component came with other software/ }));
      const drawer = await openDetails("glib");
      expect(await within(drawer).findByText("Couldn't update")).toBeInTheDocument();
      expect(within(drawer).getByRole("button", { name: "View log: glib" })).toBeInTheDocument();
      expect(within(drawer).queryByRole("button", { name: "Update" })).toBeNull();

      fireEvent.click(within(drawer).getByRole("button", { name: "Retry" }));
      await screen.findByRole("dialog", { name: "Update “glib”?" });
      expect(mockInvoke).toHaveBeenCalledWith("plan_operation", {
        request: { kind: "Upgrade", instance_id: "brew:/opt/homebrew", artifact_kind: "Formula", name: "glib" },
      });
    });

    it("shows an update that could not start in its own tool's drawer, not in another's", async () => {
      const answer = mockInvoke.getMockImplementation()!;
      mockInvoke.mockImplementation((cmd, args) =>
        cmd === "plan_operation" ? Promise.reject("brew is busy") : answer(cmd, args),
      );
      renderWithProviders(<InstalledPage />);

      await findRow("jq");
      fireEvent.click(screen.getByRole("button", { name: /^1 more component came with other software/ }));
      const glib = await openDetails("glib");
      fireEvent.click(within(glib).getByRole("button", { name: "Update" }));
      expect(await within(glib).findByText(/^Couldn't prepare the update/)).toBeInTheDocument();

      fireEvent.click(within(glib).getByRole("button", { name: "Close" }));
      await waitFor(() => expect(screen.queryByRole("dialog", { name: "glib" })).toBeNull());
      const jq = await openDetails("jq");
      expect(within(jq).queryByText(/^Couldn't prepare the update/)).toBeNull();
    });

    it.each([
      ["Queued", "Queued"],
      ["Running", "Uninstalling…"],
    ] as const)("offers no second Uninstall while one is %s, on the row and in the drawer", async (status, label) => {
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
      renderWithProviders(<InstalledPage />);

      const row = await findRow("jq");
      expect(await within(row).findByRole("button", { name: label })).toBeDisabled();
      expect(within(row).queryByRole("button", { name: "Uninstall…" })).toBeNull();
      const drawer = await openDetails("jq");
      expect(within(drawer).getByRole("button", { name: label })).toBeDisabled();
      expect(within(drawer).queryByRole("button", { name: "Uninstall…" })).toBeNull();
    });

    it("offers no Update for an update the Updates page does not offer", async () => {
      served = {
        ...snapshot,
        instances: [{ ...brew, status: { unavailable: "NotResponding", notes: [] } }],
      };
      renderWithProviders(<InstalledPage />);

      await findRow("jq");
      fireEvent.click(screen.getByRole("button", { name: /^1 more component came with other software/ }));
      const drawer = await openDetails("glib");
      expect(within(drawer).getByText("Can't update now")).toBeInTheDocument();
      expect(within(drawer).getByText("Can't uninstall now")).toBeInTheDocument();
      // What to do, under each of the two chips.
      expect(within(drawer).getAllByText("Homebrew isn't responding. Click Check Again later.")).toHaveLength(2);
      // Its source's own line, whole.
      expect(within(drawer).getByText("Homebrew isn't responding")).toBeInTheDocument();
      expect(
        within(drawer).getByText(
          "What's listed for Homebrew is from the last time it responded, and later changes aren't shown. Click Check Again later.",
        ),
      ).toBeInTheDocument();
      expect(within(drawer).queryByRole("button", { name: "Update" })).toBeNull();
      expect(within(drawer).getByRole("button", { name: "Uninstall…" })).toBeDisabled();
    });

    it("uninstalls through the row's own dialog, then gives way to the log", async () => {
      renderWithProviders(<InstalledPage />);

      const rowButton = within(await findRow("jq")).getByRole("button", { name: "Details: jq" });
      const drawer = await openDetails("jq");
      fireEvent.click(within(drawer).getByRole("button", { name: "Uninstall…" }));
      const dialog = await screen.findByRole("dialog", { name: "Uninstall “jq”?" });
      await within(dialog).findByRole("button", { name: "Show Command" });
      showCommand(dialog);
      expect(within(dialog).getByText("/opt/homebrew/bin/brew uninstall --formula jq")).toBeInTheDocument();

      fireEvent.click(within(dialog).getByRole("button", { name: "Uninstall" }));
      await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("submit_operation", { planId: "1" }));
      // The drawer closes for the log drawer, which it would otherwise
      // keep out of reach.
      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
      await waitFor(() => expect(useUiStore.getState().drawerOpen).toBe(true));
      expect(useUiStore.getState().focusedOpId).toBe(7);
      // What the log drawer will give the focus back to when it closes: the
      // row that opened the details, not a button that went with them.
      await waitFor(() => expect(document.activeElement).toBe(rowButton));
    });

    it("gives the focus back to its own Uninstall and Update when their confirmations are dismissed", async () => {
      renderWithProviders(<InstalledPage />);

      await findRow("jq");
      fireEvent.click(screen.getByRole("button", { name: /^1 more component came with other software/ }));
      const drawer = await openDetails("glib");
      const uninstall = within(drawer).getByRole("button", { name: "Uninstall…" });
      const update = within(drawer).getByRole("button", { name: "Update" });

      fireEvent.click(uninstall);
      await screen.findByRole("dialog", { name: "Uninstall “glib”?" });
      fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });
      await waitFor(() => expect(screen.queryByRole("dialog", { name: "Uninstall “glib”?" })).toBeNull());
      await waitFor(() => expect(document.activeElement).toBe(uninstall));
      // The drawer stays: only the question went.
      expect(screen.getByRole("dialog", { name: "glib" })).toBe(drawer);

      fireEvent.click(update);
      const confirm = await screen.findByRole("dialog", { name: "Update “glib”?" });
      fireEvent.click(within(confirm).getByRole("button", { name: "Cancel" }));
      await waitFor(() => expect(screen.queryByRole("dialog", { name: "Update “glib”?" })).toBeNull());
      await waitFor(() => expect(document.activeElement).toBe(update));
    });

    it("says where a tool is only with technical details on", async () => {
      served = { ...snapshot, instances: [claudeInstance], artifacts: [claudeArtifact], updates: [] };
      const first = renderWithProviders(<InstalledPage />);
      let drawer = await openDetails("Claude Code");
      expect(within(drawer).queryByText("Location")).toBeNull();
      first.unmount();

      servedSettings = { ...settings, show_technical_details: true };
      renderWithProviders(<InstalledPage />);
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
      renderWithProviders(<InstalledPage />);

      const drawer = await openDetails("Claude Code");
      const values = ["Version", "Newer version", "Location"].map(
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
        const first = renderWithProviders(<InstalledPage />);
        fireEvent.click(within(await findRow("jq")).getByRole("button", { name: "More actions for jq" }));
        let menu = screen.getByRole("menu");
        expect(within(menu).getAllByRole("menuitem").map((item) => item.textContent)).toEqual(["Details"]);
        fireEvent.click(within(menu).getByRole("menuitem", { name: "Details" }));
        expect(await screen.findByRole("dialog", { name: "jq" })).toBeInTheDocument();
        first.unmount();

        servedSettings = { ...settings, show_technical_details: true };
        renderWithProviders(<InstalledPage />);
        // A plain row has no command it could copy without a plan.
        fireEvent.click(within(await findRow("wget")).getByRole("button", { name: "More actions for wget" }));
        expect(within(screen.getByRole("menu")).getAllByRole("menuitem").map((item) => item.textContent)).toEqual([
          "Details",
        ]);
        fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });

        fireEvent.click(within(rowOf("jq")).getByRole("button", { name: "More actions for jq" }));
        menu = screen.getByRole("menu");
        fireEvent.click(within(menu).getByRole("menuitem", { name: "Copy Command" }));
        expect(writeText).toHaveBeenCalledWith("/opt/homebrew/bin/brew unpin jq");
        expect(await screen.findByRole("status")).toHaveTextContent("Copied");
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
      renderWithProviders(<InstalledPage />, { toolDescriptions: { "zh-CN": chinese() } });

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

    it("shows the line in a tool's details, with the source's own words under it, quieter", async () => {
      renderWithProviders(<InstalledPage />, { toolDescriptions: { "zh-CN": chinese() } });
      await findRow("jq");

      await inChinese(async () => {
        await within(rowOf("jq")).findByText("命令行 JSON 处理工具");
        fireEvent.click(within(rowOf("jq")).getByRole("button", { name: "详情：jq" }));
        const drawer = await screen.findByRole("dialog", { name: "jq" });
        const line = within(drawer).getByText("命令行 JSON 处理工具");
        const original = within(drawer).getByText(JQ);
        // Under the line, in the drawer's description, smaller and muted.
        expect(line.contains(original)).toBe(true);
        expect(line.firstChild?.textContent).toBe("命令行 JSON 处理工具");
        expect(original).toHaveAttribute("data-original-description");
        expect(original).toHaveClass("block", "text-small", "text-muted");
        fireEvent.click(within(drawer).getByRole("button", { name: "关闭" }));
        await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

        // prettier's source said nothing: its line alone.
        fireEvent.click(within(rowOf("prettier")).getByRole("button", { name: "详情：prettier" }));
        const prettier = await screen.findByRole("dialog", { name: "prettier" });
        expect(within(prettier).getByText("代码格式化工具")).toBeInTheDocument();
        expect(prettier.querySelector("[data-original-description]")).toBeNull();
        fireEvent.click(within(prettier).getByRole("button", { name: "关闭" }));
        await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
      });

      // In English: the source's words, once.
      const drawer = await openDetails("jq");
      expect(within(drawer).getAllByText(JQ)).toHaveLength(1);
      expect(within(drawer).queryByText("命令行 JSON 处理工具")).toBeNull();
      expect(drawer.querySelector("[data-original-description]")).toBeNull();
    });

    it("gives a package's row its line in English where the English table has one, and switches it with the language", async () => {
      renderWithProviders(<InstalledPage />, { toolDescriptions: { en: english(), "zh-CN": chinese() } });

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

    it("shows a package's line in English alone in its details: its source said nothing", async () => {
      renderWithProviders(<InstalledPage />, { toolDescriptions: { en: english() } });
      await within(await findRow("prettier")).findByText(PRETTIER);

      const drawer = await openDetails("prettier");
      expect(within(drawer).getByText(PRETTIER)).toBeInTheDocument();
      expect(within(drawer).queryByText("npm package")).toBeNull();
      expect(drawer.querySelector("[data-original-description]")).toBeNull();
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
      renderWithProviders(<InstalledPage />, { toolIcons });
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

    it("shows the source's logo on a tool with none of its own, on its filter and over its group, and the initial where the source has none", async () => {
      served = {
        ...snapshot,
        instances: [brew, pip],
        artifacts: [...snapshot.artifacts, ...pipSnapshot.artifacts],
      };
      renderWithProviders(<InstalledPage />, { toolIcons });

      fireEvent.click(await screen.findByRole("button", { name: /^1 more component came with other software/ }));
      const glib = await findRow("glib");
      expect(glib.querySelector(glyph(HOMEBREW))).not.toBeNull();
      expect(glib.querySelector("[data-source-badge]")).toBeNull();
      expect(within(await findRow("requests")).getByText("P")).toHaveAttribute("aria-hidden", "true");

      const filters = screen.getByRole("group", { name: "Filter by source" });
      expect(within(filters).getByRole("button", { name: "Homebrew 2" }).querySelector(glyph(HOMEBREW))).not.toBeNull();
      expect(within(within(filters).getByRole("button", { name: "pip 1" })).getByText("P")).toHaveAttribute(
        "aria-hidden",
        "true",
      );

      const sortBy = screen.getByRole("group", { name: "Sort by" });
      fireEvent.click(within(sortBy).getByRole("button", { name: "Source" }));
      const heading = await screen.findByRole("heading", { level: 2, name: "Homebrew 2" });
      expect(heading.querySelector(glyph(HOMEBREW))).not.toBeNull();
    });
  });
});
