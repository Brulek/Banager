import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { WithToolbarSlot } from "../test/toolbarSlot";
import { InstalledPage } from "./InstalledPage";
import i18n from "../i18n";
import { artifactKeyId, useUiStore } from "../store/ui";
import { queryKeys } from "../lib/queryKeys";
import { writeInventoryPreview } from "../lib/events";
import type { InstalledArtifact, IssuedPlan, ManagerInstance, OpRequest, OpSummary, Settings, Snapshot } from "../lib/types";
import { NO_FACTS, NO_SIZES } from "../lib/types";

// Uninstalling several tools at once, on the Installed page
// (.superpowers/r5/specs/r7-batch-uninstall.md): which rows get a box, the
// keyboard, and which ticks count.

const mockInvoke = vi.mocked(invoke);

const BREW = "brew:/opt/homebrew";
const brew: ManagerInstance = {
  id: BREW,
  adapter_id: "brew",
  exe_path: "/opt/homebrew/bin/brew",
  prefix: "/opt/homebrew",
  scope: "User",
  version: "7.0.3",
  unverified_version: null,
  read_only_reason: null,
  status: { unavailable: null, notes: [] },
};
const intel: ManagerInstance = {
  ...brew,
  id: "brew:/usr/local",
  exe_path: "/usr/local/bin/brew",
  prefix: "/usr/local",
  status: { unavailable: null, notes: ["IndexUpdating"] },
};
const pip: ManagerInstance = {
  ...brew,
  id: "pip:/opt/homebrew/bin/python3",
  adapter_id: "pip",
  exe_path: "/opt/homebrew/bin/python3",
  read_only_reason: "ByDesign",
};
const uv: ManagerInstance = {
  ...brew,
  id: "uv",
  adapter_id: "uv",
  exe_path: "/Users/you/.local/bin/uv",
  status: { unavailable: "NotResponding", notes: [] },
};

function artifact(instance: ManagerInstance, kind: InstalledArtifact["key"]["kind"], name: string, more: Partial<InstalledArtifact> = {}): InstalledArtifact {
  return {
    key: { instance_id: instance.id, kind, name },
    display_name: name,
    version: "1.0",
    reason: "Requested",
    description: `${name} blurb`,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
    facts: NO_FACTS,
    ...more,
  };
}

const jq = artifact(brew, "Formula", "jq");
const wget = artifact(brew, "Formula", "wget");
const git = artifact(brew, "Formula", "git");
const pinned = artifact(brew, "Formula", "postgresql@17", { uninstall_blocked: "Pinned" });
const requests = artifact(pip, "Package", "requests");
const ruff = artifact(uv, "Tool", "ruff");
const tree = artifact(intel, "Formula", "tree");

const snapshot: Snapshot = {
  generation: 1,
  round: 1,
  detect: "Found",
  instances: [brew, intel, pip, uv],
  artifacts: [jq, wget, git, pinned, requests, ruff, tree],
  updates: [],
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

let served: Snapshot;
let operations: OpSummary[];
/** The tools whose uninstall was submitted, in order. */
let submitted: string[];

beforeEach(() => {
  mockInvoke.mockReset();
  served = snapshot;
  operations = [];
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (this: HTMLElement) {
    return this.getAttribute("data-index") === null ? 900 : 52;
  });
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
  submitted = [];
  let ops = 10;
  mockInvoke.mockImplementation((cmd: string, args?: unknown) => {
    if (cmd === "get_snapshot") return Promise.resolve(served);
    if (cmd === "get_settings") return Promise.resolve(settings);
    if (cmd === "list_operations") return Promise.resolve(operations);
    if (cmd === "get_sizes") return Promise.resolve(NO_SIZES);
    if (cmd === "plan_operation") {
      const request = (args as { request: OpRequest }).request;
      return Promise.resolve({
        id: request.name.padStart(32, "0"),
        plan: {
          request,
          action: { Command: { program: brew.exe_path, args: ["uninstall", "--formula", request.name], env: [] } },
          needs_password: false,
          locks: [request.instance_id],
          cancel_policy: "KillThenReconcile",
          warnings: [{ UninstallScope: { what: "HomebrewFormulaOnly" } }],
          affected: [],
          timeout_secs: 1800,
        },
        issued_at: 1789700000,
      } satisfies IssuedPlan);
    }
    if (cmd === "submit_operation") {
      submitted.push((args as { planId: string }).planId.replace(/^0+/, ""));
      ops += 1;
      return Promise.resolve(ops);
    }
    return Promise.resolve(undefined);
  });
});

