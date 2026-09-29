import { afterEach, describe, expect, it, vi, beforeEach } from "vitest";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { invoke, type InvokeArgs } from "@tauri-apps/api/core";
import { renderWithProviders, type RenderOptions } from "../test/setup";
import { UpdatesToolbar } from "../test/updatesToolbar";
import { UpdatesPage } from "./UpdatesPage";
import { BUTTON } from "../components/ui/controls";
import { artifactKeyId, useUiStore } from "../store/ui";
import { queryKeys } from "../lib/queries";
import { loadToolIcons } from "../lib/toolIcons";
import { lazyDescriptionTable } from "../lib/toolDescriptions";
import i18n from "../i18n";
import en from "../i18n/en.json";
import zhCN from "../i18n/zh-CN.json";
import type {
  ArtifactKey,
  InstanceNote,
  OpRequest,
  OpSummary,
  Settings,
  Snapshot,
  Warning,
} from "../lib/types";

const mockInvoke = vi.mocked(invoke);

// The pretend Mac's two models (`MODELS` in src/dev/mockData.ts, which no
// test outside src/dev may import): one from another registry, one from
// Ollama's own.
const MODELS = {
  coder: "modelscope.cn/Qwen/Qwen2.5-Coder-7B-Instruct-GGUF:Q4_K_M",
  llama: "llama3.2:3b",
} as const;

// The page as the window draws it: under the toolbar's subtitle, which
// says how many can be updated, and with its Update all / Update selected
// in the toolbar (`UpdatesToolbar`).
function renderPage(options?: RenderOptions) {
  return renderWithProviders(
    <UpdatesToolbar>
      <UpdatesPage />
    </UpdatesToolbar>,
    options,
  );
}

const glibKey: ArtifactKey = { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "glib" };
const onyxKey: ArtifactKey = { instance_id: "brew:/opt/homebrew", kind: "Cask", name: "onyx" };
const myForkKey: ArtifactKey = {
  instance_id: "cargo:/Users/brulek/.cargo",
  kind: "Binary",
  name: "my-fork",
};
const qwenKey: ArtifactKey = {
  instance_id: "ollama:http://127.0.0.1:11434",
  kind: "Model",
  name: "qwen3:8b",
};
const urllib3Key: ArtifactKey = {
  instance_id: "pip:/usr/bin/python3",
  kind: "Package",
  name: "urllib3",
};
// An npm whose global prefix this account cannot write -- Node installed
// from nodejs.org's package. Read-only like pip, for an entirely different
// reason, and the advice that fixes one is nonsense for the other.
const typescriptKey: ArtifactKey = {
  instance_id: "npm:/usr/local",
  kind: "Package",
  name: "typescript",
};

