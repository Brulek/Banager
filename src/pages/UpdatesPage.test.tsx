import { afterEach, describe, expect, it, vi, beforeEach } from "vitest";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { UpdatesPage } from "./UpdatesPage";
import { artifactKeyId, useUiStore } from "../store/ui";
import { queryKeys } from "../lib/queries";
import i18n from "../i18n";
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
  };
  updates = snapshot.updates;
  instances = snapshot.instances;
  artifacts = snapshot.artifacts;
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
      return Promise.resolve({ ...snapshot, updates, instances, artifacts });
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
      if (failure !== undefined) return Promise.reject(failure);
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
    // "No description" only where there truly is none.
    expect(within(rowOf("glib")).getByText("No description")).toBeInTheDocument();
    const claude = rowOf("Claude Code");
    expect(within(claude).getAllByText("Claude Code")).toHaveLength(1);
    expect(rowNames()).toEqual(["Claude Code", "glib", "OnyX"]);
  });

  it("previews the command, submits nothing until Confirm, then submits the single update", async () => {
    const { findAllByRole, findByRole, queryByRole } = renderWithProviders(<UpdatesPage />);

    const updateButtons = await findAllByRole("button", { name: "Update" });
    fireEvent.click(updateButtons[0]);

    const dialog = await findByRole("dialog");
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

    fireEvent.click(within(dialog).getByRole("button", { name: "Confirm" }));

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
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula glib");
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --cask onyx");
    expect(submittedPlanIds()).toEqual([]);

    fireEvent.click(within(dialog).getByRole("button", { name: "Confirm" }));

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
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula glib");
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --cask onyx");

    const notices = within(dialog).getAllByText("This will ask for your Mac password.");
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
      "This will compile locally and can take several minutes.",
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
    // Counted apart from what can be updated: "1 update", and one under
    // "Can't update here".
    await findByText("1 update");
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
      within(chipDetail(rowOf("onyx"), "Unavailable")).getByText(
        "Homebrew isn't responding. Check again later.",
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
    await findByText("1 update");
    await findByText("Can't update here (1)");
    await showCantUpdate();
    const detail = chipDetail(await findRow("urllib3"), "Read-only");
    expect(detail).toHaveTextContent(
      "Packages installed with pip can only be viewed here. For command-line tools, use pipx or uv.",
    );
  });

  it("marks each pip row Read-only in a word, keeps the why behind the chip, and leaves every row its own description", async () => {
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
    expect(getAllByRole("button", { name: "Read-only" })).toHaveLength(6);
    expect(queryAllByText(/use pipx or uv/)).toHaveLength(0);
    // And each row can be told from the next again.
    for (const name of pipPackages) {
      expect(await findByText(`what ${name} is for`)).toBeInTheDocument();
    }
    chipDetail(rowOf("idna"), "Read-only");
    expect(queryAllByText(/use pipx or uv/)).toHaveLength(1);
  });

  it("says nothing about a read-only source that has no rows on this page", async () => {
    // pip being read-only is not news on a page listing two Homebrew
    // updates.
    const { queryByText, queryByRole } = renderWithProviders(<UpdatesPage />);

    await findRow("glib");
    expect(queryByText("Read-only: pip packages")).not.toBeInTheDocument();
    expect(queryByText("Read-only: npm packages")).not.toBeInTheDocument();
    expect(queryByRole("button", { name: "Read-only" })).not.toBeInTheDocument();
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

    await findByText("1 update");
    await findByText("Can't update here (1)");
    expect(queryByText("2 updates")).not.toBeInTheDocument();
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
    const advice = chipDetail(urllib3, "Read-only");
    expect(advice).toHaveTextContent(/use pipx or uv/);
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
    const npm = chipDetail(await findRow("typescript"), "Read-only");
    expect(npm).toHaveTextContent(
      "These can only be viewed here: npm keeps them in a folder your account can't change. Install Node with Homebrew so Canager can manage npm packages.",
    );
    expect(npm.textContent).not.toMatch(/pipx|uv/);
    // pip's row keeps pip's advice, right next to it.
    const pip = chipDetail(rowOf("urllib3"), "Read-only");
    expect(pip).toHaveTextContent(/use pipx or uv/);
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
    expect(queryByText("0 updates")).not.toBeInTheDocument();
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
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula glib");
    fireEvent.click(within(dialog).getByRole("button", { name: "Confirm" }));

    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "Could not start the update: This preview is more than 10 minutes old, so Canager didn't start it. Look at the preview again, then confirm it once more.",
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
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula glib");
    expect(calls("plan_operation")).toHaveLength(2);
    expect(submittedPlanIds()).toEqual([{ planId: "1" }]);

    fireEvent.click(within(dialog).getByRole("button", { name: "Confirm" }));

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
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --cask onyx");
    expect(
      within(dialog).queryByText("/opt/homebrew/bin/brew upgrade --formula glib"),
    ).not.toBeInTheDocument();

    fireEvent.click(within(dialog).getByRole("button", { name: "Confirm" }));

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
      "Couldn't prepare the update: Start Homebrew and Canager will list",
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
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --cask onyx");
    fireEvent.click(within(dialog).getByRole("button", { name: "Confirm" }));

    // glib started, onyx did not, and the dialog says which is which.
    await within(dialog).findByText("Started");
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "Could not start the update: This preview is more than 10 minutes old, so Canager didn't start it. Look at the preview again, then confirm it once more.",
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

    fireEvent.click(within(dialog).getByRole("button", { name: "Confirm" }));

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

    // Batch 1 (glib) is still planning when batch 2 (onyx) opens the dialog.
    // Planning has no side effect beyond issuing a PlanId that expires on
    // its own, so the newer click supersedes the older batch.
    fireEvent.click(updateButtons[0]);
    fireEvent.click(updateButtons[1]);
    const dialog = await findByRole("dialog");
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

    fireEvent.click(within(dialog).getByRole("button", { name: "Confirm" }));

    // Submitting: nothing closes the dialog or starts another batch until
    // this one has settled — not Cancel, not Escape, not "Update selected",
    // not "Update all".
    await waitFor(() =>
      expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeDisabled(),
    );
    expect(within(dialog).getByRole("button", { name: "Confirm" })).toBeDisabled();
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

      await findByText("3 updates");
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

      await findByText("3 updates");
      fireEvent.click(getByRole("button", { name: "Update all" }));

      const dialog = await findByRole("dialog");
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

      fireEvent.click(within(dialog).getByRole("button", { name: "Confirm" }));
      await waitFor(() => expect(submittedPlanIds()).toHaveLength(3));
      await waitFor(() => expect(queryByRole("dialog")).not.toBeInTheDocument());
      expect(useUiStore.getState().selectedUpdates).toEqual([]);
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
      expect(zhCN.updates.count_other).toBe("{{count}} 个可更新");
      expect(zhCN.updates.cantUpdateHere).toBe("不能在这里更新的（{{number}}）");
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
      expect(chipDetail(await findRow("qwen3:8b"), "Unavailable")).toHaveTextContent(
        "Ollama isn't running. Start it, then check again.",
      );
      expect(chipDetail(rowOf("tokei"), "Unavailable")).toHaveTextContent(
        "Cargo isn't responding. Check again later.",
      );
      expect(chipDetail(rowOf("glib"), "Unavailable")).toHaveTextContent(
        "Homebrew won't run while Canager has administrator powers. Open Canager again the normal way.",
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

        fireEvent.click(await findByRole("button", { name: "不能在这里更新的（2）" }));
        expect(
          within(chipDetail(await findRow("glib"), "已固定")).getByText(
            wholeSentence("它在 Homebrew 里固定了版本。要更新，先在终端运行 /opt/homebrew/bin/brew unpin glib。"),
          ),
        ).toBeInTheDocument();
        expect(chipDetail(rowOf("urllib3"), "只读")).toHaveTextContent(
          "用 pip 装的包只能在这里查看。命令行工具建议改用 pipx 或 uv 安装。",
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

    chooseFromMenu(await findRow("glib"), "Never remind me about this software");

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
    await findByText("2 updates");
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
    expect(await findByText("1 update")).toBeInTheDocument();
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
      within(myForkMenu).getByRole("menuitem", { name: "Never remind me about this software" }),
    ).toBeInTheDocument();
    expect(within(myForkMenu).queryByRole("menuitem", { name: "Skip this version" })).toBeNull();
    fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });

    const glibMenu = openMenu(rowOf("glib"));
    expect(within(glibMenu).getByRole("menuitem", { name: "Skip this version" })).toBeInTheDocument();
  });

  // A Homebrew cask declared `version :latest`, as `brew outdated --json=v2
  // --greedy` lists one -- Include self-updating apps is what makes Canager
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
      within(menu).getByRole("menuitem", { name: "Never remind me about this software" }),
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
    await findByText("2 updates");
    expect(await findAllByRole("button", { name: "Update" })).toHaveLength(2);
  });

  it("skips an Ollama model's newer build by its digest without ever printing the digest", async () => {
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
      "No pending updates — you've skipped the rest or asked not to be reminded about them.",
    );
    expect(savedSettings().skipped_versions).toEqual([{ key: qwenKey, version: digest }]);
    expect(container.textContent).not.toMatch(/sha256|5642e974/);
  });

  it("says on each menu item what it will do", async () => {
    renderWithProviders(<UpdatesPage />);

    const menu = openMenu(await findRow("glib"));
    expect(within(menu).getByRole("menuitem", { name: "Skip this version" })).toHaveAccessibleDescription(
      "You'll be reminded again when its next version is out.",
    );
    expect(
      within(menu).getByRole("menuitem", { name: "Never remind me about this software" }),
    ).toHaveAccessibleDescription(
      "You won't be reminded about any update of this again. You can undo this in Settings.",
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

  it("calls them 跳过这个版本 and 不再提醒 in Chinese, and says what each does", () => {
    expect(zhCN.updates.skipVersion).toBe("跳过这个版本");
    expect(zhCN.updates.skipVersionHint).toBe("你会在它出下一个版本时再看到提醒。");
    expect(zhCN.updates.neverRemind).toBe("不再提醒这个软件");
    expect(zhCN.updates.neverRemindHint).toBe("以后不再提醒这个软件的任何更新，可在设置里撤销。");
    expect(zhCN.updates.copyCommand).toBe("复制命令");
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
    const never = within(menu).getByRole("menuitem", { name: "Never remind me about this software" });
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
      within(after).getByRole("menuitem", { name: "Never remind me about this software" }),
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
      ["waiting its turn", { status: "Queued" }, "Waiting"],
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
      expect(await within(glib).findByText("Failed")).toBeInTheDocument();
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

      expect(await within(await findRow("glib")).findByText("Check")).toBeInTheDocument();
      fireEvent.click(getByRole("button", { name: "View log: glib" }));
      expect(useUiStore.getState().focusedOpId).toBe(11);
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
      await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula glib");
      fireEvent.click(within(dialog).getByRole("button", { name: "Confirm" }));

      await waitFor(() => expect(useUiStore.getState().updateTargets).toEqual({ 7: "2.90.0" }));
    });
  });

  it("says every update is hidden — not that everything is up to date — once each is skipped or never reminded about", async () => {
    settings.ignored_updates = [glibKey];
    settings.skipped_versions = [{ key: onyxKey, version: "5.1.0" }];
    const { findByText, queryByText } = renderWithProviders(<UpdatesPage />);

    await findByText(
      "No pending updates — you've skipped the rest or asked not to be reminded about them.",
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

  it("warns that Homebrew's catalogue may be behind, with a way to retry", async () => {
    // A note, not an unavailability: brew answered, and what it said may
    // simply be out of date. "Everything is up to date" is the one
    // sentence that must not appear over it.
    updates = [];
    instances = [
      { ...snapshot.instances[0], status: { unavailable: null, notes: ["IndexMayBeStale"] } },
      ...snapshot.instances.slice(1),
    ];
    const { findByText, queryByText, getByRole } = renderWithProviders(<UpdatesPage />);

    await findByText("“Up to date” may not be accurate for Homebrew");
    expect(queryByText("Everything is up to date")).not.toBeInTheDocument();
    fireEvent.click(getByRole("button", { name: "Try again" }));
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

    await findByText("Homebrew is still downloading its latest list of software");
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
      expect(
        queryByText("Start Ollama and Canager will list what's in it and check it for updates."),
      ).toBeNull();
      const details = getByRole("button", { name: "Details: Ollama isn't running" });
      fireEvent.click(details);
      expect(
        document.getElementById(details.getAttribute("aria-controls") ?? ""),
      ).toHaveTextContent("Start Ollama and Canager will list what's in it and check it for updates.");
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

      await findByText("Canager can't reach Ollama right now");
      const details = getByRole("button", { name: "Details: Canager can't reach Ollama right now" });
      fireEvent.click(details);
      expect(
        document.getElementById(details.getAttribute("aria-controls") ?? ""),
      ).toHaveTextContent(
        "Ollama is installed but didn't answer. What's listed here is what Canager saw the last time it did, so anything added or removed since then is missing.",
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
        expect(chipDetail(row, "Unavailable")).toHaveTextContent(
          "Ollama isn't responding. Check again later.",
        );
      }
    });

    it("marks every row of a read-only source Read-only, and no row of another source", async () => {
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
      const { queryByText } = renderWithProviders(<UpdatesPage />);

      await showCantUpdate();
      await findRow("urllib3");
      for (const name of ["glib", "onyx", "jq"]) {
        expect(within(rowOf(name)).queryByRole("button", { name: "Read-only" })).toBeNull();
      }
      for (const name of pipPackages) {
        expect(within(rowOf(name)).getByRole("button", { name: "Read-only" })).toBeInTheDocument();
        expect(within(rowOf(name)).getByText("pip")).toBeInTheDocument();
      }
      // Not a notice line of its own: the rows say it.
      expect(queryByText("Read-only: pip packages")).toBeNull();
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
        name: "Details: Canager can't reach Homebrew right now",
      });
      fireEvent.click(details);
      expect(
        document.getElementById(details.getAttribute("aria-controls") ?? ""),
      ).toHaveTextContent(
        "Homebrew is installed but didn't answer. What's listed here is what Canager saw the last time it did, so anything added or removed since then is missing.",
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
        name: "Details: Canager can't reach Homebrew right now",
      });
      fireEvent.click(coldDetails);
      const text = document.getElementById(coldDetails.getAttribute("aria-controls") ?? "");
      expect(text).toHaveTextContent(
        "Homebrew is installed but didn't answer, so Canager doesn't know what's in it right now.",
      );
      expect(text?.textContent).not.toMatch(/What's listed here/);
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

    expect(await findByText("2 updates")).toBeInTheDocument();
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
      await within(dialog).findByText("A newer build of this model is available"),
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
    expect(chipDetail(claude, "Updates itself").textContent).toBe(
      "It usually updates itself. You can also update it now.",
    );
    expect(getAllByRole("button", { name: "Update" })).toHaveLength(1);
    expect(within(claude).getByRole("button", { name: "Update" })).toBeInTheDocument();
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
    expect(within(onyx).queryByRole("button", { name: "Updates itself" })).toBeNull();
  });

  it("gives a standalone tool's row the summary its Installed row shows, not 'No description'", async () => {
    // A standalone artifact's `description` is `null` on the wire (a bare
    // string cannot be localised), so its sentence is looked up by adapter
    // id, on this page as on the Installed page (`artifactBlurb`). Grok
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
    expect(
      within(grok).getByText(
        "xAI's Grok coding assistant for the terminal. Installed with its own installer.",
      ),
    ).toBeInTheDocument();
    expect(queryByText("No description")).toBeNull();
    expect(within(grok).queryByRole("button", { name: "Updates itself" })).toBeNull();
    expect(getAllByRole("button", { name: "Update" })).toHaveLength(1);
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
    expect(within(claude).queryByRole("button", { name: "Updates itself" })).toBeNull();
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
    expect(within(claude).queryByRole("button", { name: "Updates itself" })).toBeNull();
    expect(within(claude).getByRole("button", { name: "Unavailable" })).toBeInTheDocument();
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
    await findByText("1 update");
    await findByText("Can't update here (1)");
    await showCantUpdate();
    const claude = await findRow("Claude Code");
    expect(within(claude).getByText("2.1.281 → 2.1.290")).toBeInTheDocument();
    const detail = chipDetail(claude, "Updates itself");
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
    const detail = chipDetail(claude, "Updates itself");
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
    ["NotOnPath", "Claude Code isn't in your PATH"],
    ["ShadowedByHomebrew", "Another program named claude runs when you type claude"],
    ["ShadowedByNpm", "Another program named claude runs when you type claude"],
    ["ShadowedByOther", "Another program named claude runs when you type claude"],
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
      expect(within(claude).queryByRole("button", { name: "Updates itself" })).toBeNull();
      expect(queryByText(/usually updates itself|just run it/)).toBeNull();
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

    expect(await findByText("The claude link is still there, but its program is gone")).toBeInTheDocument();
    expect(await findByText("No updates in the sources Canager could check")).toBeInTheDocument();
    expect(queryByText("Everything is up to date")).toBeNull();
  });

  it("calls the two self-updating chips what the author asked for in Chinese", () => {
    // 会自动更新: a tool Canager can update too. 自动更新: one only it can.
    expect(zhCN.updates.selfUpdating).toBe("会自动更新");
    expect(zhCN.updates.selfUpdatingDetail).toBe("它平时会自己更新，也可以现在更新。");
    expect(zhCN.updates.blocked.SelfUpdatesOnly.badge).toBe("自动更新");
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
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --formula glib");
    await within(dialog).findByText("/opt/homebrew/bin/brew upgrade --cask onyx");

    const hints = within(dialog).getAllByText(
      "Don't close Canager or your Mac while this runs. Stopping it partway leaves a broken installation, so this can't be cancelled once it starts.",
    );
    expect(hints).toHaveLength(1);
    expect(hints[0].closest("div")?.textContent).toContain("onyx");
    expect(hints[0].closest("div")?.textContent).not.toContain("glib");
  });

  describe("Select all and Invert selection", () => {
    // Each button's accessible name starts with the words on it ("Select
    // all", "Invert selection") and goes on to say which rows it acts on.
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
      expect(getByRole("button", { name: INVERT }).textContent).toBe("Invert selection");
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