afterEach(async () => {
  vi.restoreAllMocks();
  await i18n.changeLanguage("en");
});

function renderInstalled() {
  return renderWithProviders(
    <WithToolbarSlot>
      <InstalledPage />
    </WithToolbarSlot>,
  );
}

function rowOf(name: string): HTMLElement {
  const rows = screen
    .getAllByText(name, { selector: "[data-tool-row] p" })
    .map((element) => element.closest("[data-tool-row]"));
  if (rows.length !== 1 || !(rows[0] instanceof HTMLElement)) throw new Error(`expected one row named ${name}`);
  return rows[0];
}

async function findRow(name: string): Promise<HTMLElement> {
  await screen.findByText(name, { selector: "[data-tool-row] p" });
  return rowOf(name);
}

const boxOf = (name: string) => within(rowOf(name)).queryByRole("checkbox");
const status = () => document.querySelector("[data-selection-status]")?.textContent ?? "";

describe("the Installed page's checkboxes", () => {
  it("puts a box exactly on the rows whose Uninstall is there and enabled", async () => {
    operations = [
      {
        id: 3,
        kind: "Uninstall",
        instance_id: BREW,
        artifact_kind: "Formula",
        name: "git",
        status: "Queued",
        outcome: null,
        argv_preview: [],
        cancel_policy: "KillThenReconcile",
      },
    ];
    renderInstalled();
    await findRow("jq");
    expect(boxOf("jq")).toHaveAccessibleName("Select jq to uninstall");
    expect(boxOf("wget")).not.toBeNull();
    // An uninstall of it queued, pinned, a read-only source, a source that
    // did not answer, a Homebrew updating its list: no Uninstall, no box.
    await waitFor(() => expect(boxOf("git")).toBeNull());
    expect(within(rowOf("git")).getByRole("button", { name: "Queued: uninstall git" })).toBeDisabled();
    for (const name of ["postgresql@17", "requests", "ruff"]) expect(boxOf(name)).toBeNull();
    expect(boxOf("tree")).toBeNull();
    // The column keeps its room on every row, so the avatars line up.
    for (const name of ["jq", "postgresql@17", "requests"]) {
      expect(rowOf(name).querySelector("[data-row-separator]")?.className).toContain("left-[5.75rem]");
    }
  });

  it("puts none on the first check's list, whose every Uninstall waits for it", async () => {
    served = { ...snapshot, generation: 0, round: 0, detect: "Missing", instances: [], artifacts: [], refreshed_at: null };
    const { queryClient } = renderInstalled();
    act(() => writeInventoryPreview(queryClient, { round: 1, instances: [brew], artifacts: [jq, wget] }));
    await findRow("jq");
    expect(boxOf("jq")).toBeNull();
    expect(boxOf("wget")).toBeNull();
    expect(screen.getByRole("checkbox", { name: "Select all items that can be uninstalled here" })).toBeDisabled();
  });

  it("ticks a row with Space, opens its details with Enter, and opens a row with no box with Space", async () => {
    renderInstalled();
    const row = await findRow("jq");
    act(() => row.focus());
    fireEvent.keyDown(row, { key: " " });
    expect(boxOf("jq")).toBeChecked();
    expect(screen.queryByRole("complementary")).toBeNull();
    fireEvent.keyDown(row, { key: " " });
    expect(boxOf("jq")).not.toBeChecked();
    fireEvent.keyDown(row, { key: "Enter" });
    expect(await screen.findByRole("complementary", { name: "jq" })).toBeInTheDocument();

    fireEvent.keyDown(rowOf("postgresql@17"), { key: " " });
    expect(await screen.findByRole("complementary", { name: "postgresql@17" })).toBeInTheDocument();
  });

  it("keeps the details and the ticks apart", async () => {
    renderInstalled();
    await findRow("jq");
    fireEvent.click(within(rowOf("jq")).getByRole("button", { name: "Details: jq" }));
    expect(await screen.findByRole("complementary", { name: "jq" })).toBeInTheDocument();
    expect(boxOf("jq")).not.toBeChecked();
    fireEvent.click(boxOf("wget")!);
    expect(boxOf("wget")).toBeChecked();
    expect(screen.getByRole("complementary", { name: "jq" })).toBeInTheDocument();
    expect(rowOf("wget")).not.toHaveAttribute("data-selected");
  });

  it("keeps ticks through a search, counting only the rows shown", async () => {
    renderInstalled();
    await findRow("jq");
    fireEvent.click(boxOf("jq")!);
    fireEvent.click(boxOf("wget")!);
    expect(status()).toBe("2 selected");
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "wg" } });
    await waitFor(() => expect(screen.queryByText("jq", { selector: "[data-tool-row] p" })).toBeNull());
    expect(status()).toBe("1 selected");
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "" } });
    await findRow("jq");
    expect(boxOf("jq")).toBeChecked();
    expect(status()).toBe("2 selected");
  });

  it("drops for good the tick of a tool that leaves the list", async () => {
    const { queryClient } = renderInstalled();
    await findRow("jq");
    fireEvent.click(boxOf("jq")!);
    fireEvent.click(boxOf("wget")!);
    act(() => {
      queryClient.setQueryData(queryKeys.snapshot, { ...served, generation: 2, artifacts: served.artifacts.filter((a) => a !== jq) });
    });
    await waitFor(() => expect(useUiStore.getState().selectedUninstalls).toEqual([artifactKeyId(wget.key)]));
    // Installed again: not ticked.
    act(() => {
      queryClient.setQueryData(queryKeys.snapshot, { ...served, generation: 3 });
    });
    await findRow("jq");
    expect(boxOf("jq")).not.toBeChecked();
    expect(boxOf("wget")).toBeChecked();
  });
});