const snapshot: Snapshot = {
  generation: 2,
  round: 2,
  detect: "Found",
  // A candidate's adapter is only reachable by joining its key's
  // `instance_id` back to the snapshot's instances, so the page needs real
  // ones: pip's candidates are genuinely checkable but can never be acted
  // on.
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
    {
      id: "pip:/usr/bin/python3",
      adapter_id: "pip",
      exe_path: "/usr/bin/python3",
      prefix: "/usr",
      scope: "User",
      version: "26.2.1",
      status: { unavailable: null, notes: [] },
      unverified_version: null,
      read_only_reason: "ByDesign",
    },
    {
      id: "npm:/usr/local",
      adapter_id: "npm",
      exe_path: "/usr/local/bin/npm",
      prefix: "/usr/local",
      scope: "User",
      version: "12.0.2",
      status: { unavailable: null, notes: [] },
      unverified_version: null,
      read_only_reason: "PrefixNotWritable",
    },
    // Writable and answering, so the cargo candidates below have a source
    // `refresh` could have produced them from.
    {
      id: "cargo:/Users/brulek/.cargo",
      adapter_id: "cargo",
      exe_path: "/Users/brulek/.cargo/bin/cargo",
      prefix: "/Users/brulek/.cargo",
      scope: "User",
      version: "1.92.0",
      status: { unavailable: null, notes: [] },
      unverified_version: null,
      read_only_reason: null,
    },
  ],
  artifacts: [],
  updates: [
    {
      key: glibKey,
      current: "2.88.3",
      target: "2.90.0",
      channel: "Native",
      checkable: true,
      warnings: [],
      blocked: null,
    },
    {
      key: onyxKey,
      current: "5.0.2",
      target: "5.1.0",
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

const stoppedOllama: Snapshot["instances"][number] = {
  id: "ollama:http://127.0.0.1:11434",
  adapter_id: "ollama",
  exe_path: "/usr/local/bin/ollama",
  prefix: "/usr/local",
  scope: "User",
  version: "0.34.1",
  status: { unavailable: "NotRunning", notes: [] },
  unverified_version: null,
  read_only_reason: null,
};

let settings: Settings;
let updates: Snapshot["updates"];
let instances: Snapshot["instances"];
// A row's name and its one line of description come from `artifacts`, so
// a test about what a row says needs them.
let artifacts: Snapshot["artifacts"];
// This round's failed calls; `stale` follows them, as `refresh` sets it.
let errors: Snapshot["errors"];
// What `list_operations` answers: the backend lists operations newest first.
let operations: OpSummary[];
// Every plan_operation answer carries a fresh server-issued id: a PlanId is
// single-use, so the multi-select tests below must prove that each submit
// sent a *different* id, not the same one twice.
let nextPlanId: number;
// Per-test knobs for the mocked backend. A name in `planFailures` makes its
// plan_operation reject with that text; a plan id in `submitFailures` makes
// its submit_operation reject; `saveFailure` makes set_settings reject. A
// name in `holdPlans`, an id in `holdSubmits`, or `holdSaves` keeps the
// reply pending until the test calls the matching `release*` function.
// Rejections are bare strings, exactly as a `Result<_, String>` command
// rejects; Task 10's `call()` turns them into Errors.
let planFailures: Record<string, string>;
let submitFailures: Record<string, string>;
let saveFailure: string | null;
let holdPlans: Set<string>;
let holdSubmits: Set<string>;
let holdSaves: boolean;
// Names whose plan comes back with `needs_password: true`, mirroring the
// brew adapter, which sets it for every Cask upgrade.
let needsPassword: Set<string>;
// Names whose plan comes back `NoCancel`, mirroring the rustup recipe's
// `self update` (crates/canager-core/src/adapters/standalone/recipes.rs).
let noCancel: Set<string>;
let planWarnings: Record<string, Warning[]>;
let releasePlan: Record<string, () => void>;
let releaseSubmit: Record<string, () => void>;
let releaseSave: Array<() => void>;

// `id` stays a number here purely so the tests can order plans ("the first
// issued", "the second issued"); the wire type is a string (a random
// 128-bit token, not a sequential counter -- see PlanId in
// crates/canager-core/src/session/mod.rs), so it is stringified going out.
function issuedPlanFor(request: OpRequest, id: number) {
  return {
    id: String(id),
    plan: {
      request,
      action: {
        Command: {
          program: "/opt/homebrew/bin/brew",
          args: ["upgrade", request.artifact_kind === "Cask" ? "--cask" : "--formula", request.name],
          env: [],
        },
      },
      needs_password: needsPassword.has(request.name),
      locks: ["brew:/opt/homebrew"],
      cancel_policy: noCancel.has(request.name) ? "NoCancel" : "KillThenReconcile",
      warnings: planWarnings[request.name] ?? [],
      affected: [],
      timeout_secs: 1800,
    },
    issued_at: 1758000000,
  };
}

function calls(cmd: string) {
  return mockInvoke.mock.calls.filter(([name]) => name === cmd);
}

function submittedPlanIds() {
  return calls("submit_operation").map(([, args]) => args);
}

function plannedNames(): string[] {
  return calls("plan_operation").map(([, args]) => (args as { request: OpRequest }).request.name);
}

// A row's checkbox, by its name ("Select glib for update"): not the list
// header's, which ticks them all.
const ROW_CHECKBOX = /^Select .+ for update$/;
// The list header's box, which ticks every row that has a checkbox; its
// accessible name goes on to say which rows it acts on.
const SELECT_ALL = "Select all items that can be updated here";

// The height every virtualized row reports back in jsdom.
const ROW_HEIGHT = 56;

// Which slot of the virtualized list `element` was drawn in, or null for
// something outside the list. The list is flat in the DOM -- every row and
// the "Can't update here" toggle is a sibling carrying its position as
// `data-index`.
function slotOf(element: HTMLElement): number | null {
  const slot = element.closest("[data-index]");
  return slot === null ? null : Number(slot.getAttribute("data-index"));
}

// A paragraph whose whole text, across the `<code>` the Updates page sets a
// command in, is `text`. `getByText` alone matches an element's own text
// nodes, which no longer hold the whole sentence.
function wholeSentence(text: string) {
  return (_content: string, element: Element | null) =>
    element?.tagName === "P" && element.textContent === text;
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

// Unfolds "N more can't be updated here".
async function showCantUpdate() {
  const toggle = await screen.findByRole("button", { name: /^\d+ more can't be updated here$/ });
  expect(toggle).toHaveAttribute("aria-expanded", "false");
  fireEvent.click(toggle);
}

// Opens the chip called `label` on `row`, and returns what it shows.
function chipDetail(row: HTMLElement, label: string): HTMLElement {
  const chip = within(row).getByRole("button", { name: label });
  fireEvent.click(chip);
  const panel = document.getElementById(chip.getAttribute("aria-controls") ?? "");
  if (panel === null) throw new Error(`the ${label} chip opened nothing`);
  return panel;
}

// Opens `row`'s ⋯ menu.
function openMenu(row: HTMLElement): HTMLElement {
  fireEvent.click(within(row).getByRole("button", { name: /^More actions for / }));
  return screen.getByRole("menu");
}

// Opens the confirmation's "Show the command(s)": the commands are one
// press away, not on the sheet.
function showCommands(dialog: HTMLElement) {
  const disclosure = within(dialog).getByRole("button", { name: /^Show Command/ });
  if (disclosure.getAttribute("aria-expanded") !== "true") fireEvent.click(disclosure);
}

// The names of the rows, top to bottom.
function rowNames(): string[] {
  return [...document.querySelectorAll("[data-tool-row]")].map(
    (row) => row.querySelector("p")?.textContent ?? "",
  );
}

function brewCandidate(name: string): Snapshot["updates"][number] {
  return {
    key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name },
    current: "1.0.0",
    target: "1.1.0",
    channel: "Native",
    checkable: true,
    warnings: [],
    blocked: null,
  };
}

function operation(key: ArtifactKey, fields: Partial<OpSummary> = {}): OpSummary {
  return {
    id: 7,
    kind: "Upgrade",
    instance_id: key.instance_id,
    artifact_kind: key.kind,
    name: key.name,
    status: "Running",
    outcome: null,
    argv_preview: ["/opt/homebrew/bin/brew", "upgrade", key.name],
    cancel_policy: "KillThenReconcile",
    ...fields,
  };
}

beforeEach(() => {
  settings = {
    language: "System",
    show_technical_details: false,
    ignored_updates: [],
    skipped_versions: [],
    include_self_updating: false,
    auto_check: false,
    notify_updates: false,
  };
  updates = snapshot.updates;
  instances = snapshot.instances;
  artifacts = snapshot.artifacts;
  errors = snapshot.errors;
  operations = [];
  nextPlanId = 1;
  planFailures = {};
  submitFailures = {};
  saveFailure = null;
  holdPlans = new Set();
  holdSubmits = new Set();
  holdSaves = false;
  needsPassword = new Set();
  noCancel = new Set();
  planWarnings = {};
  releasePlan = {};
  releaseSubmit = {};
  releaseSave = [];
  mockInvoke.mockReset();
  // @tanstack/react-virtual measures its scroll container and every row
  // through offsetWidth / offsetHeight, which jsdom hardcodes to 0 with no
  // layout engine behind them. Without these the virtualizer sees a
  // zero-height viewport and renders no rows at all. Same stubs as
  // InstalledPage.test.tsx, which virtualized first.
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (
    this: HTMLElement,
  ) {
    return this.getAttribute("data-index") === null ? 600 : ROW_HEIGHT;
  });
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
  mockInvoke.mockImplementation((cmd: string, args?: unknown) => {
    if (cmd === "get_snapshot")
      return Promise.resolve({
        ...snapshot,
        updates,
        instances,
        artifacts,
        errors,
        stale: errors.length > 0,
      });
    if (cmd === "get_settings") return Promise.resolve(settings);
    if (cmd === "list_operations") return Promise.resolve(operations);
    if (cmd === "set_settings") {
      if (saveFailure !== null) return Promise.reject(saveFailure);
      const next = (args as { settings: Settings }).settings;
      if (holdSaves) {
        return new Promise<void>((resolve) => {
          releaseSave.push(() => {
            settings = next;
            resolve();
          });
        });
      }
      settings = next;
      return Promise.resolve(undefined);
    }
    if (cmd === "plan_operation") {
      const request = (args as { request: OpRequest }).request;
      const failure = planFailures[request.name];
      if (failure !== undefined) {
        if (!holdPlans.has(request.name)) return Promise.reject(failure);
        return new Promise((_resolve, reject) => {
          releasePlan[request.name] = () => reject(failure);
        });
      }
      const issued = issuedPlanFor(request, nextPlanId);
      nextPlanId += 1;
      if (holdPlans.has(request.name)) {
        return new Promise((resolve) => {
          releasePlan[request.name] = () => resolve(issued);
        });
      }
      return Promise.resolve(issued);
    }
    if (cmd === "submit_operation") {
      const { planId } = args as { planId: string };
      const failure = submitFailures[planId];
      if (failure !== undefined) return Promise.reject(failure);
      if (holdSubmits.has(planId)) {
        return new Promise((resolve) => {
          releaseSubmit[planId] = () => resolve(7);
        });
      }
      return Promise.resolve(7);
    }
    return Promise.resolve(undefined);
  });
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("UpdatesPage", () => {
  it("shows the first check's spinner and why it takes a while until the backend has answered, not Loading…", async () => {
    // At launch `get_snapshot` has not answered yet: the first check is
    // under way, as the Overview says in the same words.
    const answer = mockInvoke.getMockImplementation();
    mockInvoke.mockImplementation((cmd: string, args?: InvokeArgs) =>
      cmd === "get_snapshot" ? new Promise(() => {}) : answer!(cmd, args),
    );
    const { findByRole, getByText, queryByText, container } = renderPage();

    expect(await findByRole("heading", { level: 2, name: "Checking…" })).toBeInTheDocument();
    expect(
      getByText("The first check looks up every tool's newest version online, and sometimes takes a minute or two."),
    ).toBeInTheDocument();
    expect(container.querySelector("[data-first-check] svg")).toHaveAttribute("width", "32");
    expect(queryByText("Loading…")).not.toBeInTheDocument();
  });

  it("shows each update's version change in its version column, in tabular numerals, with technical details off", async () => {
    // The redesign's rule for a row: what it is, which version, what can
    // be done (docs/superpowers/2026-09-27-ui-redesign.md, 原则 1). The
    // version jump is on every row now; "Show technical details" keeps
    // paths, commands and the tools' own error text.
    expect(settings.show_technical_details).toBe(false);
    const { findByText } = renderPage();

    const glib = await findByText("2.88.3 → 2.90.0");
    expect(glib.className).toContain("tabular-nums");
    expect(await findByText("5.0.2 → 5.1.0")).toBeInTheDocument();
  });

  it("lists the rows it can update by name, whatever order the sources gave them in", async () => {
    updates = [brewCandidate("wget"), brewCandidate("aria2"), ...snapshot.updates, brewCandidate("Zstd")];
    renderPage();

    await findRow("aria2");
    expect(rowNames()).toEqual(["aria2", "glib", "onyx", "wget", "Zstd"]);
  });

  it("names each row the way the Installed page does, with its source in a chip beside it", async () => {
    // A cask's row reads "OnyX", its display name, not the token `onyx`;
    // a package with no entry in `artifacts` keeps its own name. Every row
    // shows its source, since the list is no longer grouped by source --
    // except a tool with its own installer, which is its own source.
    const claudeKey: ArtifactKey = { instance_id: "standalone-claude", kind: "Binary", name: "claude" };
    instances = [
      ...snapshot.instances,
      {
        id: "standalone-claude",
        adapter_id: "standalone-claude",
        exe_path: "/Users/someone/.local/bin/claude",
        prefix: "/Users/someone/.local/share/claude",
        scope: "User",
        version: "2.1.281",
        status: { unavailable: null, notes: [] },
        unverified_version: null,
        read_only_reason: null,
      },
    ];
    artifacts = [
      {
        key: onyxKey,
        display_name: "OnyX",
        version: "5.0.2",
        reason: "Requested",
        description: "Verify system files structure",
        homepage: null,
        size_bytes: null,
        installed_at: null,
        path: null,
        auto_updates: false,
        uninstall_blocked: null,
      },
      {
        key: claudeKey,
        display_name: "Claude Code",
        version: "2.1.281",
        reason: "Requested",
        description: null,
        homepage: null,
        size_bytes: null,
        installed_at: null,
        path: null,
        auto_updates: false,
        uninstall_blocked: null,
      },
    ];
    updates = [
      ...snapshot.updates,
      {
        key: claudeKey,
        current: "2.1.281",
        target: "2.1.290",
        channel: "Registry",
        checkable: true,
        warnings: [],
        blocked: null,
      },
    ];
    renderPage();

    const onyx = await findRow("OnyX");
    expect(within(onyx).getByText("Homebrew")).toBeInTheDocument();
    expect(within(onyx).getByText("Verify system files structure")).toBeInTheDocument();
    expect(within(onyx).getByRole("checkbox")).toHaveAccessibleName("Select OnyX for update");
    expect(within(rowOf("glib")).getByText("Homebrew")).toBeInTheDocument();
    // A package the snapshot has no description for says what its source
    // says it is, never "No description".
    expect(within(rowOf("glib")).getByText("Homebrew package")).toBeInTheDocument();
    const claude = rowOf("Claude Code");
    expect(within(claude).getAllByText("Claude Code")).toHaveLength(1);
    expect(rowNames()).toEqual(["Claude Code", "glib", "OnyX"]);
  });

  it("previews the command, submits nothing until Confirm, then submits the single update", async () => {
    const { findAllByRole, findByRole, queryByRole } = renderPage();

    const updateButtons = await findAllByRole("button", { name: "Update" });
    fireEvent.click(updateButtons[0]);

    const dialog = await findByRole("dialog");
    showCommands(dialog);
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula glib");
    expect(mockInvoke).toHaveBeenCalledWith("plan_operation", {
      request: {
        kind: "Upgrade",
        instance_id: "brew:/opt/homebrew",
        artifact_kind: "Formula",
        name: "glib",
      },
    });
    // Seeing the command comes first; opening the dialog submits nothing.
    expect(submittedPlanIds()).toEqual([]);

    fireEvent.click(within(dialog).getByRole("button", { name: "Update" }));

    await waitFor(() => expect(submittedPlanIds()).toEqual([{ planId: "1" }]));
    // Every item started, so the dialog closes on its own.
    await waitFor(() => expect(queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("previews every selected command, then submits one operation per item, each with its own plan id", async () => {
    const { findAllByRole, getByRole, findByRole, queryByRole } = renderPage();

    const checkboxes = await findAllByRole("checkbox", { name: ROW_CHECKBOX });
    fireEvent.click(checkboxes[0]);
    fireEvent.click(checkboxes[1]);

    // The button counts what it will update.
    fireEvent.click(getByRole("button", { name: "Update Selected (2)" }));
    const dialog = await findByRole("dialog");
    showCommands(dialog);
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula glib");
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --cask onyx");
    expect(submittedPlanIds()).toEqual([]);

    fireEvent.click(within(dialog).getByRole("button", { name: "Update" }));

    await waitFor(() => expect(submittedPlanIds()).toEqual([{ planId: "1" }, { planId: "2" }]));
    await waitFor(() => expect(queryByRole("dialog")).not.toBeInTheDocument());
    expect(useUiStore.getState().selectedUpdates).toEqual([]);
  });

  it("warns per item, before the sudo prompt, about the one update that needs a password", async () => {
    // A batch can mix Casks and formulae, and the brew adapter only sets
    // `needs_password` for Casks (crates/canager-core/src/adapters/brew/
    // mod.rs). Spec §6: whatever will ask for a password says so in the
    // preview, next to the command it belongs to.
    needsPassword.add("onyx");
    const { findAllByRole, getByRole, findByRole } = renderPage();

    const checkboxes = await findAllByRole("checkbox", { name: ROW_CHECKBOX });
    fireEvent.click(checkboxes[0]);
    fireEvent.click(checkboxes[1]);

    fireEvent.click(getByRole("button", { name: /^Update Selected/ }));
    const dialog = await findByRole("dialog");
    showCommands(dialog);
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula glib");
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --cask onyx");

    const notices = within(dialog).getAllByText("Some apps ask for your Mac password at this step.");
    expect(notices).toHaveLength(1);
    // Under onyx's own row of the list, and no other's.
    expect(notices[0].closest("[data-sheet-tool]")?.querySelector("[data-sheet-name]")).toHaveTextContent("onyx");
  });

  it("shows a warning carried on the plan, such as cargo's compile-locally notice", async () => {
    planWarnings.glib = ["CompilesLocally"];
    const { findAllByRole, findByRole } = renderPage();

    fireEvent.click((await findAllByRole("button", { name: "Update" }))[0]);
    const dialog = await findByRole("dialog");
    await within(dialog).findByText(
      "This compiles on your Mac and takes a while.",
    );
  });

  it("says an update ends in Homebrew's clean-up when brew.env turns it back on, with the why behind its ⓘ", async () => {
    // `Warning::HomebrewPeriodicCleanup`, once a brew.env takes back
    // Canager's HOMEBREW_NO_INSTALL_CLEANUP=1
    // (crates/canager-core/src/adapters/brew/brew_env.rs): after every
    // `brew upgrade`, Homebrew deletes the older versions and old downloads
    // of the package it upgrades (`Cleanup.install_clean!`), and, when its
    // periodic clean-up is due, those of all its software (`Cleanup#clean!`).
    planWarnings.glib = ["HomebrewPeriodicCleanup"];
    const { findAllByRole, findByRole } = renderPage();

    fireEvent.click((await findAllByRole("button", { name: "Update" }))[0]);
    const dialog = await findByRole("dialog");
    const line =
      "After installing or updating, Homebrew deletes the older versions of this software and of any it updates along with it, and stray old downloads; when its periodic clean-up is due, those of all Homebrew software.";
    await within(dialog).findByText(line);
    const why =
      "Homebrew is run with HOMEBREW_NO_INSTALL_CLEANUP=1, but your brew.env sets it to nothing, and brew.env wins.";
    expect(within(dialog).queryByText(why)).toBeNull();
    fireEvent.click(within(dialog).getByRole("button", { name: `Details: ${line}` }));
    expect(screen.getByText(why)).toBeInTheDocument();
    expect(
      within(dialog).queryByText(/installed only as dependencies/),
    ).not.toBeInTheDocument();
  });

  it("says in Chinese that Homebrew cleans up after every update, and more when its periodic clean-up is due", async () => {
    planWarnings.glib = ["HomebrewPeriodicCleanup"];
    await i18n.changeLanguage("zh-CN");
    try {
      const { findAllByRole, findByRole } = renderPage();

      fireEvent.click((await findAllByRole("button", { name: "更新" }))[0]);
      const dialog = await findByRole("dialog");
      await within(dialog).findByText(
        "安装或更新后，Homebrew会删除此软件及一起更新的软件的旧版本，以及残留的旧下载文件；定期清理到期时，所有Homebrew软件的旧版本和旧下载文件也会被删除。",
      );
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("says an update ends in Homebrew's clean-up and its autoremove when brew.env turns both back on", async () => {
    // `Warning::HomebrewCleanupAutoremoves` follows it when a brew.env
    // takes back HOMEBREW_NO_AUTOREMOVE=1 as well: the periodic clean-up
    // then also autoremoves.
    planWarnings.glib = ["HomebrewPeriodicCleanup", "HomebrewCleanupAutoremoves"];
    const { findAllByRole, findByRole } = renderPage();

    fireEvent.click((await findAllByRole("button", { name: "Update" }))[0]);
    const dialog = await findByRole("dialog");
    await within(dialog).findByText(
      "After installing or updating, Homebrew deletes the older versions of this software and of any it updates along with it, and stray old downloads; when its periodic clean-up is due, those of all Homebrew software.",
    );
    await within(dialog).findByText(
      "Homebrew's periodic clean-up also removes other Homebrew packages that were installed only as dependencies and that nothing needs any more.",
    );
  });

  it("offers no Update button and no checkbox for a candidate the adapter could not check, and says why behind its chip", async () => {
    // The whole point of UpdateCandidate.checkable. A git-sourced cargo
    // crate reports checkable:false because crates.io knows nothing about
    // it -- and "Update" on such a row would run `cargo install --force
    // my-fork` against the crates.io crate of the same name, a different
    // package entirely. The same flag covers an Ollama model whose manifest
    // could not be read and a pipx tool whose PyPI lookup failed.
    updates = [
      {
        key: myForkKey,
        current: "0.1.0",
        target: "0.1.0",
        channel: "Registry",
        checkable: false,
        warnings: ["NonRegistrySource"],
        blocked: null,
      },
    ];
    const { queryByRole, getByRole } = renderPage();

    await showCantUpdate();
    const myFork = await findRow("my-fork");
    expect(queryByRole("button", { name: "Update" })).not.toBeInTheDocument();
    expect(queryByRole("checkbox", { name: ROW_CHECKBOX })).not.toBeInTheDocument();
    // Nothing to tick either in the list's header.
    expect(getByRole("checkbox", { name: "Select all items that can be updated here" })).toBeDisabled();
    // A row that can't be updated here has its name, its line, its word
    // and its ⋯: no version column -- it has no version to move to.
    expect(within(myFork).queryByText("0.1.0")).not.toBeInTheDocument();
    const detail = chipDetail(myFork, "Can't check");
    expect(within(detail).getByText("Couldn't find its latest version.")).toBeInTheDocument();
    expect(within(detail).getByText("It wasn't installed from crates.io.")).toBeInTheDocument();
  });

  it("keeps an uncheckable candidate out of Update selected even when it was selected earlier", async () => {
    // Hiding the checkbox is not enough on its own. A selection lives in
    // the UI store and outlives the row that made it, so a candidate
    // selected while it was checkable stays selected after a refresh flips
    // `checkable` to false (cargo's crates.io lookup failing is enough to
    // do that). Without this, "Update Selected" would still plan the row
    // whose Update button was just taken away -- `cargo install --force
    // my-fork` against the same-named crates.io crate, the exact hazard
    // `checkable` exists for.
    updates = [
      snapshot.updates[0],
      {
        key: myForkKey,
        current: "0.1.0",
        target: "0.1.0",
        channel: "Registry",
        checkable: false,
        warnings: ["NonRegistrySource"],
        blocked: null,
      },
    ];
    act(() => {
      useUiStore.getState().toggleUpdate(glibKey);
      useUiStore.getState().toggleUpdate(myForkKey);
    });

    const { getByRole, findByRole } = renderPage();

    await findRow("glib");
    // One, not two: my-fork's selection counts for nothing.
    fireEvent.click(getByRole("button", { name: "Update Selected (1)" }));

    const dialog = await findByRole("dialog");
    showCommands(dialog);
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula glib");
    expect(plannedNames()).toEqual(["glib"]);
  });

  it("offers no Update button and no checkbox for a pinned formula, and says how to release it", async () => {
    // `brew outdated` lists a pinned formula like any other, and `brew
    // upgrade glib` then exits 1 with "Not upgrading 1 pinned package".
    // The row stays -- the newer version is real -- but it offers nothing
    // Homebrew will refuse, and says why and what the user can do.
    updates = [{ ...snapshot.updates[0], blocked: "Pinned" }, snapshot.updates[1]];
    const { findByText, getAllByRole, getByText } = renderPage();

    await findRow("onyx");
    // Only onyx's.
    expect(getAllByRole("button", { name: "Update" })).toHaveLength(1);
    expect(getAllByRole("checkbox", { name: ROW_CHECKBOX })).toHaveLength(1);
    expect(getAllByRole("checkbox", { name: ROW_CHECKBOX })[0]).toHaveAccessibleName("Select onyx for update");
    // Counted apart from what can be updated: "1 Update Available", and one under
    // "Can't update here".
    await findByText("1 Update Available");
    await showCantUpdate();
    expect(getByText("1 more can't be updated here")).toBeInTheDocument();
    const glib = await findRow("glib");
    // Its name, its line, its word and its ⋯: no version and no button.
    expect(within(glib).queryByText("2.88.3 → 2.90.0")).not.toBeInTheDocument();
    expect(within(glib).queryByRole("button", { name: "Update" })).not.toBeInTheDocument();
    // Their columns are there all the same, empty, so its word stands in
    // the status column the rows above it have: one column down the list.
    const onyx = await findRow("onyx");
    const columns = (row: HTMLElement) =>
      [...row.children].map((child) =>
        child.hasAttribute("data-status-column") ? "status" : child.hasAttribute("data-version") ? "version" : null,
      );
    expect(columns(glib)).toEqual(columns(onyx));
    expect((glib.querySelector("[data-version]") as HTMLElement).textContent).toBe("");
    expect(glib.querySelector("[data-status]")).toHaveTextContent("Pinned");
    expect(onyx.children.length).toBe(glib.children.length);
    const detail = chipDetail(glib, "Pinned");
    expect(
      within(detail).getByText(
        wholeSentence(
          "It's pinned in Homebrew. To update it, first run /opt/homebrew/bin/brew unpin glib in Terminal.",
        ),
      ),
    ).toBeInTheDocument();
    // Set apart as code, so it is visibly a command and nothing around it
    // gets copied with it.
    expect(within(detail).getByText("/opt/homebrew/bin/brew unpin glib").tagName).toBe("CODE");
  });

  it("names the cask form of the unpin command for a pinned cask", async () => {
    // `brew unpin <name>` resolves a formula first; `--cask` makes it
    // release the cask even when a formula shares the name.
    updates = [snapshot.updates[0], { ...snapshot.updates[1], blocked: "Pinned" }];
    renderPage();

    await showCantUpdate();
    const detail = chipDetail(await findRow("onyx"), "Pinned");
    expect(within(detail).getByText("/opt/homebrew/bin/brew unpin --cask onyx").tagName).toBe("CODE");
  });

  it("does not promise a pinned app that updates itself will stay at its version", async () => {
    // `brew pin` warns that a cask with `auto_updates true` "may update
    // itself outside Homebrew despite being pinned". Such a row reaches
    // the page mostly with include_self_updating (`brew outdated --greedy`).
    // Its detail says only what the pin stops and how to lift it, which is
    // true of it too.
    settings = { ...settings, include_self_updating: true };
    updates = [snapshot.updates[0], { ...snapshot.updates[1], blocked: "Pinned" }];
    artifacts = [
      {
        key: onyxKey,
        display_name: "OnyX",
        version: "5.0.2",
        reason: "Requested",
        description: "Verify system files structure",
        homepage: null,
        size_bytes: null,
        installed_at: null,
        path: null,
        auto_updates: true,
        uninstall_blocked: null,
      },
    ];
    const { queryByText } = renderPage();

    await showCantUpdate();
    const detail = chipDetail(await findRow("OnyX"), "Pinned");
    expect(
      within(detail).getByText(
        wholeSentence(
          "It's pinned in Homebrew. To update it, first run /opt/homebrew/bin/brew unpin --cask onyx in Terminal.",
        ),
      ),
    ).toBeInTheDocument();
    expect(queryByText(/version it has now|keeping this/)).toBeNull();
  });

  it("promises a silent source's pinned row nothing about when its update will come", async () => {
    // `updateStateOf` checks `blocked` before `sourceUnavailable`, so a
    // pinned candidate under a Homebrew that did not answer is still
    // chipped Pinned, not Unavailable -- but the row gets no Update button
    // either way until Homebrew answers a check again (`isUpdateActionable`
    // needs `isAvailable`), so its detail may not promise the update at
    // any "next time".
    instances = [
      { ...snapshot.instances[0], status: { unavailable: "NotResponding", notes: [] } },
      ...snapshot.instances.slice(1),
    ];
    updates = [{ ...snapshot.updates[0], blocked: "Pinned" }, snapshot.updates[1]];
    const { queryAllByRole, queryByText } = renderPage();

    await showCantUpdate();
    const glib = await findRow("glib");
    expect(queryAllByRole("button", { name: "Update" })).toHaveLength(0);
    const detail = chipDetail(glib, "Pinned");
    expect(
      within(detail).getByText(
        wholeSentence(
          "It's pinned in Homebrew. To update it, first run /opt/homebrew/bin/brew unpin glib in Terminal.",
        ),
      ),
    ).toBeInTheDocument();
    expect(queryByText(/next time/)).toBeNull();
    // onyx, not pinned, is Unavailable, and says so of Homebrew.
    expect(
      within(chipDetail(rowOf("onyx"), "Can't update now")).getByText(
        "Homebrew isn't responding. Click Check Again later.",
      ),
    ).toBeInTheDocument();
  });

  it("names the brew that owns a pinned package when a Mac has two, not whichever PATH finds", async () => {
    // A Mac migrated from Intel keeps /usr/local beside /opt/homebrew,
    // and Terminal's `brew` is /opt/homebrew/bin/brew. glib pinned in
    // /usr/local has to be released by /usr/local/bin/brew: the other one
    // answers "glib not pinned" and the row would stay Pinned for good.
    const intelBrew: Snapshot["instances"][number] = {
      ...snapshot.instances[0],
      id: "brew:/usr/local",
      exe_path: "/usr/local/bin/brew",
      prefix: "/usr/local",
    };
    instances = [snapshot.instances[0], intelBrew];
    updates = [
      snapshot.updates[1],
      {
        ...snapshot.updates[0],
        key: { ...glibKey, instance_id: "brew:/usr/local" },
        blocked: "Pinned",
      },
    ];
    const { queryByText } = renderPage();

    await showCantUpdate();
    const detail = chipDetail(await findRow("glib"), "Pinned");
    expect(within(detail).getByText("/usr/local/bin/brew unpin glib").tagName).toBe("CODE");
    expect(queryByText(/\/opt\/homebrew\/bin\/brew unpin/)).toBeNull();
  });

  it("names each of two Homebrews by the Mac it is for, on the rows and in the confirmation", async () => {
    // A Mac migrated from Intel: a Homebrew in /usr/local beside the one
    // in /opt/homebrew. Each is named as the sidebar names it, so that
    // 「Homebrew」 never stands for either.
    const intelBrew: Snapshot["instances"][number] = {
      ...snapshot.instances[0],
      id: "brew:/usr/local",
      exe_path: "/usr/local/bin/brew",
      prefix: "/usr/local",
    };
    instances = [snapshot.instances[0], intelBrew];
    const intelGlib: ArtifactKey = { ...glibKey, instance_id: "brew:/usr/local" };
    updates = [
      { ...snapshot.updates[0], key: intelGlib },
      brewCandidate("wget"),
      { ...brewCandidate("wget"), key: { instance_id: "brew:/usr/local", kind: "Formula", name: "wget" } },
    ];
    const { findByRole, getByRole } = renderPage();

    // The row's source, for a screen reader and in the avatar's tooltip.
    const glib = await findRow("glib");
    expect(within(glib).getByText("Homebrew (Intel)")).toHaveClass("sr-only");
    expect(glib.querySelector('[title="Homebrew (Intel)"]')).not.toBeNull();
    expect(within(glib).queryByText("Homebrew")).toBeNull();
    // A name both have says in sight which is which.
    const wgets = screen
      .getAllByText("wget", { selector: "[data-tool-row] p" })
      .map((element) => element.closest("[data-tool-row]") as HTMLElement);
    expect(wgets.map((row) => within(row).getByText(/^Homebrew \(/).textContent).sort()).toEqual([
      "Homebrew (Apple silicon)",
      "Homebrew (Intel)",
    ]);

    // The update's confirmation, under its question.
    fireEvent.click(within(glib).getByRole("button", { name: "Update" }));
    const dialog = await findByRole("dialog", { name: "Update “glib”?" });
    const jump = await within(dialog).findByText("2.88.3 → 2.90.0");
    expect(jump.closest("[data-dialog-subtitle]")).toHaveTextContent("Homebrew (Intel) · 2.88.3 → 2.90.0");
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

    // Several: the two wgets' rows say in sight whose each is, and glib's
    // for a screen reader.
    fireEvent.click(getByRole("button", { name: "Update All" }));
    const several = await findByRole("dialog", { name: "Update 3 tools?" });
    const sources = within(several).getAllByText(/^Homebrew/);
    expect(sources.filter((source) => !source.classList.contains("sr-only")).map((source) => source.textContent)).toEqual(
      ["Homebrew (Apple silicon)", "Homebrew (Intel)"],
    );
    expect(sources.filter((source) => source.classList.contains("sr-only")).map((source) => source.textContent)).toEqual(
      ["Homebrew (Intel)"],
    );
  });

  it("names pipx and pipx's own unpin command on a pinned pipx tool", async () => {
    // `pipx list --outdated` lists a pinned tool as `cowsay [pinned]: 5.0
    // -> 6.1`, and `pipx upgrade cowsay` then changes nothing and exits 0.
    // The row must say pipx, not Homebrew, and give the command pipx
    // itself names ("Run `pipx unpin cowsay` to unpin it"), from the pipx
    // Canager found.
    const pipx: Snapshot["instances"][number] = {
      id: "pipx",
      adapter_id: "pipx",
      exe_path: "/opt/homebrew/bin/pipx",
      prefix: "/opt/homebrew/bin",
      scope: "User",
      version: "1.17.3",
      status: { unavailable: null, notes: [] },
      unverified_version: null,
      read_only_reason: null,
    };
    instances = [snapshot.instances[0], pipx];
    updates = [
      snapshot.updates[0],
      {
        key: { instance_id: "pipx", kind: "Tool", name: "cowsay" },
        current: "5.0",
        target: "6.1",
        channel: "Native",
        checkable: true,
        warnings: [],
        blocked: "Pinned",
      },
    ];
    const { getAllByRole, queryByText } = renderPage();

    await findRow("glib");
    // Only glib's.
    expect(getAllByRole("button", { name: "Update" })).toHaveLength(1);
    expect(getAllByRole("checkbox", { name: ROW_CHECKBOX })).toHaveLength(1);
    expect(getAllByRole("checkbox", { name: ROW_CHECKBOX })[0]).toHaveAccessibleName("Select glib for update");
    await showCantUpdate();
    const detail = chipDetail(await findRow("cowsay"), "Pinned");
    expect(
      within(detail).getByText(
        wholeSentence(
          "It's pinned in pipx. To update it, first run /opt/homebrew/bin/pipx unpin cowsay in Terminal.",
        ),
      ),
    ).toBeInTheDocument();
    expect(within(detail).getByText("/opt/homebrew/bin/pipx unpin cowsay").tagName).toBe("CODE");
    expect(queryByText(/brew unpin/)).toBeNull();
    expect(queryByText(/pinned in Homebrew/)).toBeNull();
  });

  it("keeps a pinned candidate out of Update selected even when it was selected earlier", async () => {
    // Selected while it was not pinned; a refresh since says it is. The
    // selection outlives the row's checkbox, so `isActionable` has to be
    // what filters the batch, or `brew upgrade --formula glib` would be
    // planned anyway (and refused by `Session::issue_plan`).
    updates = [{ ...snapshot.updates[0], blocked: "Pinned" }, snapshot.updates[1]];
    act(() => {
      useUiStore.getState().toggleUpdate(glibKey);
      useUiStore.getState().toggleUpdate(onyxKey);
    });

    const { getByRole, findByRole } = renderPage();

    await findRow("onyx");
    fireEvent.click(getByRole("button", { name: "Update Selected (1)" }));

    await findByRole("dialog");
    expect(plannedNames()).toEqual(["onyx"]);
  });

  it("offers no Update button and no checkbox for a pip package, and points at pipx or uv instead", async () => {
    // pip is read-only by design: its plan() refuses every operation with
    // "unsupported: pip is read-only in Canager; use pipx or uv to manage
    // {name}". Its candidates are still built with checkable: true, because
    // pip genuinely can check -- so gating the Update button on `checkable`
    // alone offered a button whose only possible outcome is a raw Rust error
    // string in a dialog.
    updates = [
      snapshot.updates[0],
      {
        key: urllib3Key,
        current: "2.2.1",
        target: "2.3.0",
        channel: "Registry",
        checkable: true,
        warnings: [],
        blocked: null,
      },
    ];
    const { findByText, findAllByRole } = renderPage();

    // glib's button and checkbox, and only glib's.
    expect(await findAllByRole("button", { name: "Update" })).toHaveLength(1);
    expect(await findAllByRole("checkbox", { name: ROW_CHECKBOX })).toHaveLength(1);
    // One update the user can act on, and one they cannot -- both said out
    // loud. Counting only the first left "0 updates available" above six
    // listed rows on a machine whose only outdated packages were pip's.
    await findByText("1 Update Available");
    await findByText("1 more can't be updated here");
    await showCantUpdate();
    const detail = chipDetail(await findRow("urllib3"), "View only");
    expect(detail).toHaveTextContent(
      "You can only view pip installs here. Install Python tools with pipx or uv to update and uninstall them here.",
    );
  });

  it("marks each pip row View only in a word, keeps the why behind the chip, and leaves every row its own description", async () => {
    // Six outdated pip packages used to mean six copies of a
    // ~200-character paragraph, and the packages' own blurbs were displaced
    // by it, so the six rows read identically. Each row now says it in one
    // word, the explanation is one click away, and the blurbs stay.
    const pipPackages = ["urllib3", "requests", "certifi", "idna", "charset-normalizer", "six"];
    updates = pipPackages.map((name) => ({
      key: { instance_id: "pip:/usr/bin/python3", kind: "Package" as const, name },
      current: "1.0.0",
      target: "1.1.0",
      channel: "Registry" as const,
      checkable: true,
      warnings: [],
      blocked: null,
    }));
    artifacts = pipPackages.map((name) => ({
      key: { instance_id: "pip:/usr/bin/python3", kind: "Package" as const, name },
      display_name: name,
      version: "1.0.0",
      reason: "Requested" as const,
      description: `what ${name} is for`,
      homepage: null,
      size_bytes: null,
      installed_at: null,
      path: null,
      auto_updates: false,
      uninstall_blocked: null,
    }));
    const { findByText, queryAllByText, getAllByRole } = renderPage();

    await showCantUpdate();
    await findRow("urllib3");
    expect(getAllByRole("button", { name: "View only" })).toHaveLength(6);
    expect(queryAllByText(/with pipx or uv/)).toHaveLength(0);
    // And each row can be told from the next again.
    for (const name of pipPackages) {
      expect(await findByText(`what ${name} is for`)).toBeInTheDocument();
    }
    chipDetail(rowOf("idna"), "View only");
    expect(queryAllByText(/with pipx or uv/)).toHaveLength(1);
  });

  it("says nothing about a read-only source that has no rows on this page", async () => {
    // pip being read-only is not news on a page listing two Homebrew
    // updates.
    const { queryByText, queryByRole } = renderPage();

    await findRow("glib");
    expect(queryByText("View only")).not.toBeInTheDocument();
    expect(queryByRole("button", { name: "View only" })).not.toBeInTheDocument();
  });

  it("does not count a row it could not check as an available update", async () => {
    // The headline counts what Canager can act on, and `checkable` is one
    // of the things that decides that. A writable, answering source whose
    // registry lookup failed produced six rows with no buttons under the
    // words "6 updates available". The two numbers now come from exactly
    // the predicate that draws the buttons.
    updates = [
      snapshot.updates[0],
      {
        key: myForkKey,
        current: "0.1.0",
        target: "0.1.0",
        channel: "Registry",
        checkable: false,
        warnings: ["NonRegistrySource"],
        blocked: null,
      },
    ];
    const { findByText, queryByText } = renderPage();

    await findByText("1 Update Available");
    await findByText("1 more can't be updated here");
    expect(queryByText("2 Updates Available")).not.toBeInTheDocument();
  });

  it("says why a row can't be checked even when its source is also read-only", async () => {
    // pip is read-only *and* reaches PyPI, so a failed lookup produces
    // rows where both facts are true at once -- and the read-only advice
    // used to win outright, leaving no trace that Canager had not managed
    // to check anything. A row has one word (spec §3.4): "Can't check",
    // this check's news, the words the page's line over these rows counts;
    // its why says both -- that this check found nothing, then that no
    // button will ever appear here.
    updates = [
      {
        key: urllib3Key,
        current: "2.2.1",
        target: "2.2.1",
        channel: "Native",
        checkable: false,
        warnings: [{ Message: "pip list --outdated: ERROR: Could not fetch URL https://pypi.org/simple/" }],
        blocked: null,
      },
    ];
    renderPage();

    await showCantUpdate();
    const urllib3 = await findRow("urllib3");
    expect(within(urllib3).queryByRole("button", { name: "View only" })).not.toBeInTheDocument();
    const reason = chipDetail(urllib3, "Can't check");
    expect([...reason.querySelectorAll("p")].map((line) => line.textContent)).toEqual([
      "Couldn't find its latest version.",
      "You can only view pip installs here. Install Python tools with pipx or uv to update and uninstall them here.",
    ]);
  });

  it("keeps the tool's own error text behind Show technical details", async () => {
    // Going offline used to paper every row with the same line of English
    // stderr: every installed package gets a row, and every row got "pip
    // list --outdated: ERROR: Could not fetch URL https://pypi.org/simple/"
    // as its description. `show_technical_details` promises to hide "the
    // commands Canager actually runs", which is exactly what that line is
    // (spec §6).
    const names = ["urllib3", "requests", "certifi"];
    updates = names.map((name) => ({
      key: { instance_id: "pip:/usr/bin/python3", kind: "Package" as const, name },
      current: "1.0.0",
      target: "1.0.0",
      channel: "Registry" as const,
      checkable: false,
      warnings: [
        { Message: "pip list --outdated: ERROR: Could not fetch URL https://pypi.org/simple/" },
      ],
      blocked: null,
    }));
    const { queryAllByText } = renderPage();

    await showCantUpdate();
    const detail = chipDetail(await findRow("certifi"), "Can't check");
    // What a person who does not write code is told instead: that it could
    // not be checked, in one short sentence -- then pip's way out.
    expect([...detail.querySelectorAll("p")].map((line) => line.textContent)).toEqual([
      "Couldn't find its latest version.",
      "You can only view pip installs here. Install Python tools with pipx or uv to update and uninstall them here.",
    ]);
    expect(queryAllByText(/Could not fetch URL/)).toHaveLength(0);
    expect(queryAllByText(/pip list --outdated/)).toHaveLength(0);
    // ...and, once for the page, a button that shows the rest.
    const summary = queryAllByText(/3 tools couldn't be checked for updates/);
    expect(summary).toHaveLength(1);
    expect(within(summary[0].parentElement as HTMLElement).getByRole("button", { name: "Show Reasons" })).toBeEnabled();
  });

  it("says where to see why rows could not be checked once for the page, not once per row", async () => {
    // Offline, one failed `npm outdated -g` turns every global package into
    // an uncheckable row. The explanation used to be a 180-character
    // paragraph on each of them -- seventy globals, seventy copies. It
    // names no cause: the tool's own words, which are what would, are
    // exactly what is hidden.
    updates = Array.from({ length: 70 }, (_, index) => ({
      key: { instance_id: "npm:/usr/local", kind: "Package" as const, name: `global-${index}` },
      current: "1.0.0",
      target: "1.0.0",
      channel: "Native" as const,
      checkable: false,
      warnings: [{ Message: "npm outdated -g: npm error code ENOTFOUND" }],
      blocked: null,
    }));
    const { findByText, queryAllByText } = renderPage();

    await findByText("70 more can't be updated here");
    await showCantUpdate();
    const summary = queryAllByText(/70 tools couldn't be checked for updates/);
    expect(summary).toHaveLength(1);
    // The sentence says what happened, and a button next to it shows why,
    // rather than a sentence saying which setting to find where.
    expect(summary[0].textContent).toBe("70 tools couldn't be checked for updates.");
    expect(queryAllByText(/ENOTFOUND/)).toHaveLength(0);
  });

  it("shows why with one press: Show Reasons turns on Show technical details, and the tools' words take the line's place", async () => {
    updates = ["urllib3", "requests"].map((name) => ({
      key: { instance_id: "npm:/usr/local", kind: "Package" as const, name },
      current: "1.0.0",
      target: "1.0.0",
      channel: "Native" as const,
      checkable: false,
      warnings: [{ Message: "npm outdated -g: npm error code ENOTFOUND" }],
      blocked: null,
    }));
    const before = { ...settings };
    const { findByText, getByRole, queryByText } = renderPage();

    await showCantUpdate();
    await findByText("2 tools couldn't be checked for updates.");
    const showReasons = getByRole("button", { name: "Show Reasons" });
    // Where it goes, in its tooltip: the setting it turns on.
    expect(showReasons).toHaveAttribute("title", "Turns on “Show technical details” in Settings");
    fireEvent.click(showReasons);

    await waitFor(() =>
      expect(calls("set_settings").map(([, args]) => (args as { settings: Settings }).settings.show_technical_details)).toEqual([true]),
    );
    // The rest of the settings as they were.
    expect((calls("set_settings")[0][1] as { settings: Settings }).settings).toEqual({
      ...before,
      show_technical_details: true,
    });
    await waitFor(() => expect(queryByText(/couldn't be checked for updates/)).not.toBeInTheDocument());
    const detail = chipDetail(await findRow("requests"), "Can't check");
    expect(detail).toHaveTextContent("npm outdated -g: npm error code ENOTFOUND");
  });

  it("does not count a row with its own reason in the page's cannot-check line", async () => {
    // A git-installed crate says its own, different, reason behind its
    // chip; "turn on technical details to see why" is not about it. With
    // only such rows there is nothing for the page to add.
    updates = [
      {
        key: myForkKey,
        current: "0.1.0",
        target: "0.1.0",
        channel: "Registry",
        checkable: false,
        warnings: ["NonRegistrySource"],
        blocked: null,
      },
    ];
    const { queryByText } = renderPage();

    await showCantUpdate();
    await findRow("my-fork");
    expect(queryByText(/of these for updates/)).not.toBeInTheDocument();
  });

  it("shows the tool's own error text once Show technical details is on, under the plain sentence", async () => {
    settings.show_technical_details = true;
    updates = [
      {
        key: urllib3Key,
        current: "2.2.1",
        target: "2.2.1",
        channel: "Registry",
        checkable: false,
        warnings: [
          { Message: "pip list --outdated: ERROR: Could not fetch URL https://pypi.org/simple/" },
        ],
        blocked: null,
      },
    ];
    const { queryByText } = renderPage();

    await showCantUpdate();
    const detail = chipDetail(await findRow("urllib3"), "Can't check");
    const lines = [...detail.querySelectorAll("p")].map((line) => line.textContent);
    expect(lines).toEqual([
      "Couldn't find its latest version.",
      "pip list --outdated: ERROR: Could not fetch URL https://pypi.org/simple/",
      "You can only view pip installs here. Install Python tools with pipx or uv to update and uninstall them here.",
    ]);
    // The rows already carry the tools' words, so the page's line --
    // which exists to point at this switch -- has nothing to add.
    expect(queryByText(/of these for updates/)).not.toBeInTheDocument();
  });

  it("gives a reason that was written for this audience with technical details off", async () => {
    // Two kinds of text end up behind an uncheckable row's chip.
    // `NonRegistrySource` is written for this audience already; a `Message`
    // is whatever the tool printed, in whatever language it printed it.
    updates = [
      {
        key: myForkKey,
        current: "0.1.0",
        target: "0.1.0",
        channel: "Registry",
        checkable: false,
        warnings: ["NonRegistrySource"],
        blocked: null,
      },
    ];
    renderPage();

    await showCantUpdate();
    const detail = chipDetail(await findRow("my-fork"), "Can't check");
    expect([...detail.querySelectorAll("p")].map((line) => line.textContent)).toEqual([
      "Couldn't find its latest version.",
      "It wasn't installed from crates.io.",
    ]);
  });

  it("tells an npm user to install Node with Homebrew, not to use pipx or uv", async () => {
    // Both sources are read-only, for different reasons, and the wire says
    // which. Before `read_only_reason` the page hardcoded pip's advice for
    // every read-only row, so a user whose npm prefix is root-owned was
    // told to install their JavaScript tooling with a Python tool.
    updates = [
      {
        key: typescriptKey,
        current: "5.6.2",
        target: "5.7.0",
        channel: "Registry",
        checkable: true,
        warnings: [],
        blocked: null,
      },
      {
        key: urllib3Key,
        current: "2.2.1",
        target: "2.3.0",
        channel: "Registry",
        checkable: true,
        warnings: [],
        blocked: null,
      },
    ];
    const { queryAllByRole } = renderPage();

    await showCantUpdate();
    const npm = chipDetail(await findRow("typescript"), "View only");
    // Only the packages installed with a Node from Homebrew: the ones in
    // this folder do not move over (T5).
    expect(npm).toHaveTextContent(
      "npm keeps these in a folder your account can't change, so you can only view them. After you install Node with Homebrew, you can manage the npm packages you install with it here.",
    );
    expect(npm.textContent).not.toMatch(/pipx|uv/);
    // pip's row keeps pip's advice, right next to it.
    const pip = chipDetail(rowOf("urllib3"), "View only");
    expect(pip).toHaveTextContent(/with pipx or uv/);
    expect(queryAllByRole("button", { name: "Update" })).toHaveLength(0);
  });

  it("says nothing here can be updated, rather than 0 updates, when every row is read-only", async () => {
    updates = [
      {
        key: urllib3Key,
        current: "2.2.1",
        target: "2.3.0",
        channel: "Registry",
        checkable: true,
        warnings: [],
        blocked: null,
      },
      {
        key: typescriptKey,
        current: "5.6.2",
        target: "5.7.0",
        channel: "Registry",
        checkable: true,
        warnings: [],
        blocked: null,
      },
    ];
    const { findByText, queryByText } = renderPage();

    expect(await findByText("Nothing to update here")).toBeInTheDocument();
    expect(queryByText("0 Updates Available")).not.toBeInTheDocument();
    // The headline switching to "nothing here" does not excuse dropping the
    // number: two listed rows the user cannot act on are still counted.
    expect(await findByText("2 more can't be updated here")).toBeInTheDocument();
  });

  it("keeps a read-only source's candidate out of Update selected even when it was selected earlier", async () => {
    // Same hazard as the uncheckable case above: a selection lives in the UI
    // store and outlives the row that made it, so hiding the checkbox is not
    // enough on its own.
    updates = [
      snapshot.updates[0],
      {
        key: urllib3Key,
        current: "2.2.1",
        target: "2.3.0",
        channel: "Registry",
        checkable: true,
        warnings: [],
        blocked: null,
      },
    ];
    act(() => {
      useUiStore.getState().toggleUpdate(glibKey);
      useUiStore.getState().toggleUpdate(urllib3Key);
    });

    const { getByRole, findByRole } = renderPage();

    await findRow("glib");
    fireEvent.click(getByRole("button", { name: "Update Selected (1)" }));

    const dialog = await findByRole("dialog");
    showCommands(dialog);
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula glib");
    expect(plannedNames()).toEqual(["glib"]);
  });

  it("says an Ollama model has a new version instead of printing two digests", async () => {
    // `current` is the local manifest digest /api/tags reported; `target` is
    // the registry manifest's config digest. They are different hash spaces,
    // not two readings of one identifier -- the adapter's own comment
    // (crates/canager-core/src/adapters/ollama/mod.rs) forbids rendering
    // them as a version jump, and neither is anything to show a person who
    // does not write code. `channel: "Digest"` is the discriminator, with
    // technical details on as well.
    settings.show_technical_details = true;
    instances = [...snapshot.instances, { ...stoppedOllama, status: { unavailable: null, notes: [] } }];
    updates = [
      {
        key: qwenKey,
        current: "5642e97495e1a0888838ee1b3b1a0b1c6a0f0f5e6c2d4a8b9e7c3d1f0a2b4c6d",
        target: "sha256:9f1c0b6d2e4a7c5b3d1f8a6e4c2b0d9f7e5c3a1b8d6f4e2c0a9b7d5f3e1c8a6b",
        channel: "Digest",
        checkable: true,
        warnings: [],
        blocked: null,
      },
    ];
    const { queryByText, container } = renderPage();

    const qwen = await findRow("qwen3:8b");
    expect(within(qwen).getByText("New version")).toBeInTheDocument();
    expect(queryByText(/5642e97495e1a0888838/)).not.toBeInTheDocument();
    expect(container.textContent).not.toMatch(/sha256|→/);
  });

  it("names a model pulled from another registry by the model itself, with where it is from on its line", async () => {
    const coderKey: ArtifactKey = { ...qwenKey, name: MODELS.coder };
    instances = [...snapshot.instances, { ...stoppedOllama, status: { unavailable: null, notes: [] } }];
    updates = [
      {
        key: coderKey,
        current: "52e05d4a30959ae2542932b2c473f476dca0ce371aaf9a2227badf4e3eeec4f4",
        target: "sha256:9f1c0b6d2e4a7c5b3d1f8a6e4c2b0d9f7e5c3a1b8d6f4e2c0a9b7d5f3e1c8a6b",
        channel: "Digest",
        checkable: true,
        warnings: [{ ThirdPartyRegistry: { host: "modelscope.cn" } }],
        blocked: null,
      },
    ];
    const { findByTitle } = renderPage();

    const name = await findByTitle(MODELS.coder);
    expect(name.matches("[data-tool-row] p")).toBe(true);
    expect(name.firstElementChild?.textContent).toBe("Qwen2.5-Coder-7B-Instruct-GGUF:Q4_K_M");
    expect(name.querySelector(".sr-only")?.textContent).toBe(MODELS.coder);
    const row = name.closest("[data-tool-row]") as HTMLElement;
    expect(row.querySelector("[data-description]")?.textContent).toMatch(/^modelscope\.cn\/Qwen · /);
    // The row's checkbox and its details say it whole.
    expect(within(row).getByRole("checkbox", { name: `Select ${MODELS.coder} for update` })).toBeInTheDocument();
  });

  it("shows no version and no digest for an Ollama model Canager could not check", async () => {
    // Its `target` is the digest it has, not a newer one: "New version"
    // would be false, and the digest is never shown.
    instances = [...snapshot.instances, { ...stoppedOllama, status: { unavailable: null, notes: [] } }];
    updates = [
      {
        key: qwenKey,
        current: "5642e97495e1a0888838ee1b3b1a0b1c6a0f0f5e6c2d4a8b9e7c3d1f0a2b4c6d",
        target: "5642e97495e1a0888838ee1b3b1a0b1c6a0f0f5e6c2d4a8b9e7c3d1f0a2b4c6d",
        channel: "Digest",
        checkable: false,
        warnings: [{ Message: "registry request failed" }],
        blocked: null,
      },
    ];
    const { container } = renderPage();

    await showCantUpdate();
    const qwen = await findRow("qwen3:8b");
    expect(within(qwen).queryByText("New version")).toBeNull();
    expect(container.textContent).not.toMatch(/5642e974/);
  });

  it("submits nothing when the confirmation is cancelled", async () => {
    const { findAllByRole, findByRole, queryByRole } = renderPage();

    fireEvent.click((await findAllByRole("button", { name: "Update" }))[0]);
    const dialog = await findByRole("dialog");
    showCommands(dialog);
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula glib");

    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));

    await waitFor(() => expect(queryByRole("dialog")).not.toBeInTheDocument());
    expect(submittedPlanIds()).toEqual([]);
  });

  it("localises an expired plan and submits a fresh plan id only after a second Confirm", async () => {
    // The dialog sat open past the PlanId's 10-minute lifetime: the backend
    // rejects with `{"kind":"expired"}` (see `submit_operation_error` in
    // src-tauri/src/ipc.rs), which `planErrorMessage` renders as
    // `planRefused.expired` rather than showing the JSON or this project's
    // own hardcoded English.
    submitFailures["1"] = '{"kind":"expired"}';
    const { findAllByRole, findByRole, queryByRole } = renderPage();

    fireEvent.click((await findAllByRole("button", { name: "Update" }))[0]);
    let dialog = await findByRole("dialog");
    showCommands(dialog);
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula glib");
    fireEvent.click(within(dialog).getByRole("button", { name: "Update" }));

    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "Couldn't start the update: This confirmation is more than 10 minutes old, so nothing ran. Open it again and confirm.",
    );
    // The dead id is not retried on its own, and the dialog stays open so
    // the failure can be read rather than blinking away.
    expect(submittedPlanIds()).toEqual([{ planId: "1" }]);
    expect(calls("plan_operation")).toHaveLength(1);

    fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(queryByRole("dialog")).not.toBeInTheDocument());

    // Asking again plans again: a new id and a new preview, and still no
    // submit until the user confirms that preview.
    fireEvent.click((await findAllByRole("button", { name: "Update" }))[0]);
    dialog = await findByRole("dialog");
    showCommands(dialog);
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula glib");
    expect(calls("plan_operation")).toHaveLength(2);
    expect(submittedPlanIds()).toEqual([{ planId: "1" }]);

    fireEvent.click(within(dialog).getByRole("button", { name: "Update" }));

    await waitFor(() => expect(submittedPlanIds()).toEqual([{ planId: "1" }, { planId: "2" }]));
  });

  it("shows one item's planning failure in the dialog while the other stays submittable", async () => {
    planFailures.glib = "glib is pinned";
    const { findAllByRole, getByRole, findByRole, queryByRole } = renderPage();

    const checkboxes = await findAllByRole("checkbox", { name: ROW_CHECKBOX });
    fireEvent.click(checkboxes[0]);
    fireEvent.click(checkboxes[1]);
    fireEvent.click(getByRole("button", { name: /^Update Selected/ }));

    const dialog = await findByRole("dialog");
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "Couldn't prepare the update: glib is pinned",
    );
    // One tool can still be updated: the sheet asks about it by name.
    expect(dialog).toHaveAccessibleName("Update “onyx”?");
    showCommands(dialog);
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --cask onyx");
    expect(
      within(dialog).queryByText("/opt/homebrew/bin/brew upgrade --formula glib"),
    ).not.toBeInTheDocument();

    fireEvent.click(within(dialog).getByRole("button", { name: "Update" }));

    // onyx holds the only issued plan (glib never received an id). The batch
    // had a failure, so the dialog stays open with the outcome per item, and
    // only the item that started leaves the selection.
    await waitFor(() => expect(submittedPlanIds()).toEqual([{ planId: "1" }]));
    await within(dialog).findByText("Started");
    expect(useUiStore.getState().selectedUpdates).toEqual(["brew:/opt/homebrew|Formula|glib"]);

    fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("localises a stale-snapshot NotActionable refusal instead of showing the backend's JSON", async () => {
    // glib's row is actionable in this snapshot (brew is writable and
    // answering), but a genuine TOCTOU or a stale snapshot can still make
    // `Session::issue_plan`'s gate refuse it between the click and the
    // reply. The backend's rejection is JSON, not English -- see
    // `plan_operation_error` in src-tauri/src/ipc.rs -- and it must never
    // reach the page verbatim. A single item whose only plan fails never
    // gets a dialog (nothing issued to preview): the reason goes straight
    // to the page as an alert.
    planFailures.glib = '{"kind":"not_actionable","read_only":null,"unavailable":"NotRunning"}';
    const { findAllByRole, findByRole, queryByRole } = renderPage();

    fireEvent.click((await findAllByRole("button", { name: "Update" }))[0]);

    const alert = await findByRole("alert");
    expect(alert).toHaveTextContent(
      "Couldn't prepare the update: Open Homebrew to see what it has and check for updates.",
    );
    expect(alert.textContent).not.toMatch(/not_actionable/);
    expect(queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("after one item starts and the next fails, a retry re-plans and submits only the failed one", async () => {
    submitFailures["2"] = '{"kind":"expired"}';
    const { findAllByRole, getByRole, findByRole, queryByRole } = renderPage();

    const checkboxes = await findAllByRole("checkbox", { name: ROW_CHECKBOX });
    fireEvent.click(checkboxes[0]);
    fireEvent.click(checkboxes[1]);
    fireEvent.click(getByRole("button", { name: /^Update Selected/ }));

    let dialog = await findByRole("dialog");
    showCommands(dialog);
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --cask onyx");
    fireEvent.click(within(dialog).getByRole("button", { name: "Update" }));

    // glib started, onyx did not, and the dialog says which is which.
    await within(dialog).findByText("Started");
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "Couldn't start the update: This confirmation is more than 10 minutes old, so nothing ran. Open it again and confirm.",
    );
    expect(submittedPlanIds()).toEqual([{ planId: "1" }, { planId: "2" }]);
    // A started item leaves the selection at once; the failed one stays.
    expect(useUiStore.getState().selectedUpdates).toEqual(["brew:/opt/homebrew|Cask|onyx"]);

    fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(queryByRole("dialog")).not.toBeInTheDocument());

    // Retry: plan_operation is asked exactly once more, for onyx only — a
    // fresh id must never re-queue the item that already started.
    fireEvent.click(getByRole("button", { name: "Update Selected (1)" }));
    dialog = await findByRole("dialog");
    showCommands(dialog);
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --cask onyx");
    const secondRound = calls("plan_operation").slice(2);
    expect(secondRound).toHaveLength(1);
    expect(secondRound[0][1]).toEqual({
      request: {
        kind: "Upgrade",
        instance_id: "brew:/opt/homebrew",
        artifact_kind: "Cask",
        name: "onyx",
      },
    });

    fireEvent.click(within(dialog).getByRole("button", { name: "Update" }));

    await waitFor(() =>
      expect(submittedPlanIds()).toEqual([{ planId: "1" }, { planId: "2" }, { planId: "3" }]),
    );
  });

  it("locks the dialog while submitting and drops a superseded batch's late reply", async () => {
    holdPlans.add("glib");
    holdSubmits.add("2");
    const { findAllByRole, findByRole, getByRole, queryByRole } = renderPage();

    // Nothing is ticked, so the toolbar's one action is Update all, which
    // has two rows to update: the lock below is what disables it, not an
    // empty list.
    const updateButtons = await findAllByRole("button", { name: "Update" });
    const updateAll = getByRole("button", { name: "Update All" });

    // Batch 1 (glib) is still planning when it is closed and batch 2 (onyx)
    // opens the dialog. Planning has no side effect beyond issuing a PlanId
    // that expires on its own, so the newer batch supersedes the older one.
    // The sheet is up from the first press, over the page, so the second
    // press comes after Cancel.
    fireEvent.click(updateButtons[0]);
    const first = await findByRole("dialog", { name: "Update “glib”?" });
    fireEvent.click(within(first).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(queryByRole("dialog")).not.toBeInTheDocument());
    fireEvent.click(updateButtons[1]);
    const dialog = await findByRole("dialog", { name: "Update “onyx”?" });
    showCommands(dialog);
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --cask onyx");
    await waitFor(() => expect(releasePlan.glib).toBeDefined());

    // Batch 1's plan lands late. It must neither replace nor add to the
    // preview the user is looking at, and must not close it.
    await act(async () => {
      releasePlan.glib();
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    expect(queryByRole("dialog")).toBe(dialog);
    expect(
      within(dialog).queryByText("/opt/homebrew/bin/brew upgrade --formula glib"),
    ).not.toBeInTheDocument();
    expect(within(dialog).getByText("/opt/homebrew/bin/brew upgrade --cask onyx")).toBeInTheDocument();

    fireEvent.click(within(dialog).getByRole("button", { name: "Update" }));

    // Submitting: nothing closes the dialog or starts another batch until
    // this one has settled — not Cancel, not Escape, not the toolbar's
    // Update all, not a row's Update.
    await waitFor(() =>
      expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeDisabled(),
    );
    expect(within(dialog).getByRole("button", { name: "Update" })).toBeDisabled();
    expect(updateAll).toBeDisabled();
    expect(updateButtons[0]).toBeDisabled();
    fireEvent.keyDown(dialog, { key: "Escape" });
    expect(queryByRole("dialog")).toBe(dialog);

    await waitFor(() => expect(releaseSubmit["2"]).toBeDefined());
    releaseSubmit["2"]();
    await waitFor(() => expect(queryByRole("dialog")).not.toBeInTheDocument());
    expect(submittedPlanIds()).toEqual([{ planId: "2" }]);
    expect(updateAll).not.toBeDisabled();
    // Ticked, glib is what the one button updates, and that is not locked either.
    fireEvent.click(getByRole("checkbox", { name: "Select glib for update" }));
    expect(getByRole("button", { name: "Update Selected (1)" })).not.toBeDisabled();
  });

  describe("the toolbar", () => {
    it("counts the rows it can update, and its one button turns from Update all to Update selected with the ticked ones", async () => {
      updates = [...snapshot.updates, brewCandidate("jq"), { ...brewCandidate("wget"), blocked: "Pinned" }];
      const { findByText, getByRole, getAllByRole, queryByRole, container } = renderPage();

      await findByText("3 Updates Available");
      // In the toolbar, not on the page, and the only button there.
      const toolbar = container.querySelector("[data-toolbar-slot]") as HTMLElement;
      const labels = () =>
        within(toolbar)
          .getAllByRole("button")
          .map((button) => button.querySelector("[data-button-label]")?.textContent);
      expect(labels()).toEqual(["Update All"]);
      expect(queryByRole("button", { name: /^Update Selected/ })).not.toBeInTheDocument();
      // Nor does the page say how many again: the toolbar's subtitle does.
      expect(screen.getAllByText("3 Updates Available")).toHaveLength(1);

      fireEvent.click(getAllByRole("checkbox", { name: ROW_CHECKBOX })[0]);
      expect(labels()).toEqual(["Update Selected (1)"]);
      fireEvent.click(getAllByRole("checkbox", { name: ROW_CHECKBOX })[2]);
      expect(getByRole("button", { name: "Update Selected (2)" })).toBeEnabled();
      // The accent, as Update all's: the one thing the screen asks for.
      expect(getByRole("button", { name: "Update Selected (2)" }).className).toBe(BUTTON.regular.default);
      expect(queryByRole("button", { name: "Update All" })).not.toBeInTheDocument();

      // Unticked again, back to Update all.
      fireEvent.click(getAllByRole("checkbox", { name: ROW_CHECKBOX })[0]);
      fireEvent.click(getAllByRole("checkbox", { name: ROW_CHECKBOX })[2]);
      expect(labels()).toEqual(["Update All"]);
    });

    it("keeps its one button as wide as it is at its widest, so nothing beside it moves as its words change", async () => {
      updates = [...snapshot.updates, brewCandidate("jq")];
      const { findByText, getByRole, getAllByRole, container } = renderPage();

      await findByText("3 Updates Available");
      const toolbar = container.querySelector("[data-toolbar-slot]") as HTMLElement;
      // Laid under its words, unseen and unheard: everything it can say,
      // the count at every row ticked.
      const sizers = () =>
        [...toolbar.querySelectorAll("[data-button-sizer]")].map((sizer) => {
          expect(sizer).toHaveAttribute("aria-hidden", "true");
          expect(sizer).toHaveClass("invisible", "col-start-1", "row-start-1");
          return sizer.textContent;
        });
      expect(sizers()).toEqual(["Update All", "Update Selected (3)"]);
      const all = getByRole("button", { name: "Update All" });
      // In the one grid cell with its words, digits of one width.
      const grid = all.querySelector("[data-button-label]")?.parentElement as HTMLElement;
      expect(grid).toHaveClass("grid", "justify-items-center", "tabular-nums");
      expect(all.querySelector("[data-button-label]")).toHaveClass("col-start-1", "row-start-1");

      // Ticked, the same under other words: the same width.
      fireEvent.click(getAllByRole("checkbox", { name: ROW_CHECKBOX })[0]);
      expect(getByRole("button", { name: "Update Selected (1)" })).toBeInTheDocument();
      expect(sizers()).toEqual(["Update All", "Update Selected (3)"]);
    });

    it("says how many in the toolbar as Latest does in English, one or several", async () => {
      updates = [snapshot.updates[0]];
      const { findByText, unmount } = renderPage();
      await findByText("1 Update Available");
      unmount();

      updates = [...snapshot.updates, brewCandidate("jq")];
      const again = renderPage();
      await again.findByText("3 Updates Available");
    });

    it("says 更新所选（N） in Chinese once rows are ticked", async () => {
      await i18n.changeLanguage("zh-CN");
      try {
        const { findAllByRole, getByRole } = renderPage();
        for (const box of await findAllByRole("checkbox", { name: /^选择要更新的/ })) fireEvent.click(box);
        expect(getByRole("button", { name: "更新所选（2）" })).toBeEnabled();
      } finally {
        await i18n.changeLanguage("en");
      }
    });

    it("Update all ticks every row it can update and opens the same confirmation with exactly those", async () => {
      // 360's 全部更新: one press for everything Canager can update here,
      // into the one confirmation Update selected uses -- the command for
      // each, then one operation per item on Confirm.
      updates = [
        ...snapshot.updates,
        brewCandidate("jq"),
        { ...brewCandidate("wget"), blocked: "Pinned" },
        {
          key: urllib3Key,
          current: "2.2.1",
          target: "2.3.0",
          channel: "Registry",
          checkable: true,
          warnings: [],
          blocked: null,
        },
      ];
      const { getByRole, findByRole, queryByRole, findByText } = renderPage();

      await findByText("3 Updates Available");
      fireEvent.click(getByRole("button", { name: "Update All" }));

      const dialog = await findByRole("dialog");
      showCommands(dialog);
      await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula jq");
      expect([...plannedNames()].sort()).toEqual(["glib", "jq", "onyx"]);
      // In the list's order.
      expect(within(dialog).getAllByText(/brew upgrade/).map((command) => command.textContent)).toEqual([
        "/opt/homebrew/bin/brew upgrade --formula glib",
        "/opt/homebrew/bin/brew upgrade --formula jq",
        "/opt/homebrew/bin/brew upgrade --cask onyx",
      ]);
      expect([...useUiStore.getState().selectedUpdates].sort()).toEqual(
        [glibKey, { ...glibKey, name: "jq" }, onyxKey].map(artifactKeyId).sort(),
      );
      expect(submittedPlanIds()).toEqual([]);

      fireEvent.click(within(dialog).getByRole("button", { name: "Update" }));
      await waitFor(() => expect(submittedPlanIds()).toHaveLength(3));
      await waitFor(() => expect(queryByRole("dialog")).not.toBeInTheDocument());
      expect(useUiStore.getState().selectedUpdates).toEqual([]);
    });

    it("says how many are updating, and never that nothing can be updated, while every row it could update is", async () => {
      operations = [
        operation(onyxKey, { id: 8, status: "Queued" }),
        operation(glibKey, { id: 7, status: "Running" }),
      ];
      const { findByText, getByRole, queryByText } = renderPage();

      expect(await findByText("Updating 2 tools")).toBeInTheDocument();
      expect(queryByText("Nothing to update here")).toBeNull();
      expect(getByRole("button", { name: "Update All" })).toBeDisabled();
    });

    it("says it in Chinese in the Overview's words, and how many more can be updated", async () => {
      await i18n.changeLanguage("zh-CN");
      try {
        updates = [...snapshot.updates, brewCandidate("jq")];
        operations = [operation(glibKey, { id: 7, status: "Running" })];
        const { findByText } = renderPage();

        expect(await findByText("正在更新1个工具，另有2个可更新")).toBeInTheDocument();
      } finally {
        await i18n.changeLanguage("en");
      }
    });

    it("offers Update all only when there is something it could update", async () => {
      updates = [{ ...snapshot.updates[0], blocked: "Pinned" }];
      const { findByText, getByRole } = renderPage();

      await findByText("Nothing to update here");
      expect(getByRole("button", { name: "Update All" })).toBeDisabled();
    });

    it("calls them 全部更新 and 更新所选 in Chinese, as they were asked for", () => {
      expect(zhCN.updates.updateAll).toBe("全部更新");
      expect(zhCN.updates.updateSelectedCount).toBe("更新所选（{{number}}）");
      expect(zhCN.updates.count_other).toBe("{{count}}个可更新");
      expect(zhCN.updates.cantUpdateHere).toBe("另有{{number}}个无法在这里更新");
    });
  });

  describe("Can't update here", () => {
    it("folds the rows it cannot update into one line at the bottom, and unfolds them on a press", async () => {
      updates = [
        { ...snapshot.updates[0], blocked: "Pinned" },
        snapshot.updates[1],
        {
          key: urllib3Key,
          current: "2.2.1",
          target: "2.3.0",
          channel: "Registry",
          checkable: true,
          warnings: [],
          blocked: null,
        },
      ];
      const { findByRole, queryByText } = renderPage();

      const toggle = await findByRole("button", { name: "2 more can't be updated here" });
      expect(toggle).toHaveAttribute("aria-expanded", "false");
      expect(queryByText("glib", { selector: "[data-tool-row] p" })).toBeNull();
      expect(queryByText("urllib3", { selector: "[data-tool-row] p" })).toBeNull();
      // Below the rows it can update.
      expect(slotOf(toggle)).toBeGreaterThan(slotOf(rowOf("onyx")) ?? Infinity);

      fireEvent.click(toggle);

      expect(toggle).toHaveAttribute("aria-expanded", "true");
      const glib = await findRow("glib");
      expect(slotOf(glib)).toBeGreaterThan(slotOf(toggle) ?? Infinity);
      expect(slotOf(rowOf("urllib3"))).toBeGreaterThan(slotOf(toggle) ?? Infinity);
      // Sorted by name there too.
      expect(rowNames()).toEqual(["onyx", "glib", "urllib3"]);

      fireEvent.click(toggle);
      expect(queryByText("glib", { selector: "[data-tool-row] p" })).toBeNull();
    });

    it("is a 32pt line with a 10pt triangle and muted words, and its rows have only a name, a line, a word and a ⋯", async () => {
      updates = [{ ...snapshot.updates[0], blocked: "Pinned" }, snapshot.updates[1]];
      const { findByRole } = renderPage();

      const toggle = await findByRole("button", { name: "1 more can't be updated here" });
      expect(toggle.className.split(" ")).toEqual(expect.arrayContaining(["h-8", "px-5", "text-body", "text-muted"]));
      // Not the semibold heading it was.
      expect(toggle.className).not.toContain("font-semibold");
      // On the rows' grid, as the notices over it: the triangle centred in
      // the avatars' 32 column past a checkbox's room, the words where the
      // names start.
      const [slot, words] = [...toggle.children] as HTMLElement[];
      expect(slot.className.split(" ")).toEqual(expect.arrayContaining(["ml-7", "w-8", "justify-center", "shrink-0"]));
      expect(words.className.split(" ")).toContain("ml-3");
      expect(words.textContent).toBe("1 more can't be updated here");
      const triangle = slot.firstElementChild as SVGElement;
      expect(triangle.getAttribute("width")).toBe("10");
      expect(triangle.querySelector("path")?.getAttribute("fill")).toBe("currentColor");
      expect(triangle.getAttribute("class")).not.toContain("rotate-90");

      fireEvent.click(toggle);
      expect(triangle.getAttribute("class")).toContain("rotate-90");
      const glib = await findRow("glib");
      const onyx = rowOf("onyx");
      // The row it can update: a box, a version, a button and a ⋯.
      expect(within(onyx).getByRole("checkbox")).toBeInTheDocument();
      expect(within(onyx).getByText("5.0.2 → 5.1.0")).toBeInTheDocument();
      expect(within(onyx).getByRole("button", { name: "Update" })).toBeInTheDocument();
      // The row it can't: the word and the ⋯, and nothing else to press.
      expect(within(glib).queryByRole("checkbox")).toBeNull();
      expect(within(glib).queryByText(/→/)).toBeNull();
      expect(within(glib).getAllByRole("button").map((button) => button.getAttribute("aria-label") ?? button.textContent)).toEqual([
        "Pinned",
        "More actions for glib",
      ]);
      // Its avatar in the same column as the rows above: a checkbox's room, empty.
      expect(glib.querySelector(".w-4")?.childElementCount).toBe(0);
    });

    it("has nothing to fold when every row can be updated", async () => {
      const { queryByRole } = renderPage();

      await findRow("glib");
      expect(queryByRole("button", { name: /^\d+ more can't be updated here$/ })).toBeNull();
    });
  });

  describe("status chips", () => {
    it("says a row's source is not running, not responding, or will not run as root, and what to do", async () => {
      const ollama = { ...stoppedOllama };
      const brewAsRoot = {
        ...snapshot.instances[0],
        status: { unavailable: "RefusesAsRoot" as const, notes: [] },
      };
      const cargoSilent = {
        ...snapshot.instances[3],
        status: { unavailable: "NotResponding" as const, notes: [] },
      };
      instances = [brewAsRoot, snapshot.instances[1], snapshot.instances[2], cargoSilent, ollama];
      updates = [
        snapshot.updates[0],
        { key: qwenKey, current: "a", target: "b", channel: "Digest", checkable: true, warnings: [], blocked: null },
        {
          key: { ...myForkKey, name: "tokei" },
          current: "12.1.2",
          target: "13.0.1",
          channel: "Registry",
          checkable: true,
          warnings: [],
          blocked: null,
        },
      ];
      renderPage();

      await showCantUpdate();
      expect(chipDetail(await findRow("qwen3:8b"), "Can't update now")).toHaveTextContent(
        "Ollama isn't running. Open it, then click Check Again.",
      );
      expect(chipDetail(rowOf("tokei"), "Can't update now")).toHaveTextContent(
        "Cargo isn't responding. Click Check Again later.",
      );
      expect(chipDetail(rowOf("glib"), "Can't update now")).toHaveTextContent(
        "Homebrew doesn't work when this app runs as administrator. Quit, then open Canager again with a double-click.",
      );
    });

    it("keeps the chips' words and details in Chinese as the author asked", async () => {
      await i18n.changeLanguage("zh-CN");
      try {
        updates = [
          { ...snapshot.updates[0], blocked: "Pinned" },
          {
            key: urllib3Key,
            current: "2.2.1",
            target: "2.3.0",
            channel: "Registry",
            checkable: true,
            warnings: [],
            blocked: null,
          },
        ];
        const { findByRole } = renderPage();

        fireEvent.click(await findByRole("button", { name: "另有2个无法在这里更新" }));
        expect(
          within(chipDetail(await findRow("glib"), "已固定")).getByText(
            wholeSentence("它在Homebrew中固定了版本。要更新，请先在终端运行/opt/homebrew/bin/brew unpin glib。"),
          ),
        ).toBeInTheDocument();
        expect(chipDetail(rowOf("urllib3"), "仅供查看")).toHaveTextContent(
          "pip安装的内容只能在这里查看。改用pipx或uv安装Python工具，就能在这里更新和卸载。",
        );
      } finally {
        await i18n.changeLanguage("en");
      }
    });
  });

  // The saved settings of the one `set_settings` call a test expects.
  function savedSettings(): Settings {
    const saves = calls("set_settings");
    expect(saves).toHaveLength(1);
    return (saves[0][1] as { settings: Settings }).settings;
  }

  function chooseFromMenu(row: HTMLElement, item: string) {
    const menu = openMenu(row);
    fireEvent.click(within(menu).getByRole("menuitem", { name: item }));
  }

  it("hides the row when Never remind me is chosen from its menu, and saves its package, not a version", async () => {
    const { queryByText } = renderPage();

    chooseFromMenu(await findRow("glib"), "Stop Reminding Me");

    await waitFor(() => expect(queryByText("glib")).not.toBeInTheDocument());
    expect(savedSettings().ignored_updates).toEqual([glibKey]);
    expect(savedSettings().skipped_versions).toEqual([]);
    expect(queryByText("onyx")).toBeInTheDocument();
  });

  it("hides the row when Skip this version is chosen from its menu, and saves the version it offers", async () => {
    const { queryByText } = renderPage();

    chooseFromMenu(await findRow("glib"), "Skip This Version");

    await waitFor(() => expect(queryByText("glib")).not.toBeInTheDocument());
    expect(savedSettings().skipped_versions).toEqual([{ key: glibKey, version: "2.90.0" }]);
    expect(savedSettings().ignored_updates).toEqual([]);
    expect(queryByText("onyx")).toBeInTheDocument();
  });

  it("replaces a package's earlier skip when its next version is skipped", async () => {
    // glib 2.89.0 was skipped; the source now offers 2.90.0, so the row is
    // back. Skipping it again records 2.90.0 in place of 2.89.0, which
    // could never hide anything again; onyx's skip is left as it was.
    settings.skipped_versions = [
      { key: glibKey, version: "2.89.0" },
      { key: onyxKey, version: "5.0.9" },
    ];
    renderPage();

    chooseFromMenu(await findRow("glib"), "Skip This Version");

    await waitFor(() => expect(calls("set_settings")).toHaveLength(1));
    expect(savedSettings().skipped_versions).toEqual([
      { key: onyxKey, version: "5.0.9" },
      { key: glibKey, version: "2.90.0" },
    ]);
  });

  it("lists a skipped package again once its source offers another version", async () => {
    settings.skipped_versions = [{ key: glibKey, version: "2.89.0" }];
    const { findByText, findAllByRole } = renderPage();

    await findRow("glib");
    await findByText("2 Updates Available");
    expect(await findAllByRole("button", { name: "Update" })).toHaveLength(2);
  });

  it("leaves a skipped row out of the headline, Select all and Update selected, even one selected before", async () => {
    settings.skipped_versions = [{ key: glibKey, version: "2.90.0" }];
    act(() => {
      useUiStore.getState().toggleUpdate(glibKey);
    });
    const { findByText, queryByText, getByRole, queryByRole, findByRole } = renderPage();

    await findRow("onyx");
    expect(queryByText("glib")).not.toBeInTheDocument();
    expect(await findByText("1 Update Available")).toBeInTheDocument();
    // glib's selection outlived its row, and counts for nothing: nothing is
    // ticked, as far as the toolbar and the list's header are concerned.
    expect(queryByRole("button", { name: /^Update Selected/ })).not.toBeInTheDocument();
    expect(getByRole("button", { name: "Update All" })).toBeEnabled();
    const selectAll = getByRole("checkbox", { name: SELECT_ALL }) as HTMLInputElement;
    expect(selectAll).not.toBeChecked();
    expect(selectAll.indeterminate).toBe(false);

    fireEvent.click(selectAll);
    fireEvent.click(getByRole("button", { name: "Update Selected (1)" }));
    await findByRole("dialog");
    expect(plannedNames()).toEqual(["onyx"]);
  });

  it("offers Never remind me but no Skip this version on a row Canager could not check", async () => {
    // An uncheckable row's `target` is its installed version, not one the
    // source offered, so there is no version to skip.
    updates = [
      snapshot.updates[0],
      {
        key: myForkKey,
        current: "0.1.0",
        target: "0.1.0",
        channel: "Registry",
        checkable: false,
        warnings: ["NonRegistrySource"],
        blocked: null,
      },
    ];
    renderPage();

    await showCantUpdate();
    const myForkMenu = openMenu(await findRow("my-fork"));
    expect(
      within(myForkMenu).getByRole("menuitem", { name: "Stop Reminding Me" }),
    ).toBeInTheDocument();
    expect(within(myForkMenu).queryByRole("menuitem", { name: "Skip This Version" })).toBeNull();
    fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });

    const glibMenu = openMenu(rowOf("glib"));
    expect(within(glibMenu).getByRole("menuitem", { name: "Skip This Version" })).toBeInTheDocument();
  });

  // A Homebrew cask declared `version :latest`, as `brew outdated --json=v2
  // --greedy` lists one -- Show self-updating apps is what makes Canager
  // pass `--greedy` -- whenever it takes its download to have changed:
  // `latest -> latest`, for every release.
  const chromiumKey: ArtifactKey = {
    instance_id: "brew:/opt/homebrew",
    kind: "Cask",
    name: "chromium",
  };
  const latestCask: Snapshot["updates"][number] = {
    key: chromiumKey,
    current: "latest",
    target: "latest",
    channel: "Native",
    checkable: true,
    warnings: [],
    blocked: null,
  };

  it("offers Never remind me but no Skip this version on a Homebrew cask declared version :latest", async () => {
    // A skip of "latest" would hide each later release as well and never
    // end: Never remind me, behind a hint that promises a reminder when the
    // next version is out.
    settings.include_self_updating = true;
    updates = [snapshot.updates[0], latestCask];
    renderPage();

    const chromium = await findRow("chromium");
    expect(within(chromium).getByRole("button", { name: "Update" })).toBeInTheDocument();
    const menu = openMenu(chromium);
    expect(
      within(menu).getByRole("menuitem", { name: "Stop Reminding Me" }),
    ).toBeInTheDocument();
    expect(within(menu).queryByRole("menuitem", { name: "Skip This Version" })).toBeNull();
    fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });

    const glibMenu = openMenu(rowOf("glib"));
    expect(within(glibMenu).getByRole("menuitem", { name: "Skip This Version" })).toBeInTheDocument();
  });

  it("lists a Homebrew cask declared version :latest even with a skip of latest in its settings", async () => {
    settings.include_self_updating = true;
    settings.skipped_versions = [{ key: chromiumKey, version: "latest" }];
    updates = [snapshot.updates[0], latestCask];
    const { findByText, findAllByRole } = renderPage();

    await findRow("chromium");
    await findByText("2 Updates Available");
    expect(await findAllByRole("button", { name: "Update" })).toHaveLength(2);
  });

  it("skips an Ollama model's new version by its digest without ever printing the digest", async () => {
    settings.show_technical_details = true;
    instances = [...snapshot.instances, { ...stoppedOllama, status: { unavailable: null, notes: [] } }];
    const digest = "sha256:9f1c0b6d2e4a7c5b3d1f8a6e4c2b0d9f7e5c3a1b8d6f4e2c0a9b7d5f3e1c8a6b";
    updates = [
      {
        key: qwenKey,
        current: "5642e97495e1a0888838ee1b3b1a0b1c6a0f0f5e6c2d4a8b9e7c3d1f0a2b4c6d",
        target: digest,
        channel: "Digest",
        checkable: true,
        warnings: [],
        blocked: null,
      },
    ];
    const { findByText, container } = renderPage();

    chooseFromMenu(await findRow("qwen3:8b"), "Skip This Version");

    await findByText("No updates to handle");
    expect(savedSettings().skipped_versions).toEqual([{ key: qwenKey, version: digest }]);
    expect(container.textContent).not.toMatch(/sha256|5642e974/);
  });

  it("says on each menu item what it will do", async () => {
    renderPage();

    const menu = openMenu(await findRow("glib"));
    expect(within(menu).getByRole("menuitem", { name: "Skip This Version" })).toHaveAccessibleDescription(
      "You'll be reminded when the next version comes out.",
    );
    expect(
      within(menu).getByRole("menuitem", { name: "Stop Reminding Me" }),
    ).toHaveAccessibleDescription(
      "You won't be reminded about any update to this tool. Undo it in Settings.",
    );
  });

  it("names each row's menu after the row", async () => {
    const { getByRole } = renderPage();

    await findRow("glib");
    expect(getByRole("button", { name: "More actions for glib" })).toHaveAttribute(
      "aria-haspopup",
      "menu",
    );
    expect(getByRole("button", { name: "More actions for onyx" })).toBeInTheDocument();
  });

  it("calls them 跳过此版本 and 不再提醒 in Chinese, and says what each does", () => {
    expect(zhCN.updates.skipVersion).toBe("跳过此版本");
    expect(zhCN.updates.skipVersionHint).toBe("下个版本发布时再提醒你。");
    expect(zhCN.updates.neverRemind).toBe("不再提醒");
    expect(zhCN.updates.neverRemindHint).toBe("以后不再提醒此工具的任何更新。可以在“设置”中撤销。");
    expect(zhCN.common.copyCommand).toBe("拷贝命令");
  });

  it("disables both hiding items on every row while a save is pending so a second choice cannot overwrite the first", async () => {
    holdSaves = true;
    const { findByText, queryByText } = renderPage();

    chooseFromMenu(await findRow("glib"), "Skip This Version");
    await waitFor(() => expect(calls("set_settings")).toHaveLength(1));

    // Every item locks until the first save settles. A second choice now
    // would build its settings from the same stale base, and the later save
    // would drop the earlier one.
    const menu = openMenu(rowOf("onyx"));
    const skip = within(menu).getByRole("menuitem", { name: "Skip This Version" });
    const never = within(menu).getByRole("menuitem", { name: "Stop Reminding Me" });
    expect(skip).toHaveAttribute("aria-disabled", "true");
    expect(never).toHaveAttribute("aria-disabled", "true");
    fireEvent.click(skip);
    fireEvent.click(never);
    expect(calls("set_settings")).toHaveLength(1);
    fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });

    releaseSave[0]();
    await waitFor(() => expect(queryByText("glib")).not.toBeInTheDocument());
    await findByText("onyx");
    const after = openMenu(rowOf("onyx"));
    await waitFor(() =>
      expect(within(after).getByRole("menuitem", { name: "Skip This Version" })).not.toHaveAttribute(
        "aria-disabled",
      ),
    );
    expect(
      within(after).getByRole("menuitem", { name: "Stop Reminding Me" }),
    ).not.toHaveAttribute("aria-disabled");
    expect(calls("set_settings")).toHaveLength(1);
  });

  it("shows the backend's message when saving the choice fails", async () => {
    saveFailure = "settings.json is read-only";
    const { findByRole } = renderPage();

    chooseFromMenu(await findRow("glib"), "Skip This Version");

    expect(await findByRole("alert")).toHaveTextContent(
      "Couldn't save that choice: settings.json is read-only",
    );
    // Nothing was saved, so nothing disappears.
    expect(rowOf("glib")).toBeInTheDocument();
  });

  describe("Copy Command", () => {
    let writeText: ReturnType<typeof vi.fn>;

    beforeEach(() => {
      writeText = vi.fn().mockResolvedValue(undefined);
      Object.defineProperty(navigator, "clipboard", {
        value: { writeText },
        configurable: true,
      });
    });

    afterEach(() => {
      Object.defineProperty(navigator, "clipboard", { value: undefined, configurable: true });
    });

    it("copies a pinned row's unpin command, with technical details on, and says it did", async () => {
      settings.show_technical_details = true;
      updates = [{ ...snapshot.updates[0], blocked: "Pinned" }, snapshot.updates[1]];
      const { findByRole } = renderPage();

      await showCantUpdate();
      chooseFromMenu(await findRow("glib"), "Copy Command");

      expect(writeText).toHaveBeenCalledWith("/opt/homebrew/bin/brew unpin glib");
      expect(await findByRole("status")).toHaveTextContent("Copied");
    });

    it("stands in a group of its own, under a hairline, apart from the choices about the update", async () => {
      settings.show_technical_details = true;
      updates = [{ ...snapshot.updates[0], blocked: "Pinned" }];
      renderPage();

      await showCantUpdate();
      const menu = openMenu(await findRow("glib"));
      const separator = within(menu).getByRole("separator");
      expect(separator.nextElementSibling).toHaveTextContent("Copy Command");
      expect(separator.previousElementSibling).toHaveAttribute("role", "menuitem");
    });

    it("says so when the clipboard refuses", async () => {
      settings.show_technical_details = true;
      writeText.mockRejectedValue(new Error("denied"));
      updates = [{ ...snapshot.updates[0], blocked: "Pinned" }];
      const { findByText } = renderPage();

      await showCantUpdate();
      chooseFromMenu(await findRow("glib"), "Copy Command");

      expect(await findByText("Couldn't copy")).toBeInTheDocument();
    });

    it("is not offered with technical details off, nor where the command would need a preview", async () => {
      updates = [{ ...snapshot.updates[0], blocked: "Pinned" }, snapshot.updates[1]];
      const view = renderPage();

      await showCantUpdate();
      expect(within(openMenu(await findRow("glib"))).queryByRole("menuitem", { name: "Copy Command" })).toBeNull();
      view.unmount();

      // With the switch on, an updatable row's command is only known from
      // its plan, which the menu does not ask for.
      settings.show_technical_details = true;
      renderPage();
      expect(within(openMenu(await findRow("onyx"))).queryByRole("menuitem", { name: "Copy Command" })).toBeNull();
      expect(calls("plan_operation")).toHaveLength(0);
    });
  });

  describe("an update's progress, in its row", () => {
    // `list_operations` newest first, as the backend sends it. The page
    // remembers the version each update it started was for.
    function started(opId: number, target: string) {
      useUiStore.setState({ updateTargets: { ...useUiStore.getState().updateTargets, [opId]: target } });
    }

    const cases: Array<[string, Partial<OpSummary>, string]> = [
      ["waiting its turn", { status: "Queued" }, "Queued"],
      ["running", { status: "Running" }, "Updating…"],
      ["being checked afterwards", { status: "Verifying" }, "Updating…"],
      ["being cancelled", { status: "CancelRequested" }, "Cancelling…"],
    ];

    it.each(cases)("says an update %s in place of the Update button", async (_name, fields, text) => {
      operations = [operation(glibKey, fields)];
      renderPage();

      const glib = await findRow("glib");
      expect(await within(glib).findByText(text)).toBeInTheDocument();
      expect(within(glib).queryByRole("button", { name: "Update" })).toBeNull();
      // onyx's row is untouched.
      expect(within(rowOf("onyx")).getByRole("button", { name: "Update" })).toBeInTheDocument();
    });

    // What the header says: a row an update is installing is said in
    // words, never counted as one more that can be updated.
    const takesRow: Array<[string, Partial<OpSummary>, string]> = [
      ["under way", { status: "Running" }, "Updating 1 tool, 1 more can be updated"],
      ["that worked", { status: "Done", outcome: "Succeeded" }, "1 Update Available"],
    ];

    it.each(takesRow)("leaves a row with an update %s out of its checkbox, the count, the header's box and Update all", async (_name, fields, header) => {
      operations = [operation(glibKey, fields)];
      started(7, "2.90.0");
      // Ticked before its update started.
      useUiStore.setState({ selectedUpdates: [artifactKeyId(glibKey)] });
      const { findByText, getByRole, queryByRole } = renderPage();

      const glib = await findRow("glib");
      expect(await findByText(header)).toBeInTheDocument();
      expect(within(glib).queryByRole("checkbox")).toBeNull();
      expect(within(rowOf("onyx")).getByRole("checkbox")).toBeInTheDocument();
      // glib's tick counts for nothing.
      expect(queryByRole("button", { name: /^Update Selected/ })).not.toBeInTheDocument();
      const selectAll = getByRole("checkbox", { name: SELECT_ALL }) as HTMLInputElement;
      expect(selectAll).not.toBeChecked();
      expect(selectAll.indeterminate).toBe(false);

      // Ticks onyx, the one row with a checkbox, and leaves glib's own as it was.
      fireEvent.click(selectAll);
      expect(useUiStore.getState().selectedUpdates).toEqual([artifactKeyId(glibKey), artifactKeyId(onyxKey)]);
      expect(selectAll).toBeChecked();
      expect(getByRole("button", { name: "Update Selected (1)" })).toBeEnabled();
      // And unticks onyx alone.
      fireEvent.click(selectAll);
      expect(useUiStore.getState().selectedUpdates).toEqual([artifactKeyId(glibKey)]);

      fireEvent.click(getByRole("button", { name: "Update All" }));
      await waitFor(() => expect(plannedNames()).toEqual(["onyx"]));
    });

    it("keeps the checkbox of a row whose update failed, for Retry", async () => {
      operations = [
        operation(glibKey, { status: "Done", outcome: { Failed: { exit_code: 1, summary: "Error: no bottle" } } }),
      ];
      started(7, "2.90.0");
      const { findByText, getByRole } = renderPage();

      const glib = await findRow("glib");
      expect(await within(glib).findByText("Couldn't update")).toBeInTheDocument();
      expect(await findByText("2 Updates Available")).toBeInTheDocument();
      expect(within(glib).getByRole("checkbox")).toBeInTheDocument();
      fireEvent.click(getByRole("checkbox", { name: SELECT_ALL }));
      expect(useUiStore.getState().selectedUpdates).toEqual([artifactKeyId(glibKey), artifactKeyId(onyxKey)]);
    });

    it("ticks a finished update Updated, while its row still offers the version it was for", async () => {
      operations = [operation(glibKey, { status: "Done", outcome: "Succeeded" })];
      started(7, "2.90.0");
      renderPage();

      const glib = await findRow("glib");
      expect(await within(glib).findByText("Updated")).toBeInTheDocument();
      expect(glib.querySelector("svg")).not.toBeNull();
      expect(within(glib).queryByRole("button", { name: "Update" })).toBeNull();
    });

    it("gives the Update button back once the row offers a newer version than a finished update was for", async () => {
      // glib was updated to 2.90.0; the source offers 2.91.0 now. "Updated"
      // would be about a version that is not the one offered.
      // onyx's running update is there to show the list has arrived.
      operations = [
        operation(onyxKey, { id: 8, status: "Running" }),
        operation(glibKey, { status: "Done", outcome: "Succeeded" }),
      ];
      started(7, "2.89.0");
      renderPage();

      const glib = await findRow("glib");
      expect(await within(rowOf("onyx")).findByText("Updating…")).toBeInTheDocument();
      expect(within(glib).getByRole("button", { name: "Update" })).toBeInTheDocument();
      expect(within(glib).queryByText("Updated")).toBeNull();
    });

    it("does not show a finished update it has no record of starting", async () => {
      operations = [
        operation(onyxKey, { id: 8, status: "Running" }),
        operation(glibKey, { status: "Done", outcome: "Succeeded" }),
      ];
      renderPage();

      const glib = await findRow("glib");
      expect(await within(rowOf("onyx")).findByText("Updating…")).toBeInTheDocument();
      expect(within(glib).getByRole("button", { name: "Update" })).toBeInTheDocument();
      expect(within(glib).queryByText("Updated")).toBeNull();
    });

    it("says a failed update failed, with a way to its log", async () => {
      operations = [
        operation(glibKey, {
          id: 9,
          status: "Done",
          outcome: { Failed: { exit_code: 1, summary: "Error: glib: no bottle" } },
        }),
      ];
      started(9, "2.90.0");
      const { getByRole } = renderPage();

      const glib = await findRow("glib");
      expect(await within(glib).findByText("Couldn't update")).toBeInTheDocument();
      fireEvent.click(getByRole("button", { name: "View log: glib" }));
      expect(useUiStore.getState().focusedOpId).toBe(9);
      expect(useUiStore.getState().drawerOpen).toBe(true);
    });

    it("says why an update failed, where the tool's own words say, in the red for text, beside Retry", async () => {
      operations = [
        operation(glibKey, {
          id: 9,
          status: "Done",
          outcome: {
            Failed: {
              exit_code: 1,
              summary: 'curl: (6) Could not resolve host: ghcr.io\nError: glib: Failed to download resource "glib (2.90.0)"',
            },
          },
        }),
      ];
      started(9, "2.90.0");
      const { getByRole } = renderPage();

      const glib = await findRow("glib");
      const word = await within(glib).findByText("Connection failed");
      expect(within(glib).queryByText("Couldn't update")).toBeNull();
      // The tool's raw words stay in its log, not in the row.
      expect(within(glib).queryByText(/Could not resolve host/)).toBeNull();
      const toLog = getByRole("button", { name: "View log: glib" });
      expect(toLog).toContainElement(word);
      expect(toLog).toHaveAccessibleDescription("Connection failed");
      expect(toLog.className).toContain("text-danger-text");
      expect(toLog.className).toContain("text-small");
      expect(within(glib).getByRole("button", { name: "Retry" })).toBeInTheDocument();
      fireEvent.click(toLog);
      expect(useUiStore.getState().focusedOpId).toBe(9);
    });

    it("draws its progress as a Mac list does: a 16 spinner in muted words, and a 12 green tick beside Updated", async () => {
      operations = [operation(glibKey, { status: "Running" })];
      const view = renderPage();
      const running = await within(await findRow("glib")).findByText("Updating…");
      expect(running.className).toContain("text-muted");
      expect(running.querySelector("svg")).toHaveAttribute("width", "16");
      view.unmount();

      operations = [operation(glibKey, { id: 3, status: "Done", outcome: "Succeeded" })];
      started(3, "2.90.0");
      renderPage();
      const done = await within(await findRow("glib")).findByText("Updated");
      expect(done.className).toContain("text-foreground");
      const tick = done.querySelector("svg");
      expect(tick).toHaveAttribute("width", "12");
      expect(tick?.getAttribute("class")).toContain("text-success");
    });

    it("says a cancelled update was cancelled", async () => {
      operations = [operation(glibKey, { status: "Done", outcome: "Cancelled" })];
      started(7, "2.90.0");
      renderPage();

      expect(await within(await findRow("glib")).findByText("Cancelled")).toBeInTheDocument();
    });

    it.each<[string, OpSummary["outcome"]]>([
      ["could not be confirmed", "Unconfirmed"],
      ["needs attention", { NeedsAttention: "UnchangedAfterUpgrade" }],
    ])("asks to check an update whose result %s, with a way to its log", async (_name, outcome) => {
      operations = [operation(glibKey, { id: 11, status: "Done", outcome })];
      started(11, "2.90.0");
      const { getByRole } = renderPage();

      const word = await within(await findRow("glib")).findByText("Unexpected result");
      // 12 orange ⚠︎ and the word in the label colour.
      const toLog = getByRole("button", { name: "View log: glib" });
      expect(toLog.className).toContain("text-foreground");
      expect(word.previousElementSibling?.getAttribute("class")).toContain("text-warning");
      expect(word.previousElementSibling).toHaveAttribute("width", "12");
      fireEvent.click(toLog);
      expect(useUiStore.getState().focusedOpId).toBe(11);
    });

    const endings: Array<[string, OpSummary["outcome"], string]> = [
      ["failed", { Failed: { exit_code: 1, summary: "Error: glib: no bottle" } }, "Couldn't update"],
      ["was cancelled", "Cancelled", "Cancelled"],
      ["asks to be checked", { NeedsAttention: "UnchangedAfterUpgrade" }, "Unexpected result"],
    ];

    it.each(endings)(
      "offers Retry where Update was once an update %s, with how it ended still in the row",
      async (_name, outcome, text) => {
        operations = [operation(glibKey, { id: 9, status: "Done", outcome })];
        started(9, "2.90.0");
        renderPage();

        const glib = await findRow("glib");
        expect(await within(glib).findByText(text)).toBeInTheDocument();
        const retry = within(glib).getByRole("button", { name: "Retry" });
        // The look Update had: a row's regular grey button (`RowAction`).
        expect(retry.className).toBe(BUTTON.regular.grey);
        expect(within(glib).queryByRole("button", { name: "Update" })).toBeNull();
        // Still a row it can update: its checkbox stays.
        expect(within(glib).getByRole("checkbox")).toBeInTheDocument();
        expect(within(rowOf("onyx")).queryByRole("button", { name: "Retry" })).toBeNull();
      },
    );

    it("opens the confirmation for that row alone on Retry, and keeps the way to the failure's log", async () => {
      operations = [
        operation(glibKey, {
          id: 9,
          status: "Done",
          outcome: { Failed: { exit_code: 1, summary: "Error: glib: no bottle" } },
        }),
      ];
      started(9, "2.90.0");
      const { findByRole, getByRole } = renderPage();

      const glib = await findRow("glib");
      fireEvent.click(await within(glib).findByRole("button", { name: "Retry" }));
      const dialog = await findByRole("dialog", { name: "Update “glib”?" });
      await waitFor(() => expect(plannedNames()).toEqual(["glib"]));
      fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));

      fireEvent.click(getByRole("button", { name: "View log: glib" }));
      expect(useUiStore.getState().focusedOpId).toBe(9);
      expect(useUiStore.getState().drawerOpen).toBe(true);
    });

    it("offers no Retry beside a tick", async () => {
      operations = [operation(glibKey, { status: "Done", outcome: "Succeeded" })];
      started(7, "2.90.0");
      renderPage();

      const glib = await findRow("glib");
      expect(await within(glib).findByText("Updated")).toBeInTheDocument();
      expect(within(glib).queryByRole("button", { name: "Retry" })).toBeNull();
      expect(within(glib).queryByRole("button", { name: "Update" })).toBeNull();
    });

    it("offers no Retry on a failed row it can no longer update, only how it ended and its log", async () => {
      // Pinned since the update failed: Homebrew would refuse a second try.
      updates = [{ ...snapshot.updates[0], blocked: "Pinned" }, snapshot.updates[1]];
      operations = [
        operation(glibKey, {
          id: 9,
          status: "Done",
          outcome: { Failed: { exit_code: 1, summary: "Error: glib is pinned" } },
        }),
      ];
      started(9, "2.90.0");
      renderPage();

      await showCantUpdate();
      const glib = await findRow("glib");
      expect(await within(glib).findByText("Couldn't update")).toBeInTheDocument();
      expect(within(glib).getByRole("button", { name: "View log: glib" })).toBeInTheDocument();
      expect(within(glib).queryByRole("button", { name: "Retry" })).toBeNull();
      expect(within(glib).queryByRole("button", { name: "Update" })).toBeNull();
    });

    it("goes by the newest update of the package, and by no other kind of operation", async () => {
      operations = [
        operation(onyxKey, { id: 14, kind: "Uninstall", status: "Running" }),
        operation(glibKey, { id: 13, status: "Running" }),
        operation(glibKey, { id: 12, status: "Done", outcome: "Cancelled" }),
      ];
      started(12, "2.90.0");
      renderPage();

      expect(await within(await findRow("glib")).findByText("Updating…")).toBeInTheDocument();
      expect(within(rowOf("glib")).queryByText("Cancelled")).toBeNull();
      expect(within(rowOf("onyx")).getByRole("button", { name: "Update" })).toBeInTheDocument();
    });

    it("remembers which version an update it started was for", async () => {
      const { findAllByRole, findByRole } = renderPage();

      fireEvent.click((await findAllByRole("button", { name: "Update" }))[0]);
      const dialog = await findByRole("dialog");
      showCommands(dialog);
      await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula glib");
      fireEvent.click(within(dialog).getByRole("button", { name: "Update" }));

      await waitFor(() => expect(useUiStore.getState().updateTargets).toEqual({ 7: "2.90.0" }));
    });
  });

  describe("Just updated", () => {
    // As `useUpdateConfirm` records it when it submits one.
    function started(opId: number, target: string) {
      useUiStore.setState({ updateTargets: { ...useUiStore.getState().updateTargets, [opId]: target } });
    }

    function installed(key: ArtifactKey, version: string): Snapshot["artifacts"][number] {
      return {
        key,
        display_name: key.name === "onyx" ? "OnyX" : key.name,
        version,
        reason: "Requested",
        description: null,
        homepage: null,
        size_bytes: null,
        installed_at: null,
        path: null,
        auto_updates: false,
        uninstall_blocked: null,
      };
    }

    function justUpdated(): HTMLElement | null {
      return screen.queryByRole("region", { name: "Just updated" });
    }

    afterEach(() => {
      vi.useRealTimers();
    });

    it("takes over a finished update's tick once its row is gone, with the version it has now and when it finished", async () => {
      // Only the clock is fake: 14:40, and glib's update finished at 14:32.
      vi.useFakeTimers({ toFake: ["Date"] });
      vi.setSystemTime(new Date(2026, 8, 28, 14, 40));
      const finishedAt = new Date(2026, 8, 28, 14, 32).getTime();
      operations = [operation(glibKey, { status: "Done", outcome: "Succeeded" })];
      started(7, "2.90.0");
      useUiStore.setState({ opFinishedAt: { 7: finishedAt } });
      artifacts = [installed(glibKey, "2.88.3"), installed(onyxKey, "5.0.2")];
      const { queryClient } = renderPage();

      // Until the check after it lands, the tick is the row's, and only the row's.
      expect(await within(await findRow("glib")).findByText("Updated")).toBeInTheDocument();
      expect(justUpdated()).toBeNull();

      // The check after it: glib is at 2.90.0, with nothing left to update.
      act(() => {
        queryClient.setQueryData(queryKeys.snapshot, {
          ...snapshot,
          generation: snapshot.generation + 1,
          instances,
          artifacts: [installed(glibKey, "2.90.0"), installed(onyxKey, "5.0.2")],
          updates: [snapshot.updates[1]],
        });
      });

      const section = await screen.findByRole("region", { name: "Just updated" });
      await waitFor(() => expect(rowNames()).toEqual(["OnyX"]));
      const [line, ...more] = within(section).getAllByRole("listitem");
      expect(more).toEqual([]);
      expect(within(line).getByText("glib")).toBeInTheDocument();
      expect(within(line).getByText("2.90.0")).toBeInTheDocument();
      expect(within(line).getByText("Updated")).toBeInTheDocument();
      expect(line.querySelector("svg")).not.toBeNull();
      const time = within(line).getByText(new Intl.DateTimeFormat("en", { timeStyle: "short" }).format(finishedAt));
      expect(time.tagName).toBe("TIME");
      expect(time).toHaveAttribute("dateTime", new Date(finishedAt).toISOString());
      // Above the list, and no row of it.
      expect(section.compareDocumentPosition(rowOf("OnyX")) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
      expect(section.closest("[data-tool-row]")).toBeNull();
    });

    it("lists the newest first, one line a tool, and says the date of one that finished on another day", async () => {
      vi.useFakeTimers({ toFake: ["Date"] });
      vi.setSystemTime(new Date(2026, 8, 28, 9, 0));
      const yesterday = new Date(2026, 8, 27, 18, 5).getTime();
      const thisMorning = new Date(2026, 8, 28, 8, 55).getTime();
      operations = [
        operation(onyxKey, { id: 9, status: "Done", outcome: "Succeeded" }),
        operation(glibKey, { id: 8, status: "Done", outcome: "Succeeded" }),
        // glib's earlier update: the newer one stands for it.
        operation(glibKey, { id: 3, status: "Done", outcome: "Succeeded" }),
      ];
      started(9, "5.1.0");
      started(8, "2.90.0");
      started(3, "2.89.0");
      useUiStore.setState({ opFinishedAt: { 9: yesterday, 8: thisMorning, 3: yesterday - 60_000 } });
      updates = [];
      artifacts = [installed(glibKey, "2.90.0"), installed(onyxKey, "5.1.0")];
      renderPage();

      const section = await screen.findByRole("region", { name: "Just updated" });
      const lines = within(section).getAllByRole("listitem");
      expect(lines.map((line) => line.querySelector("span[title]")?.textContent)).toEqual(["glib", "OnyX"]);
      expect(within(lines[0]).getByText(new Intl.DateTimeFormat("en", { timeStyle: "short" }).format(thisMorning))).toBeInTheDocument();
      expect(
        within(lines[1]).getByText(new Intl.DateTimeFormat("en", { month: "numeric", day: "numeric" }).format(yesterday)),
      ).toBeInTheDocument();
      // Nothing is left to update: the section stands over the sentence that says so.
      expect(await screen.findByText("Everything is up to date")).toBeInTheDocument();
    });

    it("lists no update that failed, was cancelled or asks to be checked: those keep their rows", async () => {
      operations = [
        operation(glibKey, { id: 9, status: "Done", outcome: { Failed: { exit_code: 1, summary: "Error: no bottle" } } }),
        operation(onyxKey, { id: 10, status: "Done", outcome: { NeedsAttention: "UnchangedAfterUpgrade" } }),
        // Two whose rows are gone: still nothing to list.
        operation({ ...glibKey, name: "wget" }, { id: 11, status: "Done", outcome: "Cancelled" }),
        operation({ ...glibKey, name: "jq" }, { id: 12, status: "Done", outcome: "Unconfirmed" }),
      ];
      started(9, "2.90.0");
      started(10, "5.1.0");
      started(11, "1.1.0");
      started(12, "1.1.0");
      renderPage();

      expect(await within(await findRow("glib")).findByText("Couldn't update")).toBeInTheDocument();
      expect(within(rowOf("onyx")).getByText("Unexpected result")).toBeInTheDocument();
      expect(justUpdated()).toBeNull();
    });

    it("lists no tool uninstalled since its update, and no update still under way", async () => {
      operations = [
        operation(glibKey, { id: 9, kind: "Uninstall", status: "Done", outcome: "Succeeded" }),
        operation(glibKey, { id: 8, status: "Done", outcome: "Succeeded" }),
        operation(onyxKey, { id: 10, status: "Running" }),
      ];
      started(8, "2.90.0");
      started(10, "5.1.0");
      updates = [snapshot.updates[1]];
      renderPage();

      expect(await within(await findRow("onyx")).findByText("Updating…")).toBeInTheDocument();
      expect(justUpdated()).toBeNull();
    });

    it("stays out of the count, the header's box and Update all", async () => {
      operations = [operation(glibKey, { status: "Done", outcome: "Succeeded" })];
      started(7, "2.90.0");
      updates = [snapshot.updates[1]];
      artifacts = [installed(glibKey, "2.90.0"), installed(onyxKey, "5.0.2")];
      const { findByText, getByRole, findByRole } = renderPage();

      const section = await screen.findByRole("region", { name: "Just updated" });
      expect(await findByText("1 Update Available")).toBeInTheDocument();
      expect(within(section).queryByRole("checkbox")).toBeNull();
      expect(within(section).getAllByRole("button").map((button) => button.getAttribute("aria-label"))).toEqual([
        "Clear the Just updated list",
      ]);

      fireEvent.click(getByRole("checkbox", { name: SELECT_ALL }));
      expect(useUiStore.getState().selectedUpdates).toEqual([artifactKeyId(onyxKey)]);
      expect(getByRole("button", { name: "Update Selected (1)" })).toBeEnabled();
      fireEvent.click(getByRole("checkbox", { name: SELECT_ALL }));
      expect(useUiStore.getState().selectedUpdates).toEqual([]);

      fireEvent.click(getByRole("button", { name: "Update All" }));
      await findByRole("dialog", { name: "Update “OnyX”?" });
      expect(plannedNames()).toEqual(["onyx"]);
    });

    it("hides itself on Clear, until the next update succeeds", async () => {
      operations = [operation(glibKey, { status: "Done", outcome: "Succeeded" })];
      started(7, "2.90.0");
      updates = [snapshot.updates[1]];
      artifacts = [installed(glibKey, "2.90.0"), installed(onyxKey, "5.0.2")];
      const { queryClient } = renderPage();

      const section = await screen.findByRole("region", { name: "Just updated" });
      fireEvent.click(within(section).getByRole("button", { name: "Clear the Just updated list" }));
      await waitFor(() => expect(justUpdated()).toBeNull());
      expect(useUiStore.getState().clearedJustUpdated).toEqual([7]);
      expect(rowNames()).toEqual(["OnyX"]);

      // onyx's update succeeds and its row goes: the section is back, with onyx alone.
      operations = [operation(onyxKey, { id: 8, status: "Done", outcome: "Succeeded" }), ...operations];
      started(8, "5.1.0");
      updates = [];
      artifacts = [installed(glibKey, "2.90.0"), installed(onyxKey, "5.1.0")];
      await act(async () => {
        await queryClient.invalidateQueries({ queryKey: queryKeys.operations });
        await queryClient.invalidateQueries({ queryKey: queryKeys.snapshot });
      });

      const again = await screen.findByRole("region", { name: "Just updated" });
      const lines = within(again).getAllByRole("listitem");
      expect(lines).toHaveLength(1);
      expect(within(lines[0]).getByText("OnyX")).toBeInTheDocument();
      expect(within(lines[0]).getByText("5.1.0")).toBeInTheDocument();
    });

    it("never shows a model's digest as its new version", async () => {
      instances = [...snapshot.instances, { ...stoppedOllama, status: { unavailable: null, notes: [] } }];
      operations = [operation(qwenKey, { status: "Done", outcome: "Succeeded" })];
      started(7, "sha256:9f1c0b6d2e4a7c5b3d1f8a6e4c2b0d9f7e5c3a1b8d6f4e2c0a9b7d5f3e1c8a6b");
      artifacts = [installed(qwenKey, "5642e97495e1")];
      const { container } = renderPage();

      const section = await screen.findByRole("region", { name: "Just updated" });
      expect(within(section).getByText("qwen3:8b")).toBeInTheDocument();
      expect(container.textContent).not.toMatch(/sha256|5642e974/);
    });

    it("calls itself 刚更新的 in Chinese, with 清除 and 已更新", () => {
      expect(zhCN.updates.justUpdated.title).toBe("刚更新的");
      expect(zhCN.updates.justUpdated.clear).toBe("清除");
      expect(zhCN.updates.progress.succeeded).toBe("已更新");
    });
  });

  it("says every update is hidden — not that everything is up to date — once each is skipped or never reminded about", async () => {
    settings.ignored_updates = [glibKey];
    settings.skipped_versions = [{ key: onyxKey, version: "5.1.0" }];
    const { findByText, queryByText, getByRole } = renderPage();

    await findByText("No updates to handle");
    expect(queryByText("Everything is up to date")).not.toBeInTheDocument();
    // Where they are, one press away: Settings' hidden updates.
    expect(getByRole("button", { name: "Show Hidden Updates" }).className).toBe(`mt-4 ${BUTTON.regular.grey}`);
    fireEvent.click(getByRole("button", { name: "Show Hidden Updates" }));
    expect(useUiStore.getState().page).toBe("settings");
  });

  it("says an empty list as macOS does: a 36 tertiary symbol, the title, one sentence, one grey button", async () => {
    updates = [];
    const { findByText, getByRole } = renderPage();

    const title = await findByText("Everything is up to date");
    const empty = title.closest("[data-empty-state]") as HTMLElement;
    const symbol = empty.querySelector("svg") as SVGElement;
    expect(symbol).toHaveAttribute("width", "36");
    // Never green: nothing to do is not news to celebrate.
    expect(symbol.getAttribute("class")).toContain("text-tertiary");
    expect(symbol.getAttribute("class")).not.toContain("text-success");
    expect(title).toHaveClass("text-section", "mt-6");
    const sentence = title.nextElementSibling as HTMLElement;
    expect(sentence.textContent).toMatch(/^Checked .*\.$/);
    expect(sentence).toHaveClass("text-section", "font-normal", "text-muted", "mt-2", "max-w-90");
    const again = within(empty).getByRole("button", { name: "Check Again" });
    expect(again.className).toBe(`mt-4 ${BUTTON.regular.grey}`);
    fireEvent.click(again);
    await waitFor(() => expect(calls("refresh").length).toBeGreaterThan(0));
    expect(getByRole("button", { name: "Check Again" })).toBe(again);
  });

  it("says everything is up to date only when the backend reports no updates at all", async () => {
    updates = [];
    const { findByText } = renderPage();

    await findByText("Everything is up to date");
  });

  it("does not say everything is up to date when a source never answered", async () => {
    // The lie this page used to tell. No candidates is exactly what an
    // unreachable source produces, and the page read that silence as good
    // news: a Mac with Ollama stopped was told, in so many words, that
    // everything was up to date -- about a source Canager had not managed
    // to ask.
    updates = [];
    instances = [...snapshot.instances, stoppedOllama];
    const { findByText, queryByText, getByRole } = renderPage();

    await findByText("Ollama isn't running");
    expect(await findByText("No updates in the sources checked")).toBeInTheDocument();
    expect(queryByText("Everything is up to date")).not.toBeInTheDocument();
    // And the notice is the working one, not a copy of its words: the
    // button that starts the daemon comes with it.
    fireEvent.click(getByRole("button", { name: "Open Ollama" }));
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("open_ollama_app"));
  });

  it("warns that Homebrew's catalogue may be behind, with the header's Check again", async () => {
    // A note, not an unavailability: brew answered, and what it said may
    // simply be out of date. "Everything is up to date" is the one
    // sentence that must not appear over it.
    updates = [];
    instances = [
      { ...snapshot.instances[0], status: { unavailable: null, notes: ["IndexMayBeStale"] } },
      ...snapshot.instances.slice(1),
    ];
    const { findByText, queryByText, getByRole } = renderPage();

    await findByText("Couldn't update Homebrew's software list");
    expect(queryByText("Everything is up to date")).not.toBeInTheDocument();
    fireEvent.click(getByRole("button", { name: "Check Again" }));
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("refresh"));
  });

  it("does not say everything is up to date while Homebrew is still downloading its list of software", async () => {
    // Nothing has failed, so the notice is information rather than a
    // warning -- but this refresh did not check Homebrew for updates: its
    // candidates are the previous refresh's (`InstanceNote::IndexUpdating`
    // in crates/canager-core/src/model.rs), so none from it is not news
    // that there are none.
    updates = [];
    instances = [
      { ...snapshot.instances[0], status: { unavailable: null, notes: ["IndexUpdating"] } },
      ...snapshot.instances.slice(1),
    ];
    const { findByText, queryByText } = renderPage();

    await findByText("Homebrew is updating its software list");
    expect(await findByText("No updates in the sources checked")).toBeInTheDocument();
    expect(queryByText("Everything is up to date")).not.toBeInTheDocument();
  });

  it("does not say everything is up to date when a check failed this round", async () => {
    // Every source still reads as answering, with no note: `refresh` keeps
    // a source whose inventory or update check failed as it was, carries
    // its last rows and candidates forward, and says so only in
    // `errors`. None listed is then no news that there are none.
    updates = [];
    errors = [{ instance_id: "brew:/opt/homebrew", message: "brew outdated exited with code 1" }];
    const { findByText, queryByText } = renderPage();

    expect(await findByText("No updates in the sources checked")).toBeInTheDocument();
    expect(queryByText("Everything is up to date")).not.toBeInTheDocument();
  });

  describe("source notices", () => {
    it("puts a silent source's notice in the list's first row, 32 high, with its button in the line", async () => {
      instances = [...snapshot.instances, stoppedOllama];
      const { findByText, getByRole, queryByText } = renderPage();

      const notice = await findByText("Ollama isn't running");
      // The list's first row, which scrolls away with it (spec §3.8), over
      // the rows, and under the list's header.
      expect(slotOf(notice)).toBe(0);
      expect(slotOf(await findRow("glib"))).toBe(1);
      const header = getByRole("checkbox", { name: SELECT_ALL });
      expect(header.compareDocumentPosition(notice) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
      // A line of the list: 32 high, 20 in from the edge as the rows are,
      // on their grid -- the ⚠︎ in the avatars' column, the title where
      // the names start -- over a hairline as a row's.
      const line = notice.closest("[data-notice-line]") as HTMLElement;
      expect(line.className.split(" ")).toContain("h-8");
      expect((line.closest("[data-list-slot] > div") as HTMLElement).className.split(" ")).toContain("px-5");
      expect((line.querySelector("[data-notice-symbol]") as HTMLElement).className.split(" ")).toEqual(
        expect.arrayContaining(["ml-7", "w-8"]),
      );
      expect(notice.className.split(" ")).toContain("ml-3");
      const hairline = line.closest("[data-list-slot]")?.querySelector("[data-row-separator]") as HTMLElement;
      expect(hairline.className.split(" ")).toEqual(expect.arrayContaining(["left-18", "right-0", "h-px"]));
      // Its ⓘ and its one button, and no link.
      expect(line.innerHTML).not.toContain("text-accent-text");
      // Not one of the rows ↑ and ↓ move between.
      expect(line.closest("[data-list-slot]")?.querySelector("[data-row-focus]")).toBeNull();
      expect(getByRole("button", { name: "Open Ollama" })).toBeInTheDocument();
      // Its explanation is behind Details, not spread over the page.
      expect(queryByText("Open Ollama to see what it has and check for updates.")).toBeNull();
      const details = getByRole("button", { name: "Details: Ollama isn't running" });
      fireEvent.click(details);
      expect(
        document.getElementById(details.getAttribute("aria-controls") ?? ""),
      ).toHaveTextContent("Open Ollama to see what it has and check for updates.");
    });

    it("folds two lines into one, the warning first, and keeps them unfolded while the page changes under them, until their number does", async () => {
      const brewUpdating = {
        ...snapshot.instances[0],
        status: { unavailable: null, notes: ["IndexUpdating" as const] },
      };
      instances = [brewUpdating, ...snapshot.instances.slice(1), stoppedOllama];
      const { queryClient } = renderPage();

      // Ollama's line, with its button, though Homebrew's comes first.
      await screen.findByText("Ollama isn't running");
      expect(screen.getByRole("button", { name: "Open Ollama" })).toBeInTheDocument();
      expect(screen.queryByText("Homebrew is updating its software list")).toBeNull();
      fireEvent.click(screen.getByRole("button", { name: "1 more issue" }));
      expect(screen.getByText("Homebrew is updating its software list")).toBeInTheDocument();

      // The updates go, and the page says so under the same two lines.
      updates = [];
      await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.snapshot }));
      await screen.findByText("No updates in the sources checked");
      expect(screen.getByText("Homebrew is updating its software list")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Show Fewer" })).toBeInTheDocument();

      // One line, then two again: folded.
      instances = [...snapshot.instances, stoppedOllama];
      await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.snapshot }));
      await waitFor(() => expect(screen.queryByText("Homebrew is updating its software list")).toBeNull());
      expect(screen.queryByRole("button", { name: "Show Fewer" })).toBeNull();
      instances = [brewUpdating, ...snapshot.instances.slice(1), stoppedOllama];
      await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.snapshot }));
      expect(await screen.findByRole("button", { name: "1 more issue" })).toHaveAttribute("aria-expanded", "false");
      expect(screen.queryByText("Homebrew is updating its software list")).toBeNull();
    });

    it("names the silent source in its notice, and every row names its own, so the notice is never read as another source's", async () => {
      // The critical case, now that the list is not grouped by source.
      // Homebrew answered with three updates; Ollama did not answer and its
      // two candidates were carried forward from last time. The notice says
      // which source it is about, each Ollama row says Ollama and that its
      // source is unavailable, and no Homebrew row says either.
      const llamaKey: ArtifactKey = { ...qwenKey, name: "llama3.2:3b" };
      instances = [
        ...snapshot.instances,
        { ...stoppedOllama, status: { unavailable: "NotResponding", notes: [] } },
      ];
      updates = [
        ...snapshot.updates,
        brewCandidate("jq"),
        ...[qwenKey, llamaKey].map((key) => ({
          key,
          current: "5642e97495e1",
          target: "a1b2c3d4e5f6",
          channel: "Digest" as const,
          checkable: true,
          warnings: [],
          blocked: null,
        })),
      ];
      const { findByText, getByRole } = renderPage();

      await findByText("Ollama isn't responding");
      const details = getByRole("button", { name: "Details: Ollama isn't responding" });
      fireEvent.click(details);
      // Its rows, by name: in a list that mixes sources, "what's listed
      // here" alone would take in Homebrew's fresh rows too.
      expect(
        document.getElementById(details.getAttribute("aria-controls") ?? ""),
      ).toHaveTextContent(
        "What's listed for Ollama is from the last time it responded, and later changes aren't shown. Check again later.",
      );
      for (const name of ["glib", "onyx", "jq"]) {
        const row = rowOf(name);
        expect(within(row).getByText("Homebrew")).toBeInTheDocument();
        expect(within(row).queryByText("Ollama")).toBeNull();
        expect(within(row).getByRole("button", { name: "Update" })).toBeInTheDocument();
      }
      await showCantUpdate();
      for (const name of ["qwen3:8b", "llama3.2:3b"]) {
        const row = await findRow(name);
        expect(within(row).getByText("Ollama")).toBeInTheDocument();
        expect(chipDetail(row, "Can't update now")).toHaveTextContent(
          "Ollama isn't responding. Click Check Again later.",
        );
      }
    });

    it("marks every row of a read-only source View only, and no row of another source", async () => {
      // "Canager can only show what's installed with pip" used to sit
      // above nine rows, three of which were Homebrew's and perfectly
      // updatable.
      const pipPackages = ["urllib3", "requests", "certifi", "idna", "charset-normalizer", "six"];
      updates = [
        ...snapshot.updates,
        brewCandidate("jq"),
        ...pipPackages.map((name) => ({
          key: { instance_id: "pip:/usr/bin/python3", kind: "Package" as const, name },
          current: "1.0.0",
          target: "1.1.0",
          channel: "Registry" as const,
          checkable: true,
          warnings: [],
          blocked: null,
        })),
      ];
      const { queryAllByText } = renderPage();

      await showCantUpdate();
      await findRow("urllib3");
      for (const name of ["glib", "onyx", "jq"]) {
        expect(within(rowOf(name)).queryByRole("button", { name: "View only" })).toBeNull();
      }
      for (const name of pipPackages) {
        expect(within(rowOf(name)).getByRole("button", { name: "View only" })).toBeInTheDocument();
        expect(within(rowOf(name)).getByText("pip")).toBeInTheDocument();
      }
      // Not a notice line of its own: the rows say it, on their chips and
      // nowhere else.
      expect(queryAllByText("View only").filter((text) => text.closest("button") === null)).toEqual([]);
    });

    it("tells the truth about carried-forward rows on this page, both ways round", async () => {
      // What a silent source's notice says about its rows has to match
      // them. Homebrew is silent and its two candidates were carried
      // forward, so they are last time's answer and the user needs telling.
      instances = [
        { ...snapshot.instances[0], status: { unavailable: "NotResponding", notes: [] } },
        ...snapshot.instances.slice(1),
      ];
      const withRows = renderPage();

      const details = await withRows.findByRole("button", {
        name: "Details: Homebrew isn't responding",
      });
      fireEvent.click(details);
      expect(
        document.getElementById(details.getAttribute("aria-controls") ?? ""),
      ).toHaveTextContent(
        "What's listed for Homebrew is from the last time it responded, and later changes aren't shown. Check again later.",
      );
      // The next step it names, as its line's own button, which checks again.
      const line = details.closest("[data-notice-line]") as HTMLElement;
      const again = within(line).getByRole("button", { name: "Check Again" });
      expect(again.className).toContain(BUTTON.small.grey);
      mockInvoke.mockClear();
      fireEvent.click(again);
      await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("refresh"));
      withRows.unmount();

      // And the cold start, which is every launch: the snapshot is in
      // memory only, so the first refresh has nothing to carry forward and
      // the same notice promised rows that were not there.
      updates = [
        {
          key: urllib3Key,
          current: "2.2.1",
          target: "2.3.0",
          channel: "Registry",
          checkable: true,
          warnings: [],
          blocked: null,
        },
      ];
      const coldStart = renderPage();

      const coldDetails = await coldStart.findByRole("button", {
        name: "Details: Homebrew isn't responding",
      });
      fireEvent.click(coldDetails);
      const text = document.getElementById(coldDetails.getAttribute("aria-controls") ?? "");
      expect(text).toHaveTextContent(
        "Homebrew didn't respond, so what it has installed can't be shown.",
      );
      expect(text?.textContent).not.toMatch(/What's listed/);
    });
  });

  it("offers no Update button for a row carried forward from a source that isn't answering", async () => {
    // `refresh` keeps an unavailable source's last known candidates rather
    // than dropping them, so this row is on screen -- and `ollama pull`
    // against a daemon that is not listening cannot succeed. Offering the
    // button and then refusing the click is the pattern this phase exists
    // to remove; the count says what is really on offer instead.
    instances = [...snapshot.instances, stoppedOllama];
    updates = [
      ...snapshot.updates,
      {
        key: qwenKey,
        current: "5642e97495e1",
        target: "a1b2c3d4e5f6",
        channel: "Digest",
        checkable: true,
        warnings: [],
        blocked: null,
      },
    ];
    const { findByText, findAllByRole } = renderPage();

    expect(await findByText("2 Updates Available")).toBeInTheDocument();
    expect(await findByText("1 more can't be updated here")).toBeInTheDocument();
    await showCantUpdate();
    const row = await findRow("qwen3:8b");
    expect(within(row).queryByRole("button", { name: "Update" })).toBeNull();
    expect(within(row).queryByRole("checkbox")).toBeNull();
    // The two brew rows still have their buttons: one silent source does
    // not disarm the page.
    expect(await findAllByRole("button", { name: "Update" })).toHaveLength(2);
  });

  it("draws only the rows on screen when a failed lookup turns every package into a row", async () => {
    // Offline, a source cannot establish any remote version, so it reports
    // one `checkable: false` candidate per installed package instead of
    // none -- the page that already has to explain "we could not check"
    // is also the page asked to draw several hundred rows.
    updates = Array.from({ length: 400 }, (_, index) => ({
      key: {
        instance_id: "pip:/usr/bin/python3",
        kind: "Package" as const,
        name: `pkg-${String(index).padStart(3, "0")}`,
      },
      current: "1.0.0",
      target: "1.0.0",
      channel: "Registry" as const,
      checkable: false,
      warnings: [{ Message: "Could not reach pypi.org" }],
      blocked: null,
    }));

    const { findByText, container } = renderPage();

    // The list still knows how long it is.
    expect(await findByText("400 more can't be updated here")).toBeInTheDocument();
    await showCantUpdate();
    await findRow("pkg-000");
    const drawn = container.querySelectorAll("[data-index]").length;
    expect(drawn).toBeGreaterThan(0);
    // 600px of viewport over 56px rows is about eleven rows plus the
    // virtualizer's overscan; anything near 400 means the whole list is in
    // the DOM.
    expect(drawn).toBeLessThan(40);
    expect(container.textContent).not.toContain("pkg-399");
  });

  it("says which version you are moving to, with technical details off", async () => {
    // Spec §6: the one screen whose job is "look before you act". A
    // confirmation that hides what changes is not a confirmation.
    expect(settings.show_technical_details).toBe(false);

    const { findAllByRole, findByRole } = renderPage();

    fireEvent.click((await findAllByRole("button", { name: "Update" }))[0]);
    const dialog = await findByRole("dialog");

    // Under the question, with where it comes from.
    const jump = await within(dialog).findByText("2.88.3 → 2.90.0");
    expect(jump.closest("[data-dialog-subtitle]")).toHaveTextContent("Homebrew · 2.88.3 → 2.90.0");
  });

  describe("the confirmation sheet", () => {
    it("asks about one tool by its name, and about several by how many, each with its avatar and new version", async () => {
      const { findAllByRole, findByRole, getByRole, queryByRole } = renderPage();

      fireEvent.click((await findAllByRole("button", { name: "Update" }))[0]);
      let dialog = await findByRole("dialog", { name: "Update “glib”?" });
      // One tool: an alert, 360 wide, its 48 icon over the question.
      expect(dialog).toHaveAttribute("data-dialog-width", "360");
      expect(dialog.querySelector("[data-dialog-icon]")).not.toBeNull();
      expect(dialog.querySelector("[data-sheet-tools]")).toBeNull();
      fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
      await waitFor(() => expect(queryByRole("dialog")).toBeNull());

      const checkboxes = await findAllByRole("checkbox", { name: ROW_CHECKBOX });
      fireEvent.click(checkboxes[0]);
      fireEvent.click(checkboxes[1]);
      fireEvent.click(getByRole("button", { name: /^Update Selected/ }));
      dialog = await findByRole("dialog", { name: "Update 2 tools?" });
      // Several: 480 wide, no icon, the tools in a grouped list.
      expect(dialog).toHaveAttribute("data-dialog-width", "480");
      expect(dialog.querySelector("[data-dialog-icon]")).toBeNull();
      const tools = [...dialog.querySelectorAll("[data-sheet-tools] > [data-sheet-tool]")] as HTMLElement[];
      expect(tools.map((tool) => tool.querySelector("[data-sheet-name]")?.textContent)).toEqual(["glib", "onyx"]);
      // The row's own avatar, at 24; its source for a screen reader; the
      // version it moves to, on the right.
      const initial = within(tools[0]).getByText("H");
      expect(initial).toHaveAttribute("aria-hidden", "true");
      expect(initial.className).toMatch(/\bh-6 w-6\b/);
      expect(within(tools[0]).getByText("Homebrew")).toHaveClass("sr-only");
      expect(within(tools[0]).getByText("2.88.3 → 2.90.0")).toBeInTheDocument();
      expect(within(tools[1]).getByText("5.0.2 → 5.1.0")).toBeInTheDocument();
    });

    it("shows a tool's logo, with its source's on the corner, on its row and on its line in the sheet", async () => {
      // A pack of this test's own: glib's logo and Homebrew's.
      const GLIB = "M1 1h22v22H1z";
      const HOMEBREW = "M3 3h18v18H3z";
      const toolIcons = loadToolIcons(
        {
          version: 1,
          generated: "2026-09-28",
          glyphs: {
            "si-glib": { path: GLIB, hex: "4A86CF", title: "GLib" },
            "si-homebrew": { path: HOMEBREW, hex: "FBB040", title: "Homebrew" },
          },
          rasters: {},
          tools: { "brew:glib": "si-glib" },
          sources: { brew: "si-homebrew" },
        },
        new Map(),
      );
      const { findByRole } = renderPage({ toolIcons });
      // The tool's logo, not on the corner, and its source's, on it.
      const expectLogos = (avatarHolder: Element | null) => {
        const logo = avatarHolder?.querySelector(`path[d="${GLIB}"]`);
        expect(logo).toBeInstanceOf(Element);
        expect(logo?.closest("[data-source-badge]")).toBeNull();
        expect(avatarHolder?.querySelector(`[data-source-badge] path[d="${HOMEBREW}"]`)).toBeInstanceOf(Element);
      };

      const row = await findRow("glib");
      expectLogos(row);

      fireEvent.click(within(row).getByRole("button", { name: "Update" }));
      expectLogos((await findByRole("dialog", { name: "Update “glib”?" })).querySelector("[data-dialog-icon]"));
    });

    it("keeps the commands one press away while Show technical details is off", async () => {
      const { findAllByRole, getByRole, findByRole } = renderPage();

      const checkboxes = await findAllByRole("checkbox", { name: ROW_CHECKBOX });
      fireEvent.click(checkboxes[0]);
      fireEvent.click(checkboxes[1]);
      fireEvent.click(getByRole("button", { name: /^Update Selected/ }));
      const dialog = await findByRole("dialog");

      const disclosure = within(dialog).getByRole("button", { name: "Show Commands" });
      expect(disclosure).toHaveAttribute("aria-expanded", "false");
      expect(within(dialog).queryByText(/brew upgrade/)).toBeNull();
      fireEvent.click(disclosure);
      // Each under its tool's name.
      expect(
        within(dialog).getByText("/opt/homebrew/bin/brew upgrade --cask onyx").previousElementSibling,
      ).toHaveTextContent("onyx");
    });

    it("shows the commands from the start with Show technical details on", async () => {
      settings.show_technical_details = true;
      const { findAllByRole, findByRole } = renderPage();

      fireEvent.click((await findAllByRole("button", { name: "Update" }))[0]);
      const dialog = await findByRole("dialog");

      expect(await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula glib")).toBeInTheDocument();
      expect(within(dialog).getByRole("button", { name: "Show Command" })).toHaveAttribute(
        "aria-expanded",
        "true",
      );
    });

    it("keeps each tool's notes under its own row, in the label colour at 11, a caution marked", async () => {
      needsPassword.add("onyx");
      noCancel.add("glib");
      planWarnings.glib = ["CompilesLocally"];
      const { findAllByRole, getByRole, findByRole } = renderPage();

      const checkboxes = await findAllByRole("checkbox", { name: ROW_CHECKBOX });
      fireEvent.click(checkboxes[0]);
      fireEvent.click(checkboxes[1]);
      fireEvent.click(getByRole("button", { name: /^Update Selected/ }));
      const dialog = await findByRole("dialog");
      await waitFor(() => expect(within(dialog).getByRole("button", { name: "Update" })).toBeEnabled());

      const tools = [...dialog.querySelectorAll("[data-sheet-tool]")].map((tool) => ({
        tool: tool.querySelector("[data-sheet-name]")?.textContent,
        lines: [...tool.querySelectorAll("li")].map((item) => ({
          text: item.textContent?.trim(),
          caution: item.hasAttribute("data-caution"),
        })),
      }));
      expect(tools).toEqual([
        {
          tool: "glib",
          lines: [
            { text: "This compiles on your Mac and takes a while.", caution: false },
            {
              text: "This can't be cancelled once it starts. Don't quit Canager or shut down your Mac until it finishes.",
              caution: true,
            },
          ],
        },
        { tool: "onyx", lines: [{ text: "Some apps ask for your Mac password at this step.", caution: false }] },
      ]);
      // Text to read, not a caption: the label colour, at 11.
      for (const line of dialog.querySelectorAll("[data-sheet-tool] li")) {
        expect(line.className).toMatch(/\btext-foreground\b/);
        expect(line.className).toMatch(/\btext-small\b/);
      }
      // A caution's ⚠︎, before its words.
      const caution = dialog.querySelector("[data-sheet-tool] li[data-caution]") as HTMLElement;
      expect(caution.firstElementChild?.tagName.toLowerCase()).toBe("svg");
      expect(caution.firstElementChild).toHaveClass("text-warning");
      // No 「请注意」 block, and no count of the notes beside Update.
      expect(within(dialog).queryByRole("region", { name: "Notes" })).toBeNull();
      expect(within(dialog).queryByRole("button", { name: /notes?$/ })).toBeNull();
    });

    it("lists the tools with notes first, so that the first note is in sight without scrolling", async () => {
      // onyx comes after glib on the page; its note puts it first.
      needsPassword.add("onyx");
      const { findByRole, findByText, getByRole } = renderPage();

      await findByText("2 Updates Available");
      fireEvent.click(getByRole("button", { name: "Update All" }));
      const dialog = await findByRole("dialog", { name: "Update 2 tools?" });
      const names = () =>
        [...dialog.querySelectorAll("[data-sheet-tool]")].map((tool) => tool.querySelector("[data-sheet-name]")?.textContent);

      // (Before the plans are back, the list's own order: see "while its
      // plans are on their way" below.)
      await waitFor(() => expect(within(dialog).getByRole("button", { name: "Update" })).toBeEnabled());
      expect(names()).toEqual(["onyx", "glib"]);
      const first = dialog.querySelector("[data-sheet-tool]") as HTMLElement;
      expect(within(first).getByText("Some apps ask for your Mac password at this step.")).toBeInTheDocument();
    });

    it("says nothing about notes beside Update when there are none", async () => {
      const { findByRole, findByText, getByRole } = renderPage();

      await findByText("2 Updates Available");
      fireEvent.click(getByRole("button", { name: "Update All" }));
      const dialog = await findByRole("dialog", { name: "Update 2 tools?" });

      await waitFor(() => expect(within(dialog).getByRole("button", { name: "Update" })).toBeEnabled());
      expect(within(dialog).queryByRole("region", { name: "Notes" })).toBeNull();
      expect(within(dialog).queryByRole("button", { name: /to note$/ })).toBeNull();
    });

    it("names the source after a name the list has from two sources, and only there (R3)", async () => {
      const pipx: Snapshot["instances"][number] = {
        id: "pipx",
        adapter_id: "pipx",
        exe_path: "/opt/homebrew/bin/pipx",
        prefix: "/opt/homebrew/bin",
        scope: "User",
        version: "1.17.3",
        status: { unavailable: null, notes: [] },
        unverified_version: null,
        read_only_reason: null,
      };
      instances = [snapshot.instances[0], pipx];
      updates = [
        brewCandidate("httpie"),
        { ...brewCandidate("httpie"), key: { instance_id: "pipx", kind: "Package", name: "httpie" } },
        brewCandidate("jq"),
      ];
      const { findByRole, findByText, getByRole } = renderPage();

      await findByText("3 Updates Available");
      fireEvent.click(getByRole("button", { name: "Update All" }));
      const dialog = await findByRole("dialog", { name: "Update 3 tools?" });
      const rows = [...dialog.querySelectorAll("[data-sheet-tool]")] as HTMLElement[];
      const shown = (row: HTMLElement) =>
        [...row.querySelectorAll("[data-sheet-name] ~ span")].map((span) => [span.textContent, span.className]);
      const byName = (name: string) => rows.filter((row) => row.querySelector("[data-sheet-name]")?.textContent === name);
      // Both httpies say which they are, in 11 muted beside the name.
      expect(byName("httpie").map(shown)).toEqual([
        [["Homebrew", "shrink-0 text-small text-muted"]],
        [["pipx", "shrink-0 text-small text-muted"]],
      ]);
      // jq says it to a screen reader only.
      expect(shown(byName("jq")[0])).toEqual([["Homebrew", "sr-only"]]);
    });

    it("lists every tool of a long batch in a grouped list that scrolls inside past 320", async () => {
      updates = Array.from({ length: 10 }, (_, i) => brewCandidate(`tool-${i}`));
      const { findByRole, findByText, getByRole } = renderPage();

      await findByText("10 Updates Available");
      fireEvent.click(getByRole("button", { name: "Update All" }));
      const dialog = await findByRole("dialog", { name: "Update 10 tools?" });

      const list = dialog.querySelector("[data-sheet-tools]") as HTMLElement;
      expect([...list.querySelectorAll("[data-sheet-name]")].map((name) => name.textContent)).toEqual(
        Array.from({ length: 10 }, (_, i) => `tool-${i}`),
      );
      // The group's fill and corners, no taller than 320, scrolling inside
      // -- and reachable by the keyboard to scroll it.
      for (const look of ["bg-group", "rounded-group", "max-h-80", "overflow-y-auto"]) {
        expect(list).toHaveClass(look);
      }
      expect(list).toHaveAttribute("tabindex", "0");
      // Nothing folded behind a press.
      expect(within(dialog).queryByRole("button", { name: /more$/ })).toBeNull();
    });

    it("lists every tool of a long batch when one of them was refused, with its why", async () => {
      updates = Array.from({ length: 10 }, (_, i) => brewCandidate(`tool-${i}`));
      planFailures["tool-8"] = "tool-8 is pinned";
      const { findByRole, findByText, getByRole } = renderPage();

      await findByText("10 Updates Available");
      fireEvent.click(getByRole("button", { name: "Update All" }));
      const dialog = await findByRole("dialog", { name: "Update 9 tools?" });

      expect(dialog.querySelectorAll("[data-sheet-tool]")).toHaveLength(10);
      expect(within(dialog).getByRole("alert")).toHaveTextContent("Couldn't prepare the update: tool-8 is pinned");
      expect(within(dialog).queryByRole("button", { name: /more$/ })).toBeNull();
    });

    it("gives a row's Update the regular grey look, and the accent to Update all alone", async () => {
      // One accent button on the page: what it asks for as a whole. Every
      // row offers its own the same way, grey.
      renderPage();

      const update = within(await findRow("glib")).getByRole("button", { name: "Update" });
      expect(update.className).toBe(BUTTON.regular.grey);
      expect(screen.getByRole("button", { name: "Update All" }).className).toBe(BUTTON.regular.default);
      expect(document.querySelectorAll("button.bg-accent")).toHaveLength(1);
    });

    it("puts the focus on Update as it opens, and gives it back to the row's Update when cancelled", async () => {
      const { findAllByRole, findByRole, queryByRole } = renderPage();

      const rowUpdate = within(await findRow("glib")).getByRole("button", { name: "Update" });
      fireEvent.click(rowUpdate);
      const dialog = await findByRole("dialog");
      await waitFor(() =>
        expect(document.activeElement).toBe(within(dialog).getByRole("button", { name: "Update" })),
      );

      fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
      await waitFor(() => expect(queryByRole("dialog")).toBeNull());
      await waitFor(() => expect(document.activeElement).toBe(rowUpdate));
      expect(await findAllByRole("button", { name: "Update" })).toContain(rowUpdate);
    });

    it("gives the focus back to Update all once the updates it confirmed have started", async () => {
      const { getByRole, findByRole, queryByRole, findByText } = renderPage();

      await findByText("2 Updates Available");
      const updateAll = getByRole("button", { name: "Update All" });
      fireEvent.click(updateAll);
      const dialog = await findByRole("dialog");
      fireEvent.click(within(dialog).getByRole("button", { name: "Update" }));

      await waitFor(() => expect(submittedPlanIds()).toHaveLength(2));
      await waitFor(() => expect(queryByRole("dialog")).toBeNull());
      await waitFor(() => expect(document.activeElement).toBe(updateAll));
    });

    it("puts the focus on Close when a batch did not all start, and back on Update all after it", async () => {
      submitFailures["2"] = '{"kind":"expired"}';
      const { getByRole, findByRole, queryByRole, findByText } = renderPage();

      await findByText("2 Updates Available");
      const updateAll = getByRole("button", { name: "Update All" });
      fireEvent.click(updateAll);
      const dialog = await findByRole("dialog");
      fireEvent.click(within(dialog).getByRole("button", { name: "Update" }));

      const close = await within(dialog).findByRole("button", { name: "Close" });
      await waitFor(() => expect(document.activeElement).toBe(close));
      fireEvent.click(close);
      await waitFor(() => expect(queryByRole("dialog")).toBeNull());
      await waitFor(() => expect(document.activeElement).toBe(updateAll));
    });

    it("gives the focus back to Update selected when Escape closes it", async () => {
      const { findAllByRole, getByRole, findByRole, queryByRole } = renderPage();

      fireEvent.click((await findAllByRole("checkbox", { name: ROW_CHECKBOX }))[0]);
      const updateSelected = getByRole("button", { name: /^Update Selected/ });
      fireEvent.click(updateSelected);
      await findByRole("dialog");
      fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });

      await waitFor(() => expect(queryByRole("dialog")).toBeNull());
      await waitFor(() => expect(document.activeElement).toBe(updateSelected));
    });
  });

  describe("the confirmation sheet, while its plans are on their way", () => {
    // Hands `name`'s held plan back, and lets what it sets off settle.
    async function release(name: string) {
      await waitFor(() => expect(releasePlan[name]).toBeDefined());
      await act(async () => {
        releasePlan[name]();
        await new Promise((resolve) => setTimeout(resolve, 0));
      });
    }

    it("is up the moment Update selected is pressed, preparing, and offers Update once every plan is back", async () => {
      holdPlans.add("glib");
      holdPlans.add("onyx");
      needsPassword.add("onyx");
      const { findAllByRole, getByRole, findByRole } = renderPage();

      const checkboxes = await findAllByRole("checkbox", { name: ROW_CHECKBOX });
      fireEvent.click(checkboxes[0]);
      fireEvent.click(checkboxes[1]);
      fireEvent.click(getByRole("button", { name: "Update Selected (2)" }));

      // No plan is back yet: the tools, with their avatars and versions,
      // are the rows'.
      const dialog = await findByRole("dialog", { name: "Update 2 tools?" });
      expect(plannedNames().sort()).toEqual(["glib", "onyx"]);
      const tools = [...dialog.querySelectorAll("[data-sheet-tool]")] as HTMLElement[];
      expect(tools.map((tool) => tool.querySelector("[data-sheet-name]")?.textContent)).toEqual(["glib", "onyx"]);
      expect(within(tools[0]).getByText("H")).toHaveAttribute("aria-hidden", "true");
      expect(within(tools[0]).getByText("2.88.3 → 2.90.0")).toBeInTheDocument();
      expect(within(tools[1]).getByText("5.0.2 → 5.1.0")).toBeInTheDocument();
      // Preparing, where the notes and the commands will go, and Update off.
      expect(within(dialog).getByText("Preparing…")).toBeInTheDocument();
      expect(within(dialog).queryByText("Some apps ask for your Mac password at this step.")).toBeNull();
      expect(within(dialog).queryByRole("button", { name: /^Show Command/ })).toBeNull();
      const update = within(dialog).getByRole("button", { name: "Update" });
      expect(update).toBeDisabled();
      expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeEnabled();

      // One plan back is not the batch: still preparing, still off.
      await release("glib");
      expect(within(dialog).getByText("Preparing…")).toBeInTheDocument();
      expect(update).toBeDisabled();
      expect(within(dialog).queryByText("Some apps ask for your Mac password at this step.")).toBeNull();

      // Every plan back: the notes and the commands, and Update on.
      await release("onyx");
      await waitFor(() => expect(update).toBeEnabled());
      expect(within(dialog).queryByText("Preparing…")).toBeNull();
      // onyx's note, under onyx, which it brings to the top of the list.
      const first = dialog.querySelector("[data-sheet-tool]") as HTMLElement;
      expect(first.querySelector("[data-sheet-name]")).toHaveTextContent("onyx");
      expect(first).toHaveTextContent("Some apps ask for your Mac password at this step.");
      showCommands(dialog);
      expect(within(dialog).getByText("/opt/homebrew/bin/brew upgrade --formula glib")).toBeInTheDocument();
      expect(within(dialog).getByText("/opt/homebrew/bin/brew upgrade --cask onyx")).toBeInTheDocument();
      expect(submittedPlanIds()).toEqual([]);

      fireEvent.click(update);
      await waitFor(() => expect(submittedPlanIds()).toEqual([{ planId: "1" }, { planId: "2" }]));
    });

    it("is up at once for a row's own Update too, holding the focus itself until Update can take it", async () => {
      holdPlans.add("glib");
      const { findByRole } = renderPage();

      const rowUpdate = within(await findRow("glib")).getByRole("button", { name: "Update" });
      fireEvent.click(rowUpdate);
      const dialog = await findByRole("dialog", { name: "Update “glib”?" });
      expect(within(dialog).getByText("2.88.3 → 2.90.0")).toBeInTheDocument();
      expect(within(dialog).getByText("Preparing…")).toBeInTheDocument();
      const update = within(dialog).getByRole("button", { name: "Update" });
      expect(update).toBeDisabled();
      // One confirmation at a time: the row's Update is off behind it.
      expect(rowUpdate).toBeDisabled();
      // Not the disabled Update, and not the button under the dimmed page.
      await waitFor(() => expect(document.activeElement).toBe(dialog));

      await release("glib");
      await waitFor(() => expect(update).toBeEnabled());
      expect(document.activeElement).toBe(update);
      expect(within(dialog).queryByText("Preparing…")).toBeNull();
    });

    it("leaves the focus where the user put it while it was preparing", async () => {
      holdPlans.add("glib");
      const { findByRole } = renderPage();

      fireEvent.click(within(await findRow("glib")).getByRole("button", { name: "Update" }));
      const dialog = await findByRole("dialog", { name: "Update “glib”?" });
      const cancel = within(dialog).getByRole("button", { name: "Cancel" });
      cancel.focus();

      await release("glib");
      await waitFor(() => expect(within(dialog).getByRole("button", { name: "Update" })).toBeEnabled());
      // A key meant for Cancel is never taken by Update.
      expect(document.activeElement).toBe(cancel);
    });

    it("stays shut when the plans of a sheet closed while preparing arrive, and says nothing on the page", async () => {
      holdPlans.add("glib");
      const { findByRole, queryByRole } = renderPage();

      const rowUpdate = within(await findRow("glib")).getByRole("button", { name: "Update" });
      fireEvent.click(rowUpdate);
      const dialog = await findByRole("dialog", { name: "Update “glib”?" });
      fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
      await waitFor(() => expect(queryByRole("dialog")).toBeNull());
      await waitFor(() => expect(document.activeElement).toBe(rowUpdate));

      await release("glib");
      expect(queryByRole("dialog")).toBeNull();
      expect(queryByRole("alert")).toBeNull();
      expect(rowUpdate).toBeEnabled();
      expect(submittedPlanIds()).toEqual([]);
    });

    it("shuts when every plan is refused, and says why on the page, with the focus back on the row's Update", async () => {
      planFailures.glib = "glib is pinned";
      holdPlans.add("glib");
      const { findByRole, queryByRole } = renderPage();

      const rowUpdate = within(await findRow("glib")).getByRole("button", { name: "Update" });
      fireEvent.click(rowUpdate);
      const dialog = await findByRole("dialog", { name: "Update “glib”?" });
      expect(within(dialog).getByText("Preparing…")).toBeInTheDocument();

      await release("glib");
      expect(await findByRole("alert")).toHaveTextContent("Couldn't prepare the update: glib is pinned");
      expect(queryByRole("dialog")).toBeNull();
      await waitFor(() => expect(document.activeElement).toBe(rowUpdate));
    });
  });

  it("never shows an Ollama model's two digests as a version jump in the confirmation", async () => {
    // `current` is the local manifest digest and `target` is the registry
    // manifest's config digest: different hash spaces, unequal even after a
    // successful pull, and not something to put in front of this audience
    // either way. The row already knows this; the dialog has to as well.
    instances = [...snapshot.instances, { ...stoppedOllama, status: { unavailable: null, notes: [] } }];
    updates = [
      {
        key: qwenKey,
        current: "5642e97495e1a0888838ee1b3b1a0b1c6a0f0f5e6c2d4a8b9e7c3d1f0a2b4c6d",
        target: "sha256:9f1c0b6d2e4a7c5b3d1f8a6e4c2b0d9f7e5c3a1b8d6f4e2c0a9b7d5f3e1c8a6b",
        channel: "Digest",
        checkable: true,
        warnings: [],
        blocked: null,
      },
    ];

    const { findAllByRole, findByRole } = renderPage();

    fireEvent.click((await findAllByRole("button", { name: "Update" }))[0]);
    const dialog = await findByRole("dialog");

    expect(
      await within(dialog).findByText("This model has a new version"),
    ).toBeInTheDocument();
    expect(within(dialog).queryByText(/sha256:/)).toBeNull();
    expect(within(dialog).queryByText(/→/)).toBeNull();
  });

  const claudeKey: ArtifactKey = { instance_id: "standalone-claude", kind: "Binary", name: "claude" };
  const claudeInstance: Snapshot["instances"][number] = {
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
  const claudeArtifact: Snapshot["artifacts"][number] = {
    key: claudeKey,
    display_name: "Claude Code",
    version: "2.1.281",
    reason: "Requested",
    description: null,
    homepage: "https://code.claude.com/docs/en/setup",
    size_bytes: null,
    installed_at: null,
    path: "/Users/someone/.local/share/claude/versions/2.1.281",
    auto_updates: true,
    uninstall_blocked: "NoSafeMethod",
  };
  const claudeUpdate: Snapshot["updates"][number] = {
    key: claudeKey,
    current: "2.1.281",
    target: "2.1.290",
    channel: "Registry",
    checkable: true,
    warnings: [],
    blocked: null,
  };

  it("says a self-updating standalone tool usually updates itself, and still offers the button", async () => {
    // Spec D5: the update is real (read from the launcher's live version),
    // so the row keeps its Update button; the chip says the tool usually
    // does this itself.
    instances = [...snapshot.instances, claudeInstance];
    updates = [claudeUpdate];
    artifacts = [claudeArtifact];
    const { getAllByRole } = renderPage();

    const claude = await findRow("Claude Code");
    expect(within(claude).getByText("2.1.281 → 2.1.290")).toBeInTheDocument();
    // A plain label: its old ⓘ only said the label over again (polish-3
    // copy table, updates.selfUpdatingDetail).
    expect(within(claude).getByText("Updates itself")).toBeInTheDocument();
    expect(within(claude).queryByRole("button", { name: "Updates itself" })).toBeNull();
    expect(getAllByRole("button", { name: "Update" })).toHaveLength(1);
    expect(within(claude).getByRole("button", { name: "Update" })).toBeInTheDocument();
  });

  const claudeEndings: Array<[string, OpSummary["outcome"], string, boolean]> = [
    ["failed", { Failed: { exit_code: 1, summary: "Error: download failed" } }, "Couldn't update", true],
    ["was cancelled", "Cancelled", "Cancelled", false],
    ["asks to be checked", { NeedsAttention: "UnchangedAfterUpgrade" }, "Unexpected result", true],
  ];

  it.each(claudeEndings)(
    "gives its chip's place to how its update ended when it %s, and takes it back once a Retry runs",
    async (_name, outcome, words, logged) => {
      // Beside the chip, how it ended left the name a few letters at the
      // window's default width: 「Clau…」.
      instances = [...snapshot.instances, claudeInstance];
      updates = [claudeUpdate];
      artifacts = [claudeArtifact];
      operations = [operation(claudeKey, { id: 9, status: "Done", outcome })];
      useUiStore.setState({ updateTargets: { 9: claudeUpdate.target } });
      const { queryClient } = renderPage();

      const claude = await findRow("Claude Code");
      const chips = claude.querySelector<HTMLElement>("[data-status]");
      if (chips === null) throw new Error("the row has no chips' column");
      expect(await within(chips).findByText(words)).toBeInTheDocument();
      expect(within(chips).queryByRole("button", { name: "View log: Claude Code" }) !== null).toBe(logged);
      expect(within(claude).queryByText("Updates itself")).toBeNull();
      expect(within(claude).getByRole("button", { name: "Retry" })).toBeInTheDocument();

      // Retried: the update under way stands where the button was, and
      // the chip is back.
      operations = [operation(claudeKey, { id: 10, status: "Running" }), ...operations];
      await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.operations }));
      expect(await within(rowOf("Claude Code")).findByText("Updating…")).toBeInTheDocument();
      expect(within(rowOf("Claude Code")).getByText("Updates itself")).toBeInTheDocument();
      expect(within(rowOf("Claude Code")).queryByText(words)).toBeNull();
    },
  );

  it("takes the chip back from a failed update once the source offers a newer version", async () => {
    instances = [...snapshot.instances, claudeInstance];
    updates = [claudeUpdate];
    artifacts = [claudeArtifact];
    operations = [
      operation(claudeKey, {
        id: 9,
        status: "Done",
        outcome: { Failed: { exit_code: 1, summary: "Error: download failed" } },
      }),
    ];
    useUiStore.setState({ updateTargets: { 9: claudeUpdate.target } });
    const { queryClient } = renderPage();

    const claude = await findRow("Claude Code");
    expect(await within(claude).findByText("Couldn't update")).toBeInTheDocument();
    expect(within(claude).queryByText("Updates itself")).toBeNull();

    // "Couldn't update" was about 2.1.290; 2.1.291 gets the button, and the chip, back.
    updates = [{ ...claudeUpdate, target: "2.1.291" }];
    await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.snapshot }));
    await waitFor(() => expect(within(rowOf("Claude Code")).queryByText("Couldn't update")).toBeNull());
    expect(within(rowOf("Claude Code")).getByText("Updates itself")).toBeInTheDocument();
    expect(within(rowOf("Claude Code")).getByRole("button", { name: "Update" })).toBeInTheDocument();
  });

  it("keeps a self-updating Homebrew cask a plain row: the chip is for tools that update themselves, not for --greedy", async () => {
    // A cask listed through `include_self_updating` carries
    // `auto_updates: true` too, but Homebrew, not the app, is what the
    // button drives; its row keeps its description and no chip.
    settings = { ...settings, include_self_updating: true };
    updates = [snapshot.updates[1]];
    artifacts = [
      {
        key: onyxKey,
        display_name: "OnyX",
        version: "5.0.2",
        reason: "Requested",
        description: "Verify system files structure",
        homepage: null,
        size_bytes: null,
        installed_at: null,
        path: null,
        auto_updates: true,
        uninstall_blocked: null,
      },
    ];
    renderPage();

    const onyx = await findRow("OnyX");
    expect(within(onyx).getByText("Verify system files structure")).toBeInTheDocument();
    expect(within(onyx).queryByText("Updates itself")).toBeNull();
  });

  it("gives a standalone tool's row the summary its Installed row shows, not 'No description'", async () => {
    // A standalone artifact's `description` is `null` on the wire (a bare
    // string cannot be localised), so its line is looked up by adapter
    // id, on this page as on the Installed page (`toolDescription`). Grok
    // Build is not called self-updating (`auto_updates: false`: whether it
    // installs updates on its own is unverified), so its row gets no chip.
    const grokKey: ArtifactKey = { instance_id: "standalone-grok", kind: "Binary", name: "grok" };
    instances = [
      ...snapshot.instances,
      {
        id: "standalone-grok",
        adapter_id: "standalone-grok",
        exe_path: "/Users/someone/.grok/bin/grok",
        prefix: "/Users/someone/.grok",
        scope: "User",
        version: "1.0.41",
        status: { unavailable: null, notes: [] },
        unverified_version: null,
        read_only_reason: null,
      },
    ];
    updates = [
      {
        key: grokKey,
        current: "1.0.41",
        target: "1.0.42",
        channel: "Native",
        checkable: true,
        warnings: [],
        blocked: null,
      },
    ];
    artifacts = [
      {
        key: grokKey,
        display_name: "Grok Build",
        version: "1.0.41",
        reason: "Requested",
        description: null,
        homepage: null,
        size_bytes: null,
        installed_at: null,
        path: "/Users/someone/.grok/downloads/grok-1.0.41-macos-aarch64",
        auto_updates: false,
        uninstall_blocked: null,
      },
    ];
    const { queryByText, getAllByRole } = renderPage();

    const grok = await findRow("Grok Build");
    expect(within(grok).getByText("xAI's AI coding assistant")).toBeInTheDocument();
    expect(queryByText("No description")).toBeNull();
    expect(within(grok).queryByText("Updates itself")).toBeNull();
    expect(getAllByRole("button", { name: "Update" })).toHaveLength(1);
  });

  it("says what each row's source says it is when the source gave no description, in both languages", async () => {
    // Cargo's inventory never carries a description, and some of
    // Homebrew's casks have none: each such row says what its source says
    // it is -- an app's cask is an app, a font's is not called one -- and
    // never "No description".
    const fontKey: ArtifactKey = { instance_id: "brew:/opt/homebrew", kind: "Cask", name: "font-jetbrains-mono" };
    const tokeiKey: ArtifactKey = { instance_id: "cargo:/Users/brulek/.cargo", kind: "Binary", name: "tokei" };
    const bare = (key: ArtifactKey, displayName: string, path: string | null): Snapshot["artifacts"][number] => ({
      key,
      display_name: displayName,
      version: "1.0.0",
      reason: "Requested",
      description: null,
      homepage: null,
      size_bytes: null,
      installed_at: null,
      path,
      auto_updates: false,
      uninstall_blocked: null,
    });
    artifacts = [
      bare(onyxKey, "OnyX", "/Applications/OnyX.app"),
      bare(fontKey, "JetBrains Mono", null),
      bare(tokeiKey, "tokei", "/Users/brulek/.cargo/bin/tokei"),
    ];
    updates = [
      ...snapshot.updates,
      { ...brewCandidate("font-jetbrains-mono"), key: fontKey },
      { ...brewCandidate("tokei"), key: tokeiKey, channel: "Registry" },
    ];
    const { queryByText } = renderPage();

    expect(within(await findRow("OnyX")).getByText("App installed with Homebrew")).toBeInTheDocument();
    expect(within(rowOf("JetBrains Mono")).getByText("Homebrew package")).toBeInTheDocument();
    expect(within(rowOf("tokei")).getByText("Program installed with Cargo")).toBeInTheDocument();
    expect(within(rowOf("glib")).getByText("Homebrew package")).toBeInTheDocument();
    expect(queryByText("No description")).toBeNull();

    await act(async () => {
      await i18n.changeLanguage("zh-CN");
    });
    try {
      expect(within(rowOf("OnyX")).getByText("用Homebrew安装的App")).toBeInTheDocument();
      expect(within(rowOf("JetBrains Mono")).getByText("Homebrew软件包")).toBeInTheDocument();
      expect(within(rowOf("tokei")).getByText("用Cargo安装的程序")).toBeInTheDocument();
      expect(queryByText("暂无简介")).toBeNull();
    } finally {
      await act(async () => {
        await i18n.changeLanguage("en");
      });
    }
  });

  it("says a row's line in Chinese where the table has one, and what the row said where it has none", async () => {
    // A table of this test's own: glib's line and tokei's -- Cargo's
    // inventory gives no description -- and none for OnyX.
    const toolDescriptions = lazyDescriptionTable(async () => ({
      "brew:glib": "C 语言核心应用库",
      "cargo:tokei": "代码行数统计工具",
    }));
    const tokeiKey: ArtifactKey = { instance_id: "cargo:/Users/brulek/.cargo", kind: "Binary", name: "tokei" };
    const described = (key: ArtifactKey, displayName: string, description: string | null) => ({
      key,
      display_name: displayName,
      version: "1.0.0",
      reason: "Requested" as const,
      description,
      homepage: null,
      size_bytes: null,
      installed_at: null,
      path: null,
      auto_updates: false,
      uninstall_blocked: null,
    });
    artifacts = [
      described(glibKey, "glib", "Core application library for C"),
      described(onyxKey, "OnyX", "Verify system files structure"),
      described(tokeiKey, "tokei", null),
    ];
    updates = [...snapshot.updates, { ...brewCandidate("tokei"), key: tokeiKey, channel: "Registry" }];
    renderPage({ toolDescriptions: { "zh-CN": toolDescriptions } });

    expect(within(await findRow("glib")).getByText("Core application library for C")).toBeInTheDocument();
    expect(within(rowOf("tokei")).getByText("Program installed with Cargo")).toBeInTheDocument();

    await act(async () => {
      await i18n.changeLanguage("zh-CN");
    });
    try {
      expect(await within(rowOf("glib")).findByText("C 语言核心应用库")).toBeInTheDocument();
      expect(within(rowOf("glib")).queryByText("Core application library for C")).toBeNull();
      expect(within(rowOf("tokei")).getByText("代码行数统计工具")).toBeInTheDocument();
      expect(within(rowOf("OnyX")).getByText("Verify system files structure")).toBeInTheDocument();
    } finally {
      await act(async () => {
        await i18n.changeLanguage("en");
      });
    }
    expect(within(rowOf("glib")).getByText("Core application library for C")).toBeInTheDocument();
    expect(within(rowOf("tokei")).getByText("Program installed with Cargo")).toBeInTheDocument();
  });

  it("says a crate's line in English where the English table has one, and switches it with the language", async () => {
    // Tables of this test's own: tokei's line in each language -- Cargo's
    // inventory gives no description -- and none for my-fork in either.
    const toolDescriptions = {
      en: lazyDescriptionTable(async () => ({ "cargo:tokei": "Code line counter" })),
      "zh-CN": lazyDescriptionTable(async () => ({ "cargo:tokei": "代码行数统计工具" })),
    };
    const tokeiKey: ArtifactKey = { instance_id: "cargo:/Users/brulek/.cargo", kind: "Binary", name: "tokei" };
    const crate = (key: ArtifactKey) => ({
      key,
      display_name: key.name,
      version: "1.0.0",
      reason: "Requested" as const,
      description: null,
      homepage: null,
      size_bytes: null,
      installed_at: null,
      path: null,
      auto_updates: false,
      uninstall_blocked: null,
    });
    artifacts = [crate(tokeiKey), crate(myForkKey)];
    updates = [
      { ...brewCandidate("tokei"), key: tokeiKey, channel: "Registry" },
      { ...brewCandidate("my-fork"), key: myForkKey, channel: "Registry" },
    ];
    renderPage({ toolDescriptions });

    expect(await within(await findRow("tokei")).findByText("Code line counter")).toBeInTheDocument();
    expect(within(rowOf("tokei")).queryByText("Program installed with Cargo")).toBeNull();
    expect(within(rowOf("my-fork")).getByText("Program installed with Cargo")).toBeInTheDocument();

    await act(async () => {
      await i18n.changeLanguage("zh-CN");
    });
    try {
      expect(await within(rowOf("tokei")).findByText("代码行数统计工具")).toBeInTheDocument();
      expect(within(rowOf("tokei")).queryByText("Code line counter")).toBeNull();
      expect(within(rowOf("my-fork")).getByText("用Cargo安装的程序")).toBeInTheDocument();
    } finally {
      await act(async () => {
        await i18n.changeLanguage("en");
      });
    }
    // English again: its line, at once.
    expect(within(rowOf("tokei")).getByText("Code line counter")).toBeInTheDocument();
    expect(within(rowOf("my-fork")).getByText("Program installed with Cargo")).toBeInTheDocument();
  });

  it("gives a standalone row that cannot be checked its reason, not the self-updating chip", async () => {
    instances = [...snapshot.instances, claudeInstance];
    updates = [
      {
        ...claudeUpdate,
        target: "2.1.281",
        checkable: false,
        warnings: [{ Message: "downloads.claude.ai request failed: network error: offline" }],
      },
    ];
    artifacts = [claudeArtifact];
    const { queryAllByRole } = renderPage();

    await showCantUpdate();
    const claude = await findRow("Claude Code");
    expect(chipDetail(claude, "Can't check").textContent).toBe("Couldn't find its latest version.");
    expect(within(claude).queryByText("Updates itself")).toBeNull();
    expect(queryAllByRole("button", { name: "Update" })).toHaveLength(0);
  });

  it("gives no self-updating chip to a standalone row whose source did not answer: it has no button to offer", async () => {
    // A candidate carried forward from a Claude Code that did not answer
    // the last refresh has no Update button (`isUpdateActionable` needs
    // `isAvailable`), so "you can also update it now" would point at a
    // button that is not there.
    instances = [
      ...snapshot.instances,
      { ...claudeInstance, status: { unavailable: "NotResponding", notes: [] } },
    ];
    updates = [claudeUpdate];
    artifacts = [claudeArtifact];
    const { queryAllByRole } = renderPage();

    await showCantUpdate();
    const claude = await findRow("Claude Code");
    expect(within(claude).queryByText("Updates itself")).toBeNull();
    expect(within(claude).getByRole("button", { name: "Can't update now" })).toBeInTheDocument();
    expect(queryAllByRole("button", { name: "Update" })).toHaveLength(0);
  });

  it("offers no Update button for a tool that updates itself, and says to open it once", async () => {
    // Spec §4.4, D5 item 4: the newer version is real (read from the
    // launcher's live version), so the row stays and is counted with what
    // Canager cannot update; the tool has no update command Canager could
    // run, so there is no button and no checkbox, and the detail says what
    // does work: opening it. The launcher is a path, shown only with
    // technical details on. The claude fixtures stand in for agy here: the
    // copy record is per reason, not per tool.
    instances = [...snapshot.instances, claudeInstance];
    updates = [{ ...claudeUpdate, blocked: "SelfUpdatesOnly" }, snapshot.updates[1]];
    artifacts = [...snapshot.artifacts, claudeArtifact];
    const { findByText, getAllByRole, queryByText } = renderPage();

    await findRow("onyx");
    // Only onyx's.
    expect(getAllByRole("button", { name: "Update" })).toHaveLength(1);
    expect(getAllByRole("checkbox", { name: ROW_CHECKBOX })).toHaveLength(1);
    expect(getAllByRole("checkbox", { name: ROW_CHECKBOX })[0]).toHaveAccessibleName("Select onyx for update");
    await findByText("1 Update Available");
    await findByText("1 more can't be updated here");
    await showCantUpdate();
    const claude = await findRow("Claude Code");
    // Under "can't be updated here": no version to move to here.
    expect(within(claude).queryByText("2.1.281 → 2.1.290")).toBeNull();
    const detail = chipDetail(claude, "Only updates itself");
    expect(detail.textContent).toBe("It updates itself and can't be updated here. Open it once and it checks for a new version.");
    expect(queryByText(/\.local\/bin\/claude/)).toBeNull();
    // Not the updatable row's chip: this row has no button to point at.
    expect(queryByText(/also update it now/)).toBeNull();
  });

  it("shows the command that opens a tool that updates itself, and offers to copy it, with technical details on", async () => {
    settings.show_technical_details = true;
    instances = [...snapshot.instances, claudeInstance];
    updates = [{ ...claudeUpdate, blocked: "SelfUpdatesOnly" }];
    artifacts = [claudeArtifact];
    renderPage();

    await showCantUpdate();
    const claude = await findRow("Claude Code");
    const detail = chipDetail(claude, "Only updates itself");
    expect(
      within(detail).getByText(wholeSentence("In Terminal: /Users/someone/.local/bin/claude")),
    ).toBeInTheDocument();
    expect(within(detail).getByText("/Users/someone/.local/bin/claude").tagName).toBe("CODE");
    fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });
    expect(
      within(openMenu(claude)).getByRole("menuitem", { name: "Copy Command" }),
    ).toBeInTheDocument();
  });

  // The four PATH notes (spec §七), each with the title of the notice it
  // puts at the top of the page.
  const pathNotes: [InstanceNote, string][] = [
    ["NotOnPath", "Claude Code is installed, but typing claude in Terminal doesn't run it"],
    ["ShadowedByHomebrew", "Typing claude runs a same-named program from Homebrew first"],
    ["ShadowedByNpm", "Typing claude runs a same-named program from npm first"],
    ["ShadowedByOther", "Typing claude runs another program with the same name first"],
  ];

  it.each(pathNotes)(
    "does not say a self-updating standalone copy under a %s notice updates itself, and keeps its button",
    async (note, noticeTitle) => {
      // Typing `claude` in Terminal probably does not run this copy: it
      // is not on PATH, so nothing or another program named `claude` runs
      // (`NotOnPath`), or another program with that name is found on PATH
      // before it (`ShadowedBy*`) -- the notice says which. This copy
      // updates itself only when it runs (spec §4.4), so "it usually
      // updates itself" would leave it behind with its update up. The row
      // is a plain one: behind, with its version and its button.
      instances = [
        ...snapshot.instances,
        { ...claudeInstance, status: { unavailable: null, notes: [note] } },
      ];
      updates = [claudeUpdate];
      artifacts = [claudeArtifact];
      const { findByText, queryByText, getAllByRole } = renderPage();

      expect(await findByText(noticeTitle)).toBeInTheDocument();
      const claude = await findRow("Claude Code");
      expect(within(claude).queryByText("Updates itself")).toBeNull();
      expect(queryByText(/usually updates itself|just run it/i)).toBeNull();
      expect(within(claude).getByText("2.1.281 → 2.1.290")).toBeInTheDocument();
      expect(getAllByRole("button", { name: "Update" })).toHaveLength(1);
    },
  );

  it.each(pathNotes)(
    "says everything is up to date under a %s notice: what typing the name runs is not whether Canager could check it",
    async (note, noticeTitle) => {
      // One source, Claude Code, which answered: with no updates listed,
      // Canager read this copy's version and the published one, and the
      // published one is not newer. The note is only about what typing
      // `claude` in Terminal runs, so its notice goes above the sentence
      // and leaves the sentence alone.
      instances = [{ ...claudeInstance, status: { unavailable: null, notes: [note] } }];
      updates = [];
      artifacts = [claudeArtifact];
      const { findByText, queryByText } = renderPage();

      expect(await findByText(noticeTitle)).toBeInTheDocument();
      expect(await findByText("Everything is up to date")).toBeInTheDocument();
      expect(queryByText("No updates in the sources checked")).toBeNull();
    },
  );

  it("does not say everything is up to date when Claude Code's launcher is left without its program: there was no installed version to check", async () => {
    // Its program files are gone, so `StandaloneAdapter::check_updates`
    // returns before reading either version: there is no installed one to
    // compare with the published one. No updates from it means it was not
    // checked.
    instances = [
      { ...claudeInstance, version: null, status: { unavailable: null, notes: ["LauncherOnly"] } },
    ];
    updates = [];
    artifacts = [{ ...claudeArtifact, version: "", path: null }];
    const { findByText, queryByText } = renderPage();

    expect(await findByText("Claude Code's program files are missing")).toBeInTheDocument();
    expect(await findByText("No updates in the sources checked")).toBeInTheDocument();
    expect(queryByText("Everything is up to date")).toBeNull();
  });

  it("calls the two self-updating chips by what each leaves the user, in both languages", () => {
    // 会自行更新: a tool Canager can update too, which does it itself.
    // 只能自行更新: one only it can update, so its row has no button. They
    // once read 会自动更新 and 自动更新, and "Updates itself" both in
    // English, which said nothing of why one row had a button. (The
    // polish-3 spec, 3.4, would call both 会自行更新; the 只能 stays, as
    // the one word that says why a row has no button.)
    expect(zhCN.updates.selfUpdating).toBe("会自行更新");
    expect("selfUpdatingDetail" in zhCN.updates).toBe(false);
    expect(zhCN.updates.blocked.SelfUpdatesOnly.badge).toBe("只能自行更新");
    expect(zhCN.updates.blocked.SelfUpdatesOnly.badge).not.toBe(zhCN.updates.selfUpdating);
    expect(en.updates.selfUpdating).toBe("Updates itself");
    expect(en.updates.blocked.SelfUpdatesOnly.badge).toBe("Only updates itself");
  });

  it("says per item, under its command, that a NoCancel update cannot be stopped once it starts", async () => {
    // A batch can mix a rustup self update (NoCancel) with a Homebrew
    // upgrade (cancellable); the sentence belongs next to the command it
    // is true of. This page is one of the two readers spec §五 gives
    // operations.noCancelHint; UninstallDialog is the other.
    noCancel.add("onyx");
    const { findAllByRole, getByRole, findByRole } = renderPage();

    const checkboxes = await findAllByRole("checkbox", { name: ROW_CHECKBOX });
    fireEvent.click(checkboxes[0]);
    fireEvent.click(checkboxes[1]);

    fireEvent.click(getByRole("button", { name: /^Update Selected/ }));
    const dialog = await findByRole("dialog");
    showCommands(dialog);
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula glib");
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --cask onyx");

    const hints = within(dialog).getAllByText(
      "This can't be cancelled once it starts. Don't quit Canager or shut down your Mac until it finishes.",
    );
    expect(hints).toHaveLength(1);
    const row = hints[0].closest("[data-sheet-tool]");
    expect(row?.querySelector("[data-sheet-name]")).toHaveTextContent("onyx");
    // A caution: marked ⚠︎ before its words.
    expect(hints[0].closest("li")).toHaveAttribute("data-caution");
  });

  describe("the keyboard (R11)", () => {
    it("moves between the rows with ↑ and ↓, ticks the focused one with Space, and does nothing on Enter", async () => {
      updates = [...snapshot.updates, brewCandidate("jq"), { ...brewCandidate("wget"), blocked: "Pinned" }];
      instances = [...snapshot.instances, stoppedOllama];
      const { findByRole, getByRole, queryByRole } = renderPage();

      await findRow("jq");
      // The notice's line is the list's first row, and not one the arrows
      // stop at: the first row is the one in the Tab order.
      await findByRole("button", { name: "Open Ollama" });
      const [glib, jq, onyx] = ["glib", "jq", "onyx"].map(rowOf);
      expect(glib).toHaveAttribute("tabindex", "0");
      expect(jq).toHaveAttribute("tabindex", "-1");
      expect(onyx).toHaveAttribute("tabindex", "-1");

      act(() => glib.focus());
      fireEvent.keyDown(glib, { key: "ArrowDown" });
      expect(document.activeElement).toBe(jq);
      expect(jq).toHaveAttribute("tabindex", "0");
      expect(glib).toHaveAttribute("tabindex", "-1");

      // Space ticks the focused row, as its box would.
      fireEvent.keyDown(jq, { key: " " });
      expect(getByRole("checkbox", { name: "Select jq for update" })).toBeChecked();
      expect(getByRole("button", { name: "Update Selected (1)" })).toBeEnabled();
      expect((getByRole("checkbox", { name: SELECT_ALL }) as HTMLInputElement).indeterminate).toBe(true);
      fireEvent.keyDown(jq, { key: " " });
      expect(getByRole("checkbox", { name: "Select jq for update" })).not.toBeChecked();

      // Enter opens nothing and starts nothing.
      fireEvent.keyDown(jq, { key: "Enter" });
      expect(queryByRole("dialog")).toBeNull();
      expect(calls("plan_operation")).toEqual([]);

      // On to the last row it can update, then the line that discloses
      // the rest, which Space opens as the button it is.
      fireEvent.keyDown(jq, { key: "ArrowDown" });
      expect(document.activeElement).toBe(onyx);
      fireEvent.keyDown(onyx, { key: "ArrowDown" });
      const disclosure = getByRole("button", { name: "1 more can't be updated here" });
      expect(document.activeElement).toBe(disclosure);
      fireEvent.click(disclosure);
      const wget = await findRow("wget");
      fireEvent.keyDown(disclosure, { key: "ArrowDown" });
      expect(document.activeElement).toBe(wget);
      // A row with no checkbox: Space does nothing to the selection.
      fireEvent.keyDown(wget, { key: " " });
      expect(useUiStore.getState().selectedUpdates).toEqual([]);

      // And back up.
      fireEvent.keyDown(wget, { key: "ArrowUp" });
      expect(document.activeElement).toBe(disclosure);
      fireEvent.keyDown(disclosure, { key: "ArrowUp" });
      expect(document.activeElement).toBe(onyx);
    });

    it("leaves ↑ and ↓ to an open ⋯ menu", async () => {
      renderPage();
      const glib = await findRow("glib");
      const menu = openMenu(glib);
      const [first, second] = within(menu).getAllByRole("menuitem");
      expect(document.activeElement).toBe(first);
      fireEvent.keyDown(first, { key: "ArrowDown" });
      expect(document.activeElement).toBe(second);
    });
  });

  describe("the list header's box", () => {
    // 「全选」: a box in the rows' checkbox column, over the list, that
    // ticks every row with a checkbox -- ticked for all, a dash for some.
    // Its accessible name starts with the words beside it ("Select All")
    // and goes on to say which rows it acts on (SELECT_ALL).
    /** The header's box, and whether it shows the dash. */
    function headerBox(): HTMLInputElement {
      return screen.getByRole("checkbox", { name: SELECT_ALL }) as HTMLInputElement;
    }

    const jq = brewCandidate("jq");
    const wget: Snapshot["updates"][number] = { ...brewCandidate("wget"), blocked: "Pinned" };
    const tree = brewCandidate("tree");
    const curl = brewCandidate("curl");

    // A row in each of the five `UpdateState`s, and two rows the page does
    // not list. Only glib, onyx and jq have a checkbox: wget is pinned,
    // my-fork could not be checked, urllib3 is pip's (read-only), qwen3:8b's
    // Ollama is not running, tree is never reminded about, and the version
    // curl offers was skipped.
    function listEveryKindOfRow() {
      instances = [...snapshot.instances, stoppedOllama];
      updates = [
        ...snapshot.updates,
        jq,
        wget,
        tree,
        curl,
        {
          key: myForkKey,
          current: "0.1.0",
          target: "0.1.0",
          channel: "Registry",
          checkable: false,
          warnings: ["NonRegistrySource"],
          blocked: null,
        },
        {
          key: urllib3Key,
          current: "2.2.1",
          target: "2.3.0",
          channel: "Registry",
          checkable: true,
          warnings: [],
          blocked: null,
        },
        {
          key: qwenKey,
          current: "5642e97495e1",
          target: "a1b2c3d4e5f6",
          channel: "Digest",
          checkable: true,
          warnings: [],
          blocked: null,
        },
      ];
      settings.ignored_updates = [tree.key];
      settings.skipped_versions = [{ key: curl.key, version: curl.target }];
    }

    // The selection as a sorted list of ids: these tests are about which
    // rows are ticked, not the order the store keeps them in.
    function selectIdsOf(): string[] {
      return [...useUiStore.getState().selectedUpdates].sort();
    }

    function ids(...keys: ArtifactKey[]): string[] {
      return keys.map(artifactKeyId).sort();
    }

    function sortedPlannedNames(): string[] {
      return [...plannedNames()].sort();
    }

    it("ticks every row that has a checkbox and nothing else, and Update selected plans exactly those", async () => {
      listEveryKindOfRow();
      const { findAllByRole, getByRole, findByRole, queryAllByRole, queryByRole } = renderPage();

      // Unfolded, so every listed row is on the page: still three boxes.
      await showCantUpdate();
      await findRow("my-fork");
      const checkboxes = await findAllByRole("checkbox", { name: ROW_CHECKBOX });
      expect(checkboxes).toHaveLength(3);
      expect(queryAllByRole("checkbox", { name: ROW_CHECKBOX })).toHaveLength(3);
      const selectAll = headerBox();
      // Its words beside it, which pressing also ticks it by (a <label>).
      expect(selectAll.closest("label")?.textContent).toBe("Select All");
      expect(selectAll).not.toBeChecked();
      expect(queryByRole("button", { name: /^Update Selected/ })).not.toBeInTheDocument();

      fireEvent.click(selectAll);

      for (const checkbox of checkboxes) expect(checkbox).toBeChecked();
      expect(selectIdsOf()).toEqual(ids(glibKey, onyxKey, jq.key));
      expect(selectAll).toBeChecked();
      expect(selectAll.indeterminate).toBe(false);

      fireEvent.click(getByRole("button", { name: "Update Selected (3)" }));
      await findByRole("dialog");
      expect(sortedPlannedNames()).toEqual(["glib", "jq", "onyx"]);
    });

    it("shows a dash for some rows ticked, ticks the rest from there, and unticks them all once all are", async () => {
      listEveryKindOfRow();
      const { findByRole, getByRole, queryByRole } = renderPage();

      const glib = await findByRole("checkbox", { name: "Select glib for update" });
      const onyx = getByRole("checkbox", { name: "Select onyx for update" });
      const jqBox = getByRole("checkbox", { name: "Select jq for update" });
      expect(headerBox().indeterminate).toBe(false);

      fireEvent.click(glib);
      // Some, not all: the dash, and not ticked.
      expect(headerBox().indeterminate).toBe(true);
      expect(headerBox()).not.toBeChecked();

      // From some, pressed: all.
      fireEvent.click(headerBox());
      expect([glib, onyx, jqBox].every((box) => (box as HTMLInputElement).checked)).toBe(true);
      expect(headerBox()).toBeChecked();
      expect(headerBox().indeterminate).toBe(false);
      expect(getByRole("button", { name: "Update Selected (3)" })).toBeEnabled();

      // From all, pressed: none, and the toolbar's button is Update all again.
      fireEvent.click(headerBox());
      expect(selectIdsOf()).toEqual([]);
      expect([glib, onyx, jqBox].some((box) => (box as HTMLInputElement).checked)).toBe(false);
      expect(headerBox()).not.toBeChecked();
      expect(queryByRole("button", { name: /^Update Selected/ })).not.toBeInTheDocument();
      expect(getByRole("button", { name: "Update All" })).toBeEnabled();

      // Ticking every row by hand ticks it too.
      for (const box of [glib, onyx, jqBox]) fireEvent.click(box);
      expect(headerBox()).toBeChecked();
    });

    it("never selects a row the page does not list, and leaves a pinned row's earlier selection as it was", async () => {
      // wget was selected while it could still be updated; a refresh since
      // says it is pinned, so it has no checkbox. The box may neither take
      // it out of the selection nor put tree (never reminded about) or curl
      // (skipped) into it: it acts on the rows with a checkbox and nothing
      // else. Update selected still leaves wget out of the batch
      // (`isActionable`).
      listEveryKindOfRow();
      act(() => {
        useUiStore.getState().toggleUpdate(wget.key);
      });
      const { findAllByRole, getByRole, findByRole } = renderPage();
      await findAllByRole("checkbox", { name: ROW_CHECKBOX });
      // wget's tick is not one the list shows.
      expect(headerBox().indeterminate).toBe(false);
      expect(headerBox()).not.toBeChecked();

      fireEvent.click(headerBox());
      expect(selectIdsOf()).toEqual(ids(wget.key, glibKey, onyxKey, jq.key));

      fireEvent.click(headerBox());
      expect(selectIdsOf()).toEqual(ids(wget.key));
      // A row with no checkbox is all that is left selected: nothing to update.
      expect(getByRole("button", { name: "Update All" })).toBeEnabled();

      fireEvent.click(headerBox());
      expect(selectIdsOf()).toEqual(ids(wget.key, glibKey, onyxKey, jq.key));

      fireEvent.click(getByRole("button", { name: "Update Selected (3)" }));
      await findByRole("dialog");
      expect(sortedPlannedNames()).toEqual(["glib", "jq", "onyx"]);
    });

    it("is off, and so is Update all, when no listed row has a checkbox", async () => {
      // tree and curl could be updated but are hidden, so they are not
      // listed; every row that is listed is one Canager cannot update.
      listEveryKindOfRow();
      const withCheckbox = ids(glibKey, onyxKey, jq.key);
      updates = updates.filter((u) => !withCheckbox.includes(artifactKeyId(u.key)));
      const { findByText, getByRole, queryByRole } = renderPage();

      await findByText("Nothing to update here");
      await showCantUpdate();
      await findRow("my-fork");
      expect(queryByRole("checkbox", { name: ROW_CHECKBOX })).toBeNull();
      const selectAll = headerBox();
      expect(selectAll).toBeDisabled();
      // Its words in the colour of text that is off.
      expect(selectAll.closest("label")?.querySelector("span")?.className).toContain("text-tertiary");
      expect(getByRole("button", { name: "Update All" })).toBeDisabled();

      fireEvent.click(selectAll);
      expect(useUiStore.getState().selectedUpdates).toEqual([]);
    });

    it("sits over the list, 28 high, the box in the rows' checkbox column, and stays put while the list scrolls", async () => {
      const { container } = renderPage();
      await findRow("glib");

      const header = headerBox().closest("label")?.parentElement as HTMLElement;
      expect(header.className.split(" ")).toEqual(expect.arrayContaining(["h-7", "px-5", "border-b", "border-separator"]));
      // Outside the list's scroller, so it does not scroll with it.
      expect(slotOf(headerBox())).toBeNull();
      const scroller = container.querySelector("[data-list-slot]")?.parentElement?.parentElement as HTMLElement;
      expect(scroller.contains(header)).toBe(false);
      expect(header.compareDocumentPosition(scroller) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
      // The rows' checkboxes are 20 in too: the row's own edge, then the box.
      expect((rowOf("glib") as HTMLElement).className.split(" ")).toContain("px-5");
      expect(within(rowOf("glib")).getByRole("checkbox").parentElement?.className).toContain("w-4");
    });

    it("calls it 全选 in Chinese, as it was asked for", () => {
      expect(zhCN.updates.selectAll).toBe("全选");
      // Invert selection is gone with its button: the box does both.
      expect("invertSelection" in zhCN.updates).toBe(false);
      expect("invertSelection" in en.updates).toBe(false);
    });
  });
});

