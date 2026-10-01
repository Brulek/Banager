import { describe, expect, it, vi } from "vitest";
import { within } from "@testing-library/react";
import { renderWithProviders } from "../test/setup";
import { invoke } from "@tauri-apps/api/core";
import type { CommandState, InstalledArtifact, IssuedPlan, ManagerInstance, Snapshot, UpdateCandidate } from "../lib/types";
import { NO_FACTS } from "../lib/types";
import { SheetTool } from "./SheetParts";
import { UpdateConfirmDialog, type Batch, type BatchItem, type UpdateConfirm } from "./UpdateConfirm";
import i18n from "../i18n";

// The real component, watched: the dialog draws each tool of its list
// through it, so its calls count the tools drawn.
vi.mock("./SheetParts", async (original) => {
  const actual = await original<typeof import("./SheetParts")>();
  return { ...actual, SheetTool: vi.fn(actual.SheetTool) };
});

const candidate = (name: string): UpdateCandidate => ({
  key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name },
  current: "1.0.0",
  target: "1.1.0",
  channel: "Native",
  checkable: true,
  warnings: [],
  blocked: null,
});

const item = (of: UpdateCandidate, over: Partial<BatchItem> = {}): BatchItem => ({
  candidate: of,
  name: of.key.name,
  issued: null,
  planError: null,
  planErrorDetail: null,
  submittedOpId: null,
  submitError: null,
  submitErrorDetail: null,
  ...over,
});

const confirmOf = (batch: Batch): UpdateConfirm => ({
  openConfirm: vi.fn(),
  afterClose: vi.fn(),
  returnFocusTo: { current: null },
  dialogOpen: true,
  pageErrors: [],
  refusalOf: () => null,
  batch,
  submitting: batch.phase === "submitting",
  confirmAndSubmit: vi.fn(),
  close: vi.fn(),
});

describe("UpdateConfirmDialog", () => {
  it("draws a tool of its list again only when what it shows of it changes, not each time the dialog is drawn", () => {
    const tools = ["glib", "jq", "wget"].map(candidate);
    const drawn = vi.mocked(SheetTool);
    drawn.mockClear();
    const { rerender, getByRole } = renderWithProviders(
      <UpdateConfirmDialog confirm={confirmOf({ id: 1, phase: "planning", items: tools.map((c) => item(c)) })} />,
    );
    const dialog = getByRole("dialog", { name: "Update 3 tools?" });
    expect(drawn.mock.calls.map(([props]) => props.name)).toEqual(["glib", "jq", "wget"]);

    // The page drew it again with the same batch, its items new objects,
    // as a plan came back: no tool is drawn again.
    drawn.mockClear();
    rerender(<UpdateConfirmDialog confirm={confirmOf({ id: 1, phase: "planning", items: tools.map((c) => item(c)) })} />);
    expect(drawn).not.toHaveBeenCalled();

    // jq's update has started: jq alone is drawn again, and says so.
    rerender(
      <UpdateConfirmDialog
        confirm={confirmOf({
          id: 1,
          phase: "submitting",
          items: tools.map((c) => item(c, c.key.name === "jq" ? { submittedOpId: 7 } : {})),
        })}
      />,
    );
    expect(drawn.mock.calls.map(([props]) => props.name)).toEqual(["jq"]);
    const jq = within(dialog).getByText("jq").closest("[data-sheet-tool]") as HTMLElement;
    expect(within(jq).getByText("Started")).toBeInTheDocument();
  });
});

describe("UpdateConfirmDialog's list of several", () => {
  it("names the list it scrolls with the keyboard, in either language, as it is a Tab stop of its own", async () => {
    const tools = ["glib", "jq"].map(candidate);
    const batch: Batch = { id: 2, phase: "planning", items: tools.map((c) => item(c)) };
    const { getByRole, unmount } = renderWithProviders(<UpdateConfirmDialog confirm={confirmOf(batch)} />);
    const list = within(getByRole("dialog", { name: "Update 2 tools?" })).getByRole("list", { name: "Tools to update" });
    expect(list).toHaveAttribute("tabindex", "0");
    unmount();

    await i18n.changeLanguage("zh-CN");
    try {
      const zh = renderWithProviders(<UpdateConfirmDialog confirm={confirmOf(batch)} />);
      expect(zh.getByRole("list", { name: "要更新的工具" })).toHaveAttribute("tabindex", "0");
    } finally {
      await i18n.changeLanguage("en");
    }
  });
});

describe("UpdateConfirmDialog on a copy Terminal does not run", () => {
  const codexCopy = (instanceId: string, name: string, version: string, state: CommandState): InstalledArtifact => ({
    key: { instance_id: instanceId, kind: instanceId.startsWith("npm") ? "Package" : "Binary", name },
    display_name: name,
    version,
    reason: "Requested",
    description: null,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
    facts: { ...NO_FACTS, family: "codex", commands: [{ name: "codex", state }] },
  });
  const own = codexCopy("standalone-codex", "codex", "0.159.3", "Runs");
  const npm = codexCopy("npm:/opt/homebrew", "@openai/codex", "0.155.1", { ShadowedBy: { by: own.key } });
  const instance = (id: string, adapter: string): ManagerInstance => ({
    id,
    adapter_id: adapter,
    exe_path: "/opt/homebrew/bin/npm",
    prefix: "/opt/homebrew",
    scope: "User",
    version: "11.0.0",
    status: { unavailable: null, notes: [] },
    unverified_version: null,
    read_only_reason: null,
  });
  const snapshot: Snapshot = {
    generation: 1,
    round: 1,
    detect: "Found",
    instances: [instance("npm:/opt/homebrew", "npm"), instance("standalone-codex", "standalone-codex")],
    artifacts: [own, npm],
    updates: [],
    refreshed_at: 1,
    stale: false,
    errors: [],
  };
  const issued = (name: string): IssuedPlan => ({
    id: name,
    plan: {
      request: { kind: "Upgrade", instance_id: "npm:/opt/homebrew", artifact_kind: "Package", name },
      action: { Command: { program: "/opt/homebrew/bin/npm", args: ["install", "-g", name], env: [] } },
      needs_password: false,
      locks: ["npm:/opt/homebrew"],
      cancel_policy: "KillThenReconcile",
      warnings: [],
      affected: [],
      timeout_secs: 1800,
    },
    issued_at: 1,
  });
  const npmCandidate: UpdateCandidate = { ...candidate("@openai/codex"), key: npm.key, current: "0.155.1", target: "0.159.3" };

  it("says under it that the update leaves what Terminal runs as it was, with which copy runs behind its ⓘ", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => (cmd === "get_snapshot" ? snapshot : null));
    const jq = candidate("jq");
    const batch: Batch = {
      id: 3,
      phase: "ready",
      items: [
        item(npmCandidate, { issued: issued("@openai/codex") }),
        item(jq, { issued: issued("jq") }),
      ],
    };
    const { findByText, getByRole } = renderWithProviders(<UpdateConfirmDialog confirm={confirmOf(batch)} />);
    const note = await findByText(/^Terminal runs another copy\./);
    expect(note).toHaveTextContent("Terminal runs another copy. Updating this one doesn't change what codex runs in Terminal.");
    const dialog = getByRole("dialog");
    expect(dialog).toContainElement(note);
    // Which copy runs is behind the line's ⓘ, as on the row.
    expect(within(dialog).getByRole("button", { name: /^Details: Terminal runs another copy/ })).toBeInTheDocument();
    // Only under the copy Terminal does not use.
    expect(within(dialog).getAllByText(/Terminal runs another copy/)).toHaveLength(1);
  });
});