describe("Uninstall Selected, on the Installed page", () => {
  const button = () => screen.queryByRole("button", { name: /^Uninstall Selected/ });

  it("counts the ticked rows the list shows, and opens their sheet in the list's order", async () => {
    renderInstalled();
    await findRow("jq");
    expect(button()).toBeNull();
    fireEvent.click(boxOf("wget")!);
    fireEvent.click(boxOf("jq")!);
    expect(button()).toHaveAccessibleName("Uninstall Selected (2)…");
    // jq hidden by a search: kept, not counted.
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "wg" } });
    await waitFor(() => expect(button()).toHaveAccessibleName("Uninstall Selected (1)…"));
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "" } });
    await waitFor(() => expect(button()).toHaveAccessibleName("Uninstall Selected (2)…"));

    fireEvent.click(button()!);
    const dialog = await screen.findByRole("dialog", { name: "Uninstall these 2 tools?" });
    expect([...dialog.querySelectorAll("[data-sheet-name]")].map((name) => name.textContent)).toEqual(["jq", "wget"]);
    // One batch at a time (under the sheet, out of the accessibility tree).
    expect(document.querySelector("[data-uninstall-selected]")).toBeDisabled();
    const uninstall = await within(dialog).findByRole("button", { name: "Uninstall 2 Tools" });
    await waitFor(() => expect(uninstall).toBeEnabled());
    fireEvent.click(uninstall);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(submitted).toEqual(["jq", "wget"]);
    // Started: unticked, the button gone with them, and the focus in the list.
    expect(useUiStore.getState().selectedUninstalls).toEqual([]);
    expect(button()).toBeNull();
    await waitFor(() => expect(document.activeElement?.closest("[data-tool-row]")).not.toBeNull());
  });

  it("is off with more ticked than one batch takes", async () => {
    const many = Array.from({ length: 21 }, (_, index) => artifact(brew, "Formula", `tool-${String(index).padStart(2, "0")}`));
    served = { ...snapshot, artifacts: many };
    renderInstalled();
    await findRow("tool-00");
    act(() => useUiStore.getState().selectUninstalls(many.map((a) => a.key)));
    expect(button()).toHaveAccessibleName("Uninstall Selected (21)…");
    expect(button()).toBeDisabled();
    expect(status()).toBe("21 selected. Up to 20 can be uninstalled at a time");
  });
});