describe("UpdatesPage's virtualized list", () => {
  it("keeps each slot's measured height with that slot when an update removes the row above it", async () => {
    // Heights by what a slot holds, as a browser would measure them: a row
    // is 56px, the "Can't update here" toggle 40px.
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (
      this: HTMLElement,
    ) {
      if (this.getAttribute("data-index") === null) return 600;
      if (this.textContent?.endsWith("more can't be updated here")) return 40;
      return ROW_HEIGHT;
    });
    const urllib3 = {
      key: urllib3Key,
      current: "2.5.0",
      target: "2.6.0",
      channel: "Registry" as const,
      checkable: true,
      warnings: [],
      blocked: null,
    };
    updates = [snapshot.updates[0], urllib3];
    const { getByText, queryByText, queryClient } = renderPage();
    const slotTop = (element: HTMLElement) =>
      (element.closest("[data-index]") as HTMLElement).style.transform;

    await showCantUpdate();
    const urllib3Row = await findRow("urllib3");
    // glib's row, the toggle, urllib3.
    expect(slotTop(urllib3Row)).toBe(`translateY(${ROW_HEIGHT + 40}px)`);

    // glib finishes updating: the refresh after it leaves nothing to update,
    // so the toggle moves to the top.
    act(() => {
      queryClient.setQueryData(queryKeys.snapshot, {
        ...snapshot,
        generation: snapshot.generation + 1,
        updates: [urllib3],
        instances,
        artifacts,
      });
    });
    await waitFor(() => expect(queryByText("glib")).not.toBeInTheDocument());

    // The toggle keeps its own 40px. Keyed by position, the slot at the top
    // kept glib's 56px, and urllib3 would be drawn 16px too low.
    expect(slotTop(getByText("1 more can't be updated here"))).toBe("translateY(0px)");
    expect(slotTop(rowOf("urllib3"))).toBe("translateY(40px)");
  });
});
