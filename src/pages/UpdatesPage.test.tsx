import { afterEach, describe, expect, it, vi, beforeEach } from "vitest";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { invoke, type InvokeArgs } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { UpdatesPage } from "./UpdatesPage";
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

// Unfolds "Can't update here (N)".
async function showCantUpdate() {
  const toggle = await screen.findByRole("button", { name: /^Can't update here/ });
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
  const disclosure = within(dialog).getByRole("button", { name: /^Show the command/ });
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
  it("shows the first check's ring and why it takes a while until the backend has answered, not Loading…", async () => {
    // At launch `get_snapshot` has not answered yet: the first check is
    // under way, as the Overview says in the same words.
    const answer = mockInvoke.getMockImplementation();
    mockInvoke.mockImplementation((cmd: string, args?: InvokeArgs) =>
      cmd === "get_snapshot" ? new Promise(() => {}) : answer!(cmd, args),
    );
    const { findByRole, getByText, queryByText, container } = renderWithProviders(<UpdatesPage />);

    expect(await findByRole("heading", { level: 2, name: "Checking…" })).toBeInTheDocument();
    expect(
      getByText("The first check looks up every tool's newest version online, and sometimes takes a minute or two."),
    ).toBeInTheDocument();
    expect(container.querySelector("[data-ring]")?.getAttribute("data-ring")).toBe("checking");
    expect(queryByText("Loading…")).not.toBeInTheDocument();
  });

  it("shows each update's version change in its version column, in tabular numerals, with technical details off", async () => {
    // The redesign's rule for a row: what it is, which version, what can
    // be done (docs/superpowers/2026-09-27-ui-redesign.md, 原则 1). The
    // version jump is on every row now; "Show technical details" keeps
    // paths, commands and the tools' own error text.
    expect(settings.show_technical_details).toBe(false);
    const { findByText } = renderWithProviders(<UpdatesPage />);

    const glib = await findByText("2.88.3 → 2.90.0");
    expect(glib.className).toContain("tabular-nums");
    expect(await findByText("5.0.2 → 5.1.0")).toBeInTheDocument();
  });

  it("lists the rows it can update by name, whatever order the sources gave them in", async () => {
    updates = [brewCandidate("wget"), brewCandidate("aria2"), ...snapshot.updates, brewCandidate("Zstd")];
    renderWithProviders(<UpdatesPage />);

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
    renderWithProviders(<UpdatesPage />);

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
    const { findAllByRole, findByRole, queryByRole } = renderWithProviders(<UpdatesPage />);

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
    const { findAllByRole, getByRole, findByRole, queryByRole } = renderWithProviders(<UpdatesPage />);

    const checkboxes = await findAllByRole("checkbox");
    fireEvent.click(checkboxes[0]);
    fireEvent.click(checkboxes[1]);

    // The button counts what it will update.
    fireEvent.click(getByRole("button", { name: "Update selected (2)" }));
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
    const { findAllByRole, getByRole, findByRole } = renderWithProviders(<UpdatesPage />);

    const checkboxes = await findAllByRole("checkbox");
    fireEvent.click(checkboxes[0]);
    fireEvent.click(checkboxes[1]);

    fireEvent.click(getByRole("button", { name: /^Update selected/ }));
    const dialog = await findByRole("dialog");
    showCommands(dialog);
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula glib");
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --cask onyx");

    const notices = within(dialog).getAllByText("Some apps ask for your Mac password at this step.");
    expect(notices).toHaveLength(1);
    expect(notices[0].closest("div")?.textContent).toContain("onyx");
    expect(notices[0].closest("div")?.textContent).not.toContain("glib");
  });

  it("shows a warning carried on the plan, such as cargo's compile-locally notice", async () => {
    planWarnings.glib = ["CompilesLocally"];
    const { findAllByRole, findByRole } = renderWithProviders(<UpdatesPage />);

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
    const { findAllByRole, findByRole } = renderWithProviders(<UpdatesPage />);

    fireEvent.click((await findAllByRole("button", { name: "Update" }))[0]);
    const dialog = await findByRole("dialog");
    const line =
      "After installing or updating, Homebrew deletes the older versions of this software and of any it updates along with it, and stray old downloads; when its periodic clean-up is due, those of all Homebrew software.";
    await within(dialog).findByText(line);
    const why =
      "Canager runs Homebrew with HOMEBREW_NO_INSTALL_CLEANUP=1, but your brew.env sets it to nothing, and brew.env wins.";
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
      const { findAllByRole, findByRole } = renderWithProviders(<UpdatesPage />);

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
    const { findAllByRole, findByRole } = renderWithProviders(<UpdatesPage />);

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
    const { queryByRole } = renderWithProviders(<UpdatesPage />);

    await showCantUpdate();
    const myFork = await findRow("my-fork");
    expect(queryByRole("button", { name: "Update" })).not.toBeInTheDocument();
    expect(queryByRole("checkbox")).not.toBeInTheDocument();
    // No version to move to: the version it has.
    expect(within(myFork).getByText("0.1.0")).toBeInTheDocument();
    const detail = chipDetail(myFork, "Can't check");
    expect(within(detail).getByText("Canager can't find its latest version.")).toBeInTheDocument();
    expect(within(detail).getByText("It wasn't installed from crates.io.")).toBeInTheDocument();
  });

  it("keeps an uncheckable candidate out of Update selected even when it was selected earlier", async () => {
    // Hiding the checkbox is not enough on its own. A selection lives in
    // the UI store and outlives the row that made it, so a candidate
    // selected while it was checkable stays selected after a refresh flips
    // `checkable` to false (cargo's crates.io lookup failing is enough to
    // do that). Without this, "Update selected" would still plan the row
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

    const { getByRole, findByRole } = renderWithProviders(<UpdatesPage />);

    await findRow("glib");
    // One, not two: my-fork's selection counts for nothing.
    fireEvent.click(getByRole("button", { name: "Update selected (1)" }));

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
    const { findByText, getAllByRole, getByText } = renderWithProviders(<UpdatesPage />);

    await findRow("onyx");
    // Only onyx's.
    expect(getAllByRole("button", { name: "Update" })).toHaveLength(1);
    expect(getAllByRole("checkbox")).toHaveLength(1);
    expect(getAllByRole("checkbox")[0]).toHaveAccessibleName("Select onyx for update");
    // Counted apart from what can be updated: "1 can be updated", and one under
    // "Can't update here".
    await findByText("1 can be updated");
    await showCantUpdate();
    expect(getByText("Can't update here (1)")).toBeInTheDocument();
    const glib = await findRow("glib");
    // Its version is real, and shown.
    expect(within(glib).getByText("2.88.3 → 2.90.0")).toBeInTheDocument();
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
    renderWithProviders(<UpdatesPage />);

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
    const { queryByText } = renderWithProviders(<UpdatesPage />);

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
    const { queryAllByRole, queryByText } = renderWithProviders(<UpdatesPage />);

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
        "Homebrew isn't responding. Press Check again later.",
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
    const { queryByText } = renderWithProviders(<UpdatesPage />);

    await showCantUpdate();
    const detail = chipDetail(await findRow("glib"), "Pinned");
    expect(within(detail).getByText("/usr/local/bin/brew unpin glib").tagName).toBe("CODE");
    expect(queryByText(/\/opt\/homebrew\/bin\/brew unpin/)).toBeNull();
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
    const { getAllByRole, queryByText } = renderWithProviders(<UpdatesPage />);

    await findRow("glib");
    // Only glib's.
    expect(getAllByRole("button", { name: "Update" })).toHaveLength(1);
    expect(getAllByRole("checkbox")).toHaveLength(1);
    expect(getAllByRole("checkbox")[0]).toHaveAccessibleName("Select glib for update");
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

    const { getByRole, findByRole } = renderWithProviders(<UpdatesPage />);

    await findRow("onyx");
    fireEvent.click(getByRole("button", { name: "Update selected (1)" }));

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
    const { findByText, findAllByRole } = renderWithProviders(<UpdatesPage />);

    // glib's button and checkbox, and only glib's.
    expect(await findAllByRole("button", { name: "Update" })).toHaveLength(1);
    expect(await findAllByRole("checkbox")).toHaveLength(1);
    // One update the user can act on, and one they cannot -- both said out
    // loud. Counting only the first left "0 updates available" above six
    // listed rows on a machine whose only outdated packages were pip's.
    await findByText("1 can be updated");
    await findByText("Can't update here (1)");
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
    const { findByText, queryAllByText, getAllByRole } = renderWithProviders(<UpdatesPage />);

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
    const { queryByText, queryByRole } = renderWithProviders(<UpdatesPage />);

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
    const { findByText, queryByText } = renderWithProviders(<UpdatesPage />);

    await findByText("1 can be updated");
    await findByText("Can't update here (1)");
    expect(queryByText("2 can be updated")).not.toBeInTheDocument();
  });

  it("says why a row can't be checked even when its source is also read-only", async () => {
    // pip is read-only *and* reaches PyPI, so a failed lookup produces
    // rows where both facts are true at once -- and the read-only advice
    // used to win outright, leaving no trace that Canager had not managed
    // to check anything. Both are said, each by its own chip: that no
    // button will ever appear here, and that this check found nothing.
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
    renderWithProviders(<UpdatesPage />);

    await showCantUpdate();
    const urllib3 = await findRow("urllib3");
    const advice = chipDetail(urllib3, "View only");
    expect(advice).toHaveTextContent(/with pipx or uv/);
    expect(advice.textContent).not.toMatch(/latest version/);
    const reason = chipDetail(urllib3, "Can't check");
    expect(reason).toHaveTextContent("Canager can't find its latest version.");
    expect(reason.textContent).not.toMatch(/pipx/);
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
    const { queryAllByText } = renderWithProviders(<UpdatesPage />);

    await showCantUpdate();
    const detail = chipDetail(await findRow("certifi"), "Can't check");
    // What a person who does not write code is told instead: that it could
    // not be checked, in one short sentence...
    expect(detail.textContent).toBe("Canager can't find its latest version.");
    expect(queryAllByText(/Could not fetch URL/)).toHaveLength(0);
    expect(queryAllByText(/pip list --outdated/)).toHaveLength(0);
    // ...and, once for the page, where to go if they want the rest.
    const summary = queryAllByText(/Canager couldn't check 3 of these for updates/);
    expect(summary).toHaveLength(1);
    expect(summary[0].textContent).toContain("Show technical details");
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
    const { findByText, queryAllByText } = renderWithProviders(<UpdatesPage />);

    await findByText("Can't update here (70)");
    await showCantUpdate();
    const summary = queryAllByText(/Canager couldn't check 70 of these for updates/);
    expect(summary).toHaveLength(1);
    expect(summary[0].textContent).toBe(
      "Canager couldn't check 70 of these for updates. To see why, turn on “Show technical details” in Settings.",
    );
    expect(queryAllByText(/ENOTFOUND/)).toHaveLength(0);
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
    const { queryByText } = renderWithProviders(<UpdatesPage />);

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
    const { queryByText } = renderWithProviders(<UpdatesPage />);

    await showCantUpdate();
    const detail = chipDetail(await findRow("urllib3"), "Can't check");
    const lines = [...detail.querySelectorAll("p")].map((line) => line.textContent);
    expect(lines).toEqual([
      "Canager can't find its latest version.",
      "pip list --outdated: ERROR: Could not fetch URL https://pypi.org/simple/",
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
    renderWithProviders(<UpdatesPage />);

    await showCantUpdate();
    const detail = chipDetail(await findRow("my-fork"), "Can't check");
    expect([...detail.querySelectorAll("p")].map((line) => line.textContent)).toEqual([
      "Canager can't find its latest version.",
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
    const { queryAllByRole } = renderWithProviders(<UpdatesPage />);

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
    const { findByText, queryByText } = renderWithProviders(<UpdatesPage />);

    expect(await findByText("Nothing to update here")).toBeInTheDocument();
    expect(queryByText("0 can be updated")).not.toBeInTheDocument();
    // The headline switching to "nothing here" does not excuse dropping the
    // number: two listed rows the user cannot act on are still counted.
    expect(await findByText("Can't update here (2)")).toBeInTheDocument();
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

    const { getByRole, findByRole } = renderWithProviders(<UpdatesPage />);

    await findRow("glib");
    fireEvent.click(getByRole("button", { name: "Update selected (1)" }));

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
    const { queryByText, container } = renderWithProviders(<UpdatesPage />);

    const qwen = await findRow("qwen3:8b");
    expect(within(qwen).getByText("New version")).toBeInTheDocument();
    expect(queryByText(/5642e97495e1a0888838/)).not.toBeInTheDocument();
    expect(container.textContent).not.toMatch(/sha256|→/);
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
    const { container } = renderWithProviders(<UpdatesPage />);

    await showCantUpdate();
    const qwen = await findRow("qwen3:8b");
    expect(within(qwen).queryByText("New version")).toBeNull();
    expect(container.textContent).not.toMatch(/5642e974/);
  });

  it("submits nothing when the confirmation is cancelled", async () => {
    const { findAllByRole, findByRole, queryByRole } = renderWithProviders(<UpdatesPage />);

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
    const { findAllByRole, findByRole, queryByRole } = renderWithProviders(<UpdatesPage />);

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
    const { findAllByRole, getByRole, findByRole, queryByRole } = renderWithProviders(<UpdatesPage />);

    const checkboxes = await findAllByRole("checkbox");
    fireEvent.click(checkboxes[0]);
    fireEvent.click(checkboxes[1]);
    fireEvent.click(getByRole("button", { name: /^Update selected/ }));

    const dialog = await findByRole("dialog");
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "Couldn't prepare the update: glib is pinned",
    );
    // One tool can still be updated: the sheet asks about it by name.
    expect(dialog).toHaveAccessibleName("Update onyx?");
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
    const { findAllByRole, findByRole, queryByRole } = renderWithProviders(<UpdatesPage />);

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
    const { findAllByRole, getByRole, findByRole, queryByRole } = renderWithProviders(<UpdatesPage />);

    const checkboxes = await findAllByRole("checkbox");
    fireEvent.click(checkboxes[0]);
    fireEvent.click(checkboxes[1]);
    fireEvent.click(getByRole("button", { name: /^Update selected/ }));

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
    fireEvent.click(getByRole("button", { name: "Update selected (1)" }));
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
    const { findAllByRole, findByRole, getByRole, queryByRole } = renderWithProviders(<UpdatesPage />);

    // glib is selected so "Update selected" has something to do: the lock
    // below is what disables it, not an empty selection.
    fireEvent.click((await findAllByRole("checkbox"))[0]);
    const updateSelected = getByRole("button", { name: /^Update selected/ });
    const updateAll = getByRole("button", { name: "Update all" });
    const updateButtons = await findAllByRole("button", { name: "Update" });

    // Batch 1 (glib) is still planning when it is closed and batch 2 (onyx)
    // opens the dialog. Planning has no side effect beyond issuing a PlanId
    // that expires on its own, so the newer batch supersedes the older one.
    // The sheet is up from the first press, over the page, so the second
    // press comes after Cancel.
    fireEvent.click(updateButtons[0]);
    const first = await findByRole("dialog", { name: "Update glib?" });
    fireEvent.click(within(first).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(queryByRole("dialog")).not.toBeInTheDocument());
    fireEvent.click(updateButtons[1]);
    const dialog = await findByRole("dialog", { name: "Update onyx?" });
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
    // this one has settled — not Cancel, not Escape, not "Update selected",
    // not "Update all".
    await waitFor(() =>
      expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeDisabled(),
    );
    expect(within(dialog).getByRole("button", { name: "Update" })).toBeDisabled();
    expect(updateSelected).toBeDisabled();
    expect(updateAll).toBeDisabled();
    fireEvent.keyDown(dialog, { key: "Escape" });
    expect(queryByRole("dialog")).toBe(dialog);

    await waitFor(() => expect(releaseSubmit["2"]).toBeDefined());
    releaseSubmit["2"]();
    await waitFor(() => expect(queryByRole("dialog")).not.toBeInTheDocument());
    expect(submittedPlanIds()).toEqual([{ planId: "2" }]);
    expect(updateSelected).not.toBeDisabled();
    expect(updateAll).not.toBeDisabled();
  });

  describe("the page's header", () => {
    it("counts the rows it can update, and Update selected counts the ticked ones", async () => {
      updates = [...snapshot.updates, brewCandidate("jq"), { ...brewCandidate("wget"), blocked: "Pinned" }];
      const { findByText, getByRole, getAllByRole } = renderWithProviders(<UpdatesPage />);

      await findByText("3 can be updated");
      expect(getByRole("button", { name: "Update selected" })).toBeDisabled();
      fireEvent.click(getAllByRole("checkbox")[0]);
      fireEvent.click(getAllByRole("checkbox")[2]);
      expect(getByRole("button", { name: "Update selected (2)" })).toBeEnabled();
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
      const { getByRole, findByRole, queryByRole, findByText } = renderWithProviders(<UpdatesPage />);

      await findByText("3 can be updated");
      fireEvent.click(getByRole("button", { name: "Update all" }));

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
      const { findByText, getByRole, queryByText } = renderWithProviders(<UpdatesPage />);

      expect(await findByText("Updating 2 tools")).toBeInTheDocument();
      expect(queryByText("Nothing to update here")).toBeNull();
      expect(getByRole("button", { name: "Update all" })).toBeDisabled();
    });

    it("says it in Chinese in the Overview's words, and how many more can be updated", async () => {
      await i18n.changeLanguage("zh-CN");
      try {
        updates = [...snapshot.updates, brewCandidate("jq")];
        operations = [operation(glibKey, { id: 7, status: "Running" })];
        const { findByText } = renderWithProviders(<UpdatesPage />);

        expect(await findByText("正在更新1个工具，另有2个可更新")).toBeInTheDocument();
      } finally {
        await i18n.changeLanguage("en");
      }
    });

    it("offers Update all only when there is something it could update", async () => {
      updates = [{ ...snapshot.updates[0], blocked: "Pinned" }];
      const { findByText, getByRole } = renderWithProviders(<UpdatesPage />);

      await findByText("Nothing to update here");
      expect(getByRole("button", { name: "Update all" })).toBeDisabled();
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
      const { findByRole, queryByText } = renderWithProviders(<UpdatesPage />);

      const toggle = await findByRole("button", { name: "Can't update here (2)" });
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

    it("has nothing to fold when every row can be updated", async () => {
      const { queryByRole } = renderWithProviders(<UpdatesPage />);

      await findRow("glib");
      expect(queryByRole("button", { name: /^Can't update here/ })).toBeNull();
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
      renderWithProviders(<UpdatesPage />);

      await showCantUpdate();
      expect(chipDetail(await findRow("qwen3:8b"), "Can't update now")).toHaveTextContent(
        "Ollama isn't running. Open it, then press Check again.",
      );
      expect(chipDetail(rowOf("tokei"), "Can't update now")).toHaveTextContent(
        "Cargo isn't responding. Press Check again later.",
      );
      expect(chipDetail(rowOf("glib"), "Can't update now")).toHaveTextContent(
        "Homebrew doesn't work while Canager is opened as administrator. Quit, then open Canager again with a double-click.",
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
        const { findByRole } = renderWithProviders(<UpdatesPage />);

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
    const { queryByText } = renderWithProviders(<UpdatesPage />);

    chooseFromMenu(await findRow("glib"), "Stop reminding me");

    await waitFor(() => expect(queryByText("glib")).not.toBeInTheDocument());
    expect(savedSettings().ignored_updates).toEqual([glibKey]);
    expect(savedSettings().skipped_versions).toEqual([]);
    expect(queryByText("onyx")).toBeInTheDocument();
  });

  it("hides the row when Skip this version is chosen from its menu, and saves the version it offers", async () => {
    const { queryByText } = renderWithProviders(<UpdatesPage />);

    chooseFromMenu(await findRow("glib"), "Skip this version");

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
    renderWithProviders(<UpdatesPage />);

    chooseFromMenu(await findRow("glib"), "Skip this version");

    await waitFor(() => expect(calls("set_settings")).toHaveLength(1));
    expect(savedSettings().skipped_versions).toEqual([
      { key: onyxKey, version: "5.0.9" },
      { key: glibKey, version: "2.90.0" },
    ]);
  });

  it("lists a skipped package again once its source offers another version", async () => {
    settings.skipped_versions = [{ key: glibKey, version: "2.89.0" }];
    const { findByText, findAllByRole } = renderWithProviders(<UpdatesPage />);

    await findRow("glib");
    await findByText("2 can be updated");
    expect(await findAllByRole("button", { name: "Update" })).toHaveLength(2);
  });

  it("leaves a skipped row out of the headline, Select all and Update selected, even one selected before", async () => {
    settings.skipped_versions = [{ key: glibKey, version: "2.90.0" }];
    act(() => {
      useUiStore.getState().toggleUpdate(glibKey);
    });
    const { findByText, queryByText, getByRole, findByRole } = renderWithProviders(
      <UpdatesPage />,
    );

    await findRow("onyx");
    expect(queryByText("glib")).not.toBeInTheDocument();
    expect(await findByText("1 can be updated")).toBeInTheDocument();
    // glib's selection outlived its row, and counts for nothing.
    expect(getByRole("button", { name: "Update selected" })).toBeDisabled();

    fireEvent.click(getByRole("button", { name: "Select all items that can be updated here" }));
    fireEvent.click(getByRole("button", { name: "Update selected (1)" }));
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
    renderWithProviders(<UpdatesPage />);

    await showCantUpdate();
    const myForkMenu = openMenu(await findRow("my-fork"));
    expect(
      within(myForkMenu).getByRole("menuitem", { name: "Stop reminding me" }),
    ).toBeInTheDocument();
    expect(within(myForkMenu).queryByRole("menuitem", { name: "Skip this version" })).toBeNull();
    fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });

    const glibMenu = openMenu(rowOf("glib"));
    expect(within(glibMenu).getByRole("menuitem", { name: "Skip this version" })).toBeInTheDocument();
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
    renderWithProviders(<UpdatesPage />);

    const chromium = await findRow("chromium");
    expect(within(chromium).getByRole("button", { name: "Update" })).toBeInTheDocument();
    const menu = openMenu(chromium);
    expect(
      within(menu).getByRole("menuitem", { name: "Stop reminding me" }),
    ).toBeInTheDocument();
    expect(within(menu).queryByRole("menuitem", { name: "Skip this version" })).toBeNull();
    fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });

    const glibMenu = openMenu(rowOf("glib"));
    expect(within(glibMenu).getByRole("menuitem", { name: "Skip this version" })).toBeInTheDocument();
  });

  it("lists a Homebrew cask declared version :latest even with a skip of latest in its settings", async () => {
    settings.include_self_updating = true;
    settings.skipped_versions = [{ key: chromiumKey, version: "latest" }];
    updates = [snapshot.updates[0], latestCask];
    const { findByText, findAllByRole } = renderWithProviders(<UpdatesPage />);

    await findRow("chromium");
    await findByText("2 can be updated");
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
    const { findByText, container } = renderWithProviders(<UpdatesPage />);

    chooseFromMenu(await findRow("qwen3:8b"), "Skip this version");

    await findByText(
      "No updates to handle. The rest are hidden.",
    );
    expect(savedSettings().skipped_versions).toEqual([{ key: qwenKey, version: digest }]);
    expect(container.textContent).not.toMatch(/sha256|5642e974/);
  });

  it("says on each menu item what it will do", async () => {
    renderWithProviders(<UpdatesPage />);

    const menu = openMenu(await findRow("glib"));
    expect(within(menu).getByRole("menuitem", { name: "Skip this version" })).toHaveAccessibleDescription(
      "You'll be reminded when the next version comes out.",
    );
    expect(
      within(menu).getByRole("menuitem", { name: "Stop reminding me" }),
    ).toHaveAccessibleDescription(
      "You won't be reminded about any update to this tool. Undo it in Settings.",
    );
  });

  it("names each row's menu after the row", async () => {
    const { getByRole } = renderWithProviders(<UpdatesPage />);

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
    const { findByText, queryByText } = renderWithProviders(<UpdatesPage />);

    chooseFromMenu(await findRow("glib"), "Skip this version");
    await waitFor(() => expect(calls("set_settings")).toHaveLength(1));

    // Every item locks until the first save settles. A second choice now
    // would build its settings from the same stale base, and the later save
    // would drop the earlier one.
    const menu = openMenu(rowOf("onyx"));
    const skip = within(menu).getByRole("menuitem", { name: "Skip this version" });
    const never = within(menu).getByRole("menuitem", { name: "Stop reminding me" });
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
      expect(within(after).getByRole("menuitem", { name: "Skip this version" })).not.toHaveAttribute(
        "aria-disabled",
      ),
    );
    expect(
      within(after).getByRole("menuitem", { name: "Stop reminding me" }),
    ).not.toHaveAttribute("aria-disabled");
    expect(calls("set_settings")).toHaveLength(1);
  });

  it("shows the backend's message when saving the choice fails", async () => {
    saveFailure = "settings.json is read-only";
    const { findByRole } = renderWithProviders(<UpdatesPage />);

    chooseFromMenu(await findRow("glib"), "Skip this version");

    expect(await findByRole("alert")).toHaveTextContent(
      "Couldn't save that choice: settings.json is read-only",
    );
    // Nothing was saved, so nothing disappears.
    expect(rowOf("glib")).toBeInTheDocument();
  });

  describe("Copy command", () => {
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
      const { findByRole } = renderWithProviders(<UpdatesPage />);

      await showCantUpdate();
      chooseFromMenu(await findRow("glib"), "Copy command");

      expect(writeText).toHaveBeenCalledWith("/opt/homebrew/bin/brew unpin glib");
      expect(await findByRole("status")).toHaveTextContent("Copied");
    });

    it("says so when the clipboard refuses", async () => {
      settings.show_technical_details = true;
      writeText.mockRejectedValue(new Error("denied"));
      updates = [{ ...snapshot.updates[0], blocked: "Pinned" }];
      const { findByText } = renderWithProviders(<UpdatesPage />);

      await showCantUpdate();
      chooseFromMenu(await findRow("glib"), "Copy command");

      expect(await findByText("Couldn't copy")).toBeInTheDocument();
    });

    it("is not offered with technical details off, nor where the command would need a preview", async () => {
      updates = [{ ...snapshot.updates[0], blocked: "Pinned" }, snapshot.updates[1]];
      const view = renderWithProviders(<UpdatesPage />);

      await showCantUpdate();
      expect(within(openMenu(await findRow("glib"))).queryByRole("menuitem", { name: "Copy command" })).toBeNull();
      view.unmount();

      // With the switch on, an updatable row's command is only known from
      // its plan, which the menu does not ask for.
      settings.show_technical_details = true;
      renderWithProviders(<UpdatesPage />);
      expect(within(openMenu(await findRow("onyx"))).queryByRole("menuitem", { name: "Copy command" })).toBeNull();
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
      renderWithProviders(<UpdatesPage />);

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
      ["that worked", { status: "Done", outcome: "Succeeded" }, "1 can be updated"],
    ];

    it.each(takesRow)("leaves a row with an update %s out of its checkbox, the count, Select all, Invert and Update all", async (_name, fields, header) => {
      operations = [operation(glibKey, fields)];
      started(7, "2.90.0");
      // Ticked before its update started.
      useUiStore.setState({ selectedUpdates: [artifactKeyId(glibKey)] });
      const { findByText, getByRole } = renderWithProviders(<UpdatesPage />);

      const glib = await findRow("glib");
      expect(await findByText(header)).toBeInTheDocument();
      expect(within(glib).queryByRole("checkbox")).toBeNull();
      expect(within(rowOf("onyx")).getByRole("checkbox")).toBeInTheDocument();
      expect(getByRole("button", { name: "Update selected" })).toBeDisabled();

      fireEvent.click(getByRole("button", { name: "Invert selection among the items that can be updated here" }));
      expect(useUiStore.getState().selectedUpdates).toEqual([artifactKeyId(glibKey), artifactKeyId(onyxKey)]);
      expect(getByRole("button", { name: "Update selected (1)" })).toBeEnabled();

      useUiStore.setState({ selectedUpdates: [] });
      fireEvent.click(getByRole("button", { name: "Select all items that can be updated here" }));
      expect(useUiStore.getState().selectedUpdates).toEqual([artifactKeyId(onyxKey)]);

      fireEvent.click(getByRole("button", { name: "Update all" }));
      await waitFor(() => expect(plannedNames()).toEqual(["onyx"]));
    });

    it("keeps the checkbox of a row whose update failed, for Retry", async () => {
      operations = [
        operation(glibKey, { status: "Done", outcome: { Failed: { exit_code: 1, summary: "Error: no bottle" } } }),
      ];
      started(7, "2.90.0");
      const { findByText, getByRole } = renderWithProviders(<UpdatesPage />);

      const glib = await findRow("glib");
      expect(await within(glib).findByText("Update failed")).toBeInTheDocument();
      expect(await findByText("2 can be updated")).toBeInTheDocument();
      expect(within(glib).getByRole("checkbox")).toBeInTheDocument();
      fireEvent.click(getByRole("button", { name: "Select all items that can be updated here" }));
      expect(useUiStore.getState().selectedUpdates).toEqual([artifactKeyId(glibKey), artifactKeyId(onyxKey)]);
    });

    it("ticks a finished update Updated, while its row still offers the version it was for", async () => {
      operations = [operation(glibKey, { status: "Done", outcome: "Succeeded" })];
      started(7, "2.90.0");
      renderWithProviders(<UpdatesPage />);

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
      renderWithProviders(<UpdatesPage />);

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
      renderWithProviders(<UpdatesPage />);

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
      const { getByRole } = renderWithProviders(<UpdatesPage />);

      const glib = await findRow("glib");
      expect(await within(glib).findByText("Update failed")).toBeInTheDocument();
      fireEvent.click(getByRole("button", { name: "View log: glib" }));
      expect(useUiStore.getState().focusedOpId).toBe(9);
      expect(useUiStore.getState().drawerOpen).toBe(true);
    });

    it("says a cancelled update was cancelled", async () => {
      operations = [operation(glibKey, { status: "Done", outcome: "Cancelled" })];
      started(7, "2.90.0");
      renderWithProviders(<UpdatesPage />);

      expect(await within(await findRow("glib")).findByText("Cancelled")).toBeInTheDocument();
    });

    it.each<[string, OpSummary["outcome"]]>([
      ["could not be confirmed", "Unconfirmed"],
      ["needs attention", { NeedsAttention: "UnchangedAfterUpgrade" }],
    ])("asks to check an update whose result %s, with a way to its log", async (_name, outcome) => {
      operations = [operation(glibKey, { id: 11, status: "Done", outcome })];
      started(11, "2.90.0");
      const { getByRole } = renderWithProviders(<UpdatesPage />);

      expect(await within(await findRow("glib")).findByText("Needs attention")).toBeInTheDocument();
      fireEvent.click(getByRole("button", { name: "View log: glib" }));
      expect(useUiStore.getState().focusedOpId).toBe(11);
    });

    const endings: Array<[string, OpSummary["outcome"], string]> = [
      ["failed", { Failed: { exit_code: 1, summary: "Error: glib: no bottle" } }, "Update failed"],
      ["was cancelled", "Cancelled", "Cancelled"],
      ["asks to be checked", { NeedsAttention: "UnchangedAfterUpgrade" }, "Needs attention"],
    ];

    it.each(endings)(
      "offers Retry where Update was once an update %s, with how it ended still in the row",
      async (_name, outcome, text) => {
        operations = [operation(glibKey, { id: 9, status: "Done", outcome })];
        started(9, "2.90.0");
        renderWithProviders(<UpdatesPage />);

        const glib = await findRow("glib");
        expect(await within(glib).findByText(text)).toBeInTheDocument();
        const retry = within(glib).getByRole("button", { name: "Retry" });
        expect(retry).toHaveAttribute("data-tone", "accent");
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
      const { findByRole, getByRole } = renderWithProviders(<UpdatesPage />);

      const glib = await findRow("glib");
      fireEvent.click(await within(glib).findByRole("button", { name: "Retry" }));
      const dialog = await findByRole("dialog", { name: "Update glib?" });
      await waitFor(() => expect(plannedNames()).toEqual(["glib"]));
      fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));

      fireEvent.click(getByRole("button", { name: "View log: glib" }));
      expect(useUiStore.getState().focusedOpId).toBe(9);
      expect(useUiStore.getState().drawerOpen).toBe(true);
    });

    it("offers no Retry beside a tick", async () => {
      operations = [operation(glibKey, { status: "Done", outcome: "Succeeded" })];
      started(7, "2.90.0");
      renderWithProviders(<UpdatesPage />);

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
      renderWithProviders(<UpdatesPage />);

      await showCantUpdate();
      const glib = await findRow("glib");
      expect(await within(glib).findByText("Update failed")).toBeInTheDocument();
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
      renderWithProviders(<UpdatesPage />);

      expect(await within(await findRow("glib")).findByText("Updating…")).toBeInTheDocument();
      expect(within(rowOf("glib")).queryByText("Cancelled")).toBeNull();
      expect(within(rowOf("onyx")).getByRole("button", { name: "Update" })).toBeInTheDocument();
    });

    it("remembers which version an update it started was for", async () => {
      const { findAllByRole, findByRole } = renderWithProviders(<UpdatesPage />);

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
      const { queryClient } = renderWithProviders(<UpdatesPage />);

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
      renderWithProviders(<UpdatesPage />);

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
      renderWithProviders(<UpdatesPage />);

      expect(await within(await findRow("glib")).findByText("Update failed")).toBeInTheDocument();
      expect(within(rowOf("onyx")).getByText("Needs attention")).toBeInTheDocument();
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
      renderWithProviders(<UpdatesPage />);

      expect(await within(await findRow("onyx")).findByText("Updating…")).toBeInTheDocument();
      expect(justUpdated()).toBeNull();
    });

    it("stays out of the count, Select all and Update all", async () => {
      operations = [operation(glibKey, { status: "Done", outcome: "Succeeded" })];
      started(7, "2.90.0");
      updates = [snapshot.updates[1]];
      artifacts = [installed(glibKey, "2.90.0"), installed(onyxKey, "5.0.2")];
      const { findByText, getByRole, findByRole } = renderWithProviders(<UpdatesPage />);

      const section = await screen.findByRole("region", { name: "Just updated" });
      expect(await findByText("1 can be updated")).toBeInTheDocument();
      expect(within(section).queryByRole("checkbox")).toBeNull();
      expect(within(section).getAllByRole("button").map((button) => button.getAttribute("aria-label"))).toEqual([
        "Clear the Just updated list",
      ]);

      fireEvent.click(getByRole("button", { name: "Select all items that can be updated here" }));
      expect(useUiStore.getState().selectedUpdates).toEqual([artifactKeyId(onyxKey)]);
      expect(getByRole("button", { name: "Update selected (1)" })).toBeEnabled();

      fireEvent.click(getByRole("button", { name: "Update all" }));
      await findByRole("dialog", { name: "Update OnyX?" });
      expect(plannedNames()).toEqual(["onyx"]);
    });

    it("hides itself on Clear, until the next update succeeds", async () => {
      operations = [operation(glibKey, { status: "Done", outcome: "Succeeded" })];
      started(7, "2.90.0");
      updates = [snapshot.updates[1]];
      artifacts = [installed(glibKey, "2.90.0"), installed(onyxKey, "5.0.2")];
      const { queryClient } = renderWithProviders(<UpdatesPage />);

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
      const { container } = renderWithProviders(<UpdatesPage />);

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
    const { findByText, queryByText } = renderWithProviders(<UpdatesPage />);

    await findByText(
      "No updates to handle. The rest are hidden.",
    );
    expect(queryByText("Everything is up to date")).not.toBeInTheDocument();
  });

  it("says everything is up to date only when the backend reports no updates at all", async () => {
    updates = [];
    const { findByText } = renderWithProviders(<UpdatesPage />);

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
    const { findByText, queryByText, getByRole } = renderWithProviders(<UpdatesPage />);

    await findByText("Ollama isn't running");
    expect(await findByText("No updates in the sources Canager could check")).toBeInTheDocument();
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
    const { findByText, queryByText, getByRole } = renderWithProviders(<UpdatesPage />);

    await findByText("Couldn't update Homebrew's software list");
    expect(queryByText("Everything is up to date")).not.toBeInTheDocument();
    fireEvent.click(getByRole("button", { name: "Check again" }));
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
    const { findByText, queryByText } = renderWithProviders(<UpdatesPage />);

    await findByText("Homebrew is updating its software list");
    expect(await findByText("No updates in the sources Canager could check")).toBeInTheDocument();
    expect(queryByText("Everything is up to date")).not.toBeInTheDocument();
  });

  it("does not say everything is up to date when a check failed this round", async () => {
    // Every source still reads as answering, with no note: `refresh` keeps
    // a source whose inventory or update check failed as it was, carries
    // its last rows and candidates forward, and says so only in
    // `errors`. None listed is then no news that there are none.
    updates = [];
    errors = [{ instance_id: "brew:/opt/homebrew", message: "brew outdated exited with code 1" }];
    const { findByText, queryByText } = renderWithProviders(<UpdatesPage />);

    expect(await findByText("No updates in the sources Canager could check")).toBeInTheDocument();
    expect(queryByText("Everything is up to date")).not.toBeInTheDocument();
  });

  describe("source notices", () => {
    it("puts a silent source's notice in one line at the top, above the list, with its button in the line", async () => {
      instances = [...snapshot.instances, stoppedOllama];
      const { findByText, getByRole, queryByText } = renderWithProviders(<UpdatesPage />);

      const notice = await findByText("Ollama isn't running");
      // Outside the scrolling list, above it: in view however long the list.
      expect(slotOf(notice)).toBeNull();
      expect(
        notice.compareDocumentPosition(rowOf("glib")) & Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
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
      const { queryClient } = renderWithProviders(<UpdatesPage />);

      // Ollama's line, with its button, though Homebrew's comes first.
      await screen.findByText("Ollama isn't running");
      expect(screen.getByRole("button", { name: "Open Ollama" })).toBeInTheDocument();
      expect(screen.queryByText("Homebrew is updating its software list")).toBeNull();
      fireEvent.click(screen.getByRole("button", { name: "1 more" }));
      expect(screen.getByText("Homebrew is updating its software list")).toBeInTheDocument();

      // The updates go, and the page says so under the same two lines.
      updates = [];
      await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.snapshot }));
      await screen.findByText("No updates in the sources Canager could check");
      expect(screen.getByText("Homebrew is updating its software list")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Show fewer" })).toBeInTheDocument();

      // One line, then two again: folded.
      instances = [...snapshot.instances, stoppedOllama];
      await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.snapshot }));
      await waitFor(() => expect(screen.queryByText("Homebrew is updating its software list")).toBeNull());
      expect(screen.queryByRole("button", { name: "Show fewer" })).toBeNull();
      instances = [brewUpdating, ...snapshot.instances.slice(1), stoppedOllama];
      await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.snapshot }));
      expect(await screen.findByRole("button", { name: "1 more" })).toHaveAttribute("aria-expanded", "false");
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
      const { findByText, getByRole } = renderWithProviders(<UpdatesPage />);

      await findByText("Ollama isn't responding");
      const details = getByRole("button", { name: "Details: Ollama isn't responding" });
      fireEvent.click(details);
      // Its rows, by name: in a list that mixes sources, "what's listed
      // here" alone would take in Homebrew's fresh rows too.
      expect(
        document.getElementById(details.getAttribute("aria-controls") ?? ""),
      ).toHaveTextContent(
        "What's listed for Ollama is from the last time it responded, and later changes aren't shown. Press Check again later.",
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
          "Ollama isn't responding. Press Check again later.",
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
      const { queryAllByText } = renderWithProviders(<UpdatesPage />);

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
      const withRows = renderWithProviders(<UpdatesPage />);

      const details = await withRows.findByRole("button", {
        name: "Details: Homebrew isn't responding",
      });
      fireEvent.click(details);
      expect(
        document.getElementById(details.getAttribute("aria-controls") ?? ""),
      ).toHaveTextContent(
        "What's listed for Homebrew is from the last time it responded, and later changes aren't shown. Press Check again later.",
      );
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
      const coldStart = renderWithProviders(<UpdatesPage />);

      const coldDetails = await coldStart.findByRole("button", {
        name: "Details: Homebrew isn't responding",
      });
      fireEvent.click(coldDetails);
      const text = document.getElementById(coldDetails.getAttribute("aria-controls") ?? "");
      expect(text).toHaveTextContent(
        "Homebrew didn't respond, so Canager can't show what it has installed.",
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
    const { findByText, findAllByRole } = renderWithProviders(<UpdatesPage />);

    expect(await findByText("2 can be updated")).toBeInTheDocument();
    expect(await findByText("Can't update here (1)")).toBeInTheDocument();
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

    const { findByText, container } = renderWithProviders(<UpdatesPage />);

    // The list still knows how long it is.
    expect(await findByText("Can't update here (400)")).toBeInTheDocument();
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

    const { findAllByRole, findByRole } = renderWithProviders(<UpdatesPage />);

    fireEvent.click((await findAllByRole("button", { name: "Update" }))[0]);
    const dialog = await findByRole("dialog");

    expect(await within(dialog).findByText("2.88.3 → 2.90.0")).toBeInTheDocument();
  });

  describe("the confirmation sheet", () => {
    it("asks about one tool by its name, and about several by how many, each with its avatar and new version", async () => {
      const { findAllByRole, findByRole, getByRole, queryByRole } = renderWithProviders(<UpdatesPage />);

      fireEvent.click((await findAllByRole("button", { name: "Update" }))[0]);
      let dialog = await findByRole("dialog", { name: "Update glib?" });
      fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
      await waitFor(() => expect(queryByRole("dialog")).toBeNull());

      const checkboxes = await findAllByRole("checkbox");
      fireEvent.click(checkboxes[0]);
      fireEvent.click(checkboxes[1]);
      fireEvent.click(getByRole("button", { name: /^Update selected/ }));
      dialog = await findByRole("dialog", { name: "Update 2 tools?" });
      const tools = [...dialog.querySelectorAll("[data-sheet-tool]")] as HTMLElement[];
      expect(tools.map((tool) => within(tool).getAllByText(/./, { selector: "p" })[0].textContent)).toEqual([
        "glib",
        "onyx",
      ]);
      // The row's own avatar, its source under its name, the version it moves to.
      expect(within(tools[0]).getByText("H")).toHaveAttribute("aria-hidden", "true");
      expect(within(tools[0]).getByText("Homebrew")).toBeInTheDocument();
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
      const { findByRole } = renderWithProviders(<UpdatesPage />, { toolIcons });
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
      expectLogos((await findByRole("dialog", { name: "Update glib?" })).querySelector("[data-sheet-tool]"));
    });

    it("keeps the commands one press away while Show technical details is off", async () => {
      const { findAllByRole, getByRole, findByRole } = renderWithProviders(<UpdatesPage />);

      const checkboxes = await findAllByRole("checkbox");
      fireEvent.click(checkboxes[0]);
      fireEvent.click(checkboxes[1]);
      fireEvent.click(getByRole("button", { name: /^Update selected/ }));
      const dialog = await findByRole("dialog");

      const disclosure = within(dialog).getByRole("button", { name: "Show the commands" });
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
      const { findAllByRole, findByRole } = renderWithProviders(<UpdatesPage />);

      fireEvent.click((await findAllByRole("button", { name: "Update" }))[0]);
      const dialog = await findByRole("dialog");

      expect(await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula glib")).toBeInTheDocument();
      expect(within(dialog).getByRole("button", { name: "Show the command" })).toHaveAttribute(
        "aria-expanded",
        "true",
      );
    });

    it("keeps each tool's notes under its name, under Before you continue", async () => {
      needsPassword.add("onyx");
      noCancel.add("glib");
      planWarnings.glib = ["CompilesLocally"];
      const { findAllByRole, getByRole, findByRole } = renderWithProviders(<UpdatesPage />);

      const checkboxes = await findAllByRole("checkbox");
      fireEvent.click(checkboxes[0]);
      fireEvent.click(checkboxes[1]);
      fireEvent.click(getByRole("button", { name: /^Update selected/ }));
      const dialog = await findByRole("dialog");

      const notes = within(dialog).getByRole("region", { name: "Before you continue" });
      const groups = [...notes.querySelectorAll("ul")].map((list) => ({
        tool: list.previousElementSibling?.textContent,
        lines: [...list.querySelectorAll("li")].map((item) => item.textContent?.trim()),
      }));
      expect(groups).toEqual([
        {
          tool: "glib",
          lines: [
            "This compiles on your Mac and takes a while.",
            "You can't cancel this once it starts. Keep Canager and your Mac on until it finishes.",
          ],
        },
        { tool: "onyx", lines: ["Some apps ask for your Mac password at this step."] },
      ]);
    });

    it("puts Before you continue above the tools, with how many notes beside Update, which takes the focus to them", async () => {
      needsPassword.add("onyx");
      planWarnings.glib = ["CompilesLocally"];
      const { findByRole, findByText, getByRole } = renderWithProviders(<UpdatesPage />);

      await findByText("2 can be updated");
      fireEvent.click(getByRole("button", { name: "Update all" }));
      const dialog = await findByRole("dialog", { name: "Update 2 tools?" });

      const notes = await within(dialog).findByRole("region", { name: "Before you continue" });
      const firstTool = dialog.querySelector("[data-sheet-tool]");
      expect(firstTool).toBeInstanceOf(HTMLElement);
      expect(notes.compareDocumentPosition(firstTool as Node) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();

      const summary = within(dialog).getByRole("button", { name: "2 things to note" });
      fireEvent.click(summary);
      expect(document.activeElement).toBe(notes);
    });

    it("says nothing about notes beside Update when there are none", async () => {
      const { findByRole, findByText, getByRole } = renderWithProviders(<UpdatesPage />);

      await findByText("2 can be updated");
      fireEvent.click(getByRole("button", { name: "Update all" }));
      const dialog = await findByRole("dialog", { name: "Update 2 tools?" });

      await waitFor(() => expect(within(dialog).getByRole("button", { name: "Update" })).toBeEnabled());
      expect(within(dialog).queryByRole("region", { name: "Before you continue" })).toBeNull();
      expect(within(dialog).queryByRole("button", { name: /to note$/ })).toBeNull();
    });

    it("lists the first five tools of a long batch, and the rest one press away", async () => {
      updates = Array.from({ length: 10 }, (_, i) => brewCandidate(`tool-${i}`));
      const { findByRole, findByText, getByRole } = renderWithProviders(<UpdatesPage />);

      await findByText("10 can be updated");
      fireEvent.click(getByRole("button", { name: "Update all" }));
      const dialog = await findByRole("dialog", { name: "Update 10 tools?" });
      const toolNames = () =>
        [...dialog.querySelectorAll("[data-sheet-tool]")].map(
          (tool) => within(tool as HTMLElement).getAllByText(/./, { selector: "p" })[0].textContent,
        );

      expect(toolNames()).toEqual(["tool-0", "tool-1", "tool-2", "tool-3", "tool-4"]);
      const more = within(dialog).getByRole("button", { name: "5 more" });
      expect(more).toHaveAttribute("aria-expanded", "false");

      fireEvent.click(more);
      expect(toolNames()).toHaveLength(10);
      const fewer = within(dialog).getByRole("button", { name: "Show fewer" });
      expect(fewer).toHaveAttribute("aria-expanded", "true");

      fireEvent.click(fewer);
      expect(toolNames()).toHaveLength(5);
    });

    it("lists every tool of a long batch when one of them was refused, with its why", async () => {
      updates = Array.from({ length: 10 }, (_, i) => brewCandidate(`tool-${i}`));
      planFailures["tool-8"] = "tool-8 is pinned";
      const { findByRole, findByText, getByRole } = renderWithProviders(<UpdatesPage />);

      await findByText("10 can be updated");
      fireEvent.click(getByRole("button", { name: "Update all" }));
      const dialog = await findByRole("dialog", { name: "Update 9 tools?" });

      expect(dialog.querySelectorAll("[data-sheet-tool]")).toHaveLength(10);
      expect(within(dialog).getByRole("alert")).toHaveTextContent("Couldn't prepare the update: tool-8 is pinned");
      expect(within(dialog).queryByRole("button", { name: /more$/ })).toBeNull();
    });

    it("keeps a row's Update the accent: the thing this page recommends", async () => {
      renderWithProviders(<UpdatesPage />);

      const update = within(await findRow("glib")).getByRole("button", { name: "Update" });
      expect(update).toHaveAttribute("data-tone", "accent");
    });

    it("puts the focus on Update as it opens, and gives it back to the row's Update when cancelled", async () => {
      const { findAllByRole, findByRole, queryByRole } = renderWithProviders(<UpdatesPage />);

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
      const { getByRole, findByRole, queryByRole, findByText } = renderWithProviders(<UpdatesPage />);

      await findByText("2 can be updated");
      const updateAll = getByRole("button", { name: "Update all" });
      fireEvent.click(updateAll);
      const dialog = await findByRole("dialog");
      fireEvent.click(within(dialog).getByRole("button", { name: "Update" }));

      await waitFor(() => expect(submittedPlanIds()).toHaveLength(2));
      await waitFor(() => expect(queryByRole("dialog")).toBeNull());
      await waitFor(() => expect(document.activeElement).toBe(updateAll));
    });

    it("puts the focus on Close when a batch did not all start, and back on Update all after it", async () => {
      submitFailures["2"] = '{"kind":"expired"}';
      const { getByRole, findByRole, queryByRole, findByText } = renderWithProviders(<UpdatesPage />);

      await findByText("2 can be updated");
      const updateAll = getByRole("button", { name: "Update all" });
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
      const { findAllByRole, getByRole, findByRole, queryByRole } = renderWithProviders(<UpdatesPage />);

      fireEvent.click((await findAllByRole("checkbox"))[0]);
      const updateSelected = getByRole("button", { name: /^Update selected/ });
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
      const { findAllByRole, getByRole, findByRole } = renderWithProviders(<UpdatesPage />);

      const checkboxes = await findAllByRole("checkbox");
      fireEvent.click(checkboxes[0]);
      fireEvent.click(checkboxes[1]);
      fireEvent.click(getByRole("button", { name: "Update selected (2)" }));

      // No plan is back yet: the tools, with their avatars and versions,
      // are the rows'.
      const dialog = await findByRole("dialog", { name: "Update 2 tools?" });
      expect(plannedNames().sort()).toEqual(["glib", "onyx"]);
      const tools = [...dialog.querySelectorAll("[data-sheet-tool]")] as HTMLElement[];
      expect(tools.map((tool) => within(tool).getAllByText(/./, { selector: "p" })[0].textContent)).toEqual([
        "glib",
        "onyx",
      ]);
      expect(within(tools[0]).getByText("H")).toHaveAttribute("aria-hidden", "true");
      expect(within(tools[0]).getByText("2.88.3 → 2.90.0")).toBeInTheDocument();
      expect(within(tools[1]).getByText("5.0.2 → 5.1.0")).toBeInTheDocument();
      // Preparing, where the notes and the commands will go, and Update off.
      expect(within(dialog).getByText("Preparing…")).toBeInTheDocument();
      expect(within(dialog).queryByRole("region", { name: "Before you continue" })).toBeNull();
      expect(within(dialog).queryByRole("button", { name: /^Show the command/ })).toBeNull();
      const update = within(dialog).getByRole("button", { name: "Update" });
      expect(update).toBeDisabled();
      expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeEnabled();

      // One plan back is not the batch: still preparing, still off.
      await release("glib");
      expect(within(dialog).getByText("Preparing…")).toBeInTheDocument();
      expect(update).toBeDisabled();
      expect(within(dialog).queryByRole("region", { name: "Before you continue" })).toBeNull();

      // Every plan back: the notes and the commands, and Update on.
      await release("onyx");
      await waitFor(() => expect(update).toBeEnabled());
      expect(within(dialog).queryByText("Preparing…")).toBeNull();
      expect(within(dialog).getByRole("region", { name: "Before you continue" })).toHaveTextContent(
        "Some apps ask for your Mac password at this step.",
      );
      showCommands(dialog);
      expect(within(dialog).getByText("/opt/homebrew/bin/brew upgrade --formula glib")).toBeInTheDocument();
      expect(within(dialog).getByText("/opt/homebrew/bin/brew upgrade --cask onyx")).toBeInTheDocument();
      expect(submittedPlanIds()).toEqual([]);

      fireEvent.click(update);
      await waitFor(() => expect(submittedPlanIds()).toEqual([{ planId: "1" }, { planId: "2" }]));
    });

    it("is up at once for a row's own Update too, holding the focus itself until Update can take it", async () => {
      holdPlans.add("glib");
      const { findByRole } = renderWithProviders(<UpdatesPage />);

      const rowUpdate = within(await findRow("glib")).getByRole("button", { name: "Update" });
      fireEvent.click(rowUpdate);
      const dialog = await findByRole("dialog", { name: "Update glib?" });
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
      const { findByRole } = renderWithProviders(<UpdatesPage />);

      fireEvent.click(within(await findRow("glib")).getByRole("button", { name: "Update" }));
      const dialog = await findByRole("dialog", { name: "Update glib?" });
      const cancel = within(dialog).getByRole("button", { name: "Cancel" });
      cancel.focus();

      await release("glib");
      await waitFor(() => expect(within(dialog).getByRole("button", { name: "Update" })).toBeEnabled());
      // A key meant for Cancel is never taken by Update.
      expect(document.activeElement).toBe(cancel);
    });

    it("stays shut when the plans of a sheet closed while preparing arrive, and says nothing on the page", async () => {
      holdPlans.add("glib");
      const { findByRole, queryByRole } = renderWithProviders(<UpdatesPage />);

      const rowUpdate = within(await findRow("glib")).getByRole("button", { name: "Update" });
      fireEvent.click(rowUpdate);
      const dialog = await findByRole("dialog", { name: "Update glib?" });
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
      const { findByRole, queryByRole } = renderWithProviders(<UpdatesPage />);

      const rowUpdate = within(await findRow("glib")).getByRole("button", { name: "Update" });
      fireEvent.click(rowUpdate);
      const dialog = await findByRole("dialog", { name: "Update glib?" });
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

    const { findAllByRole, findByRole } = renderWithProviders(<UpdatesPage />);

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
    const { getAllByRole } = renderWithProviders(<UpdatesPage />);

    const claude = await findRow("Claude Code");
    expect(within(claude).getByText("2.1.281 → 2.1.290")).toBeInTheDocument();
    // A plain label: its old ⓘ only said the label over again (polish-3
    // copy table, updates.selfUpdatingDetail).
    expect(within(claude).getByText("Usually updates itself")).toBeInTheDocument();
    expect(within(claude).queryByRole("button", { name: "Usually updates itself" })).toBeNull();
    expect(getAllByRole("button", { name: "Update" })).toHaveLength(1);
    expect(within(claude).getByRole("button", { name: "Update" })).toBeInTheDocument();
  });

  const claudeEndings: Array<[string, OpSummary["outcome"], string, boolean]> = [
    ["failed", { Failed: { exit_code: 1, summary: "Error: download failed" } }, "Update failed", true],
    ["was cancelled", "Cancelled", "Cancelled", false],
    ["asks to be checked", { NeedsAttention: "UnchangedAfterUpgrade" }, "Needs attention", true],
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
      const { queryClient } = renderWithProviders(<UpdatesPage />);

      const claude = await findRow("Claude Code");
      const chips = claude.querySelector<HTMLElement>("[data-status]");
      if (chips === null) throw new Error("the row has no chips' column");
      expect(await within(chips).findByText(words)).toBeInTheDocument();
      expect(within(chips).queryByRole("button", { name: "View log: Claude Code" }) !== null).toBe(logged);
      expect(within(claude).queryByText("Usually updates itself")).toBeNull();
      expect(within(claude).getByRole("button", { name: "Retry" })).toBeInTheDocument();

      // Retried: the update under way stands where the button was, and
      // the chip is back.
      operations = [operation(claudeKey, { id: 10, status: "Running" }), ...operations];
      await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.operations }));
      expect(await within(rowOf("Claude Code")).findByText("Updating…")).toBeInTheDocument();
      expect(within(rowOf("Claude Code")).getByText("Usually updates itself")).toBeInTheDocument();
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
    const { queryClient } = renderWithProviders(<UpdatesPage />);

    const claude = await findRow("Claude Code");
    expect(await within(claude).findByText("Update failed")).toBeInTheDocument();
    expect(within(claude).queryByText("Usually updates itself")).toBeNull();

    // "Update failed" was about 2.1.290; 2.1.291 gets the button, and the chip, back.
    updates = [{ ...claudeUpdate, target: "2.1.291" }];
    await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.snapshot }));
    await waitFor(() => expect(within(rowOf("Claude Code")).queryByText("Update failed")).toBeNull());
    expect(within(rowOf("Claude Code")).getByText("Usually updates itself")).toBeInTheDocument();
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
    renderWithProviders(<UpdatesPage />);

    const onyx = await findRow("OnyX");
    expect(within(onyx).getByText("Verify system files structure")).toBeInTheDocument();
    expect(within(onyx).queryByText("Usually updates itself")).toBeNull();
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
    const { queryByText, getAllByRole } = renderWithProviders(<UpdatesPage />);

    const grok = await findRow("Grok Build");
    expect(within(grok).getByText("xAI's AI coding assistant")).toBeInTheDocument();
    expect(queryByText("No description")).toBeNull();
    expect(within(grok).queryByText("Usually updates itself")).toBeNull();
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
    const { queryByText } = renderWithProviders(<UpdatesPage />);

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
    renderWithProviders(<UpdatesPage />, { toolDescriptions: { "zh-CN": toolDescriptions } });

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
    renderWithProviders(<UpdatesPage />, { toolDescriptions });

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
    const { queryAllByRole } = renderWithProviders(<UpdatesPage />);

    await showCantUpdate();
    const claude = await findRow("Claude Code");
    expect(chipDetail(claude, "Can't check").textContent).toBe("Canager can't find its latest version.");
    expect(within(claude).queryByText("Usually updates itself")).toBeNull();
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
    const { queryAllByRole } = renderWithProviders(<UpdatesPage />);

    await showCantUpdate();
    const claude = await findRow("Claude Code");
    expect(within(claude).queryByText("Usually updates itself")).toBeNull();
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
    const { findByText, getAllByRole, queryByText } = renderWithProviders(<UpdatesPage />);

    await findRow("onyx");
    // Only onyx's.
    expect(getAllByRole("button", { name: "Update" })).toHaveLength(1);
    expect(getAllByRole("checkbox")).toHaveLength(1);
    expect(getAllByRole("checkbox")[0]).toHaveAccessibleName("Select onyx for update");
    await findByText("1 can be updated");
    await findByText("Can't update here (1)");
    await showCantUpdate();
    const claude = await findRow("Claude Code");
    expect(within(claude).getByText("2.1.281 → 2.1.290")).toBeInTheDocument();
    const detail = chipDetail(claude, "Only updates itself");
    expect(detail.textContent).toBe("It updates itself: open it once and it checks for a new version.");
    expect(queryByText(/\.local\/bin\/claude/)).toBeNull();
    // Not the updatable row's chip: this row has no button to point at.
    expect(queryByText(/also update it now/)).toBeNull();
  });

  it("shows the command that opens a tool that updates itself, and offers to copy it, with technical details on", async () => {
    settings.show_technical_details = true;
    instances = [...snapshot.instances, claudeInstance];
    updates = [{ ...claudeUpdate, blocked: "SelfUpdatesOnly" }];
    artifacts = [claudeArtifact];
    renderWithProviders(<UpdatesPage />);

    await showCantUpdate();
    const claude = await findRow("Claude Code");
    const detail = chipDetail(claude, "Only updates itself");
    expect(
      within(detail).getByText(wholeSentence("In Terminal: /Users/someone/.local/bin/claude")),
    ).toBeInTheDocument();
    expect(within(detail).getByText("/Users/someone/.local/bin/claude").tagName).toBe("CODE");
    fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });
    expect(
      within(openMenu(claude)).getByRole("menuitem", { name: "Copy command" }),
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
      const { findByText, queryByText, getAllByRole } = renderWithProviders(<UpdatesPage />);

      expect(await findByText(noticeTitle)).toBeInTheDocument();
      const claude = await findRow("Claude Code");
      expect(within(claude).queryByText("Usually updates itself")).toBeNull();
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
      const { findByText, queryByText } = renderWithProviders(<UpdatesPage />);

      expect(await findByText(noticeTitle)).toBeInTheDocument();
      expect(await findByText("Everything is up to date")).toBeInTheDocument();
      expect(queryByText("No updates in the sources Canager could check")).toBeNull();
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
    const { findByText, queryByText } = renderWithProviders(<UpdatesPage />);

    expect(await findByText("Claude Code's program files are missing")).toBeInTheDocument();
    expect(await findByText("No updates in the sources Canager could check")).toBeInTheDocument();
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
    expect(en.updates.selfUpdating).toBe("Usually updates itself");
    expect(en.updates.blocked.SelfUpdatesOnly.badge).toBe("Only updates itself");
  });

  it("says per item, under its command, that a NoCancel update cannot be stopped once it starts", async () => {
    // A batch can mix a rustup self update (NoCancel) with a Homebrew
    // upgrade (cancellable); the sentence belongs next to the command it
    // is true of. This page is one of the two readers spec §五 gives
    // operations.noCancelHint; UninstallDialog is the other.
    noCancel.add("onyx");
    const { findAllByRole, getByRole, findByRole } = renderWithProviders(<UpdatesPage />);

    const checkboxes = await findAllByRole("checkbox");
    fireEvent.click(checkboxes[0]);
    fireEvent.click(checkboxes[1]);

    fireEvent.click(getByRole("button", { name: /^Update selected/ }));
    const dialog = await findByRole("dialog");
    showCommands(dialog);
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula glib");
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --cask onyx");

    const hints = within(dialog).getAllByText(
      "You can't cancel this once it starts. Keep Canager and your Mac on until it finishes.",
    );
    expect(hints).toHaveLength(1);
    expect(hints[0].closest("div")?.textContent).toContain("onyx");
    expect(hints[0].closest("div")?.textContent).not.toContain("glib");
  });

  describe("Select all and Invert selection", () => {
    // Each button's accessible name starts with the words on it ("Select
    // all", "Invert") and goes on to say which rows it acts on.
    const SELECT_ALL = "Select all items that can be updated here";
    const INVERT = "Invert selection among the items that can be updated here";

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
    function selectedIds(): string[] {
      return [...useUiStore.getState().selectedUpdates].sort();
    }

    function ids(...keys: ArtifactKey[]): string[] {
      return keys.map(artifactKeyId).sort();
    }

    function sortedPlannedNames(): string[] {
      return [...plannedNames()].sort();
    }

    it("Select all ticks every row that has a checkbox and nothing else, and Update selected plans exactly those", async () => {
      listEveryKindOfRow();
      const { findAllByRole, getByRole, findByRole, queryAllByRole } = renderWithProviders(<UpdatesPage />);

      // Unfolded, so every listed row is on the page: still three boxes.
      await showCantUpdate();
      await findRow("my-fork");
      const checkboxes = await findAllByRole("checkbox");
      expect(checkboxes).toHaveLength(3);
      expect(queryAllByRole("checkbox")).toHaveLength(3);
      const selectAll = getByRole("button", { name: SELECT_ALL });
      expect(selectAll.textContent).toBe("Select all");
      expect(getByRole("button", { name: INVERT }).textContent).toBe("Invert");
      expect(getByRole("button", { name: "Update selected" })).toBeDisabled();

      fireEvent.click(selectAll);

      for (const checkbox of checkboxes) expect(checkbox).toBeChecked();
      expect(selectedIds()).toEqual(ids(glibKey, onyxKey, jq.key));

      fireEvent.click(getByRole("button", { name: "Update selected (3)" }));
      await findByRole("dialog");
      expect(sortedPlannedNames()).toEqual(["glib", "jq", "onyx"]);
    });

    it("Invert selection unticks the ticked rows that have a checkbox and ticks the unticked ones", async () => {
      listEveryKindOfRow();
      const { findByRole, getByRole } = renderWithProviders(<UpdatesPage />);

      const glib = await findByRole("checkbox", { name: "Select glib for update" });
      const onyx = getByRole("checkbox", { name: "Select onyx for update" });
      const jqBox = getByRole("checkbox", { name: "Select jq for update" });
      fireEvent.click(glib);

      fireEvent.click(getByRole("button", { name: INVERT }));
      expect(glib).not.toBeChecked();
      expect(onyx).toBeChecked();
      expect(jqBox).toBeChecked();
      expect(selectedIds()).toEqual(ids(onyxKey, jq.key));

      fireEvent.click(getByRole("button", { name: INVERT }));
      expect(glib).toBeChecked();
      expect(onyx).not.toBeChecked();
      expect(jqBox).not.toBeChecked();
      expect(selectedIds()).toEqual(ids(glibKey));

      // Inverting a full selection empties it, and Update selected follows.
      fireEvent.click(getByRole("button", { name: SELECT_ALL }));
      expect(getByRole("button", { name: "Update selected (3)" })).toBeEnabled();
      fireEvent.click(getByRole("button", { name: INVERT }));
      expect(selectedIds()).toEqual([]);
      expect(getByRole("button", { name: "Update selected" })).toBeDisabled();
    });

    it("never selects a row the page does not list, and leaves a pinned row's earlier selection as it was", async () => {
      // wget was selected while it could still be updated; a refresh since
      // says it is pinned, so it has no checkbox. Neither button may take it
      // out of the selection or put tree (never reminded about) or curl
      // (skipped) into it: they act on the rows with a checkbox and nothing
      // else. Update selected still leaves wget out of the batch
      // (`isActionable`).
      listEveryKindOfRow();
      act(() => {
        useUiStore.getState().toggleUpdate(wget.key);
      });
      const { findAllByRole, getByRole, findByRole } = renderWithProviders(<UpdatesPage />);
      await findAllByRole("checkbox");

      fireEvent.click(getByRole("button", { name: SELECT_ALL }));
      expect(selectedIds()).toEqual(ids(wget.key, glibKey, onyxKey, jq.key));

      fireEvent.click(getByRole("button", { name: INVERT }));
      expect(selectedIds()).toEqual(ids(wget.key));
      // A row with no checkbox is all that is left selected: nothing to update.
      expect(getByRole("button", { name: "Update selected" })).toBeDisabled();

      fireEvent.click(getByRole("button", { name: INVERT }));
      expect(selectedIds()).toEqual(ids(wget.key, glibKey, onyxKey, jq.key));

      fireEvent.click(getByRole("button", { name: "Update selected (3)" }));
      await findByRole("dialog");
      expect(sortedPlannedNames()).toEqual(["glib", "jq", "onyx"]);
    });

    it("disables both when no listed row has a checkbox", async () => {
      // tree and curl could be updated but are hidden, so they are not
      // listed; every row that is listed is one Canager cannot update.
      listEveryKindOfRow();
      const withCheckbox = ids(glibKey, onyxKey, jq.key);
      updates = updates.filter((u) => !withCheckbox.includes(artifactKeyId(u.key)));
      const { findByText, getByRole, queryByRole } = renderWithProviders(<UpdatesPage />);

      await findByText("Nothing to update here");
      await showCantUpdate();
      await findRow("my-fork");
      expect(queryByRole("checkbox")).toBeNull();
      const selectAll = getByRole("button", { name: SELECT_ALL });
      const invert = getByRole("button", { name: INVERT });
      expect(selectAll).toBeDisabled();
      expect(invert).toBeDisabled();

      fireEvent.click(selectAll);
      fireEvent.click(invert);
      expect(useUiStore.getState().selectedUpdates).toEqual([]);
    });

    it("calls them 全选 and 反选 in Chinese, as they were asked for", () => {
      expect(zhCN.updates.selectAll).toBe("全选");
      expect(zhCN.updates.invertSelection).toBe("反选");
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
      if (this.textContent?.startsWith("Can't update here")) return 40;
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
    const { getByText, queryByText, queryClient } = renderWithProviders(<UpdatesPage />);
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
    expect(slotTop(getByText("Can't update here (1)"))).toBe("translateY(0px)");
    expect(slotTop(rowOf("urllib3"))).toBe("translateY(40px)");
  });
});
