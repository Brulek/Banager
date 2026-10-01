import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import i18n from "../i18n";
import { UninstallDialog } from "./UninstallDialog";
import { KeptDataGroup } from "./KeptDataGroup";
import { keptDataOf } from "../lib/keptData";
import { deletesForGood, isCaution, warningDetailKey, warningGroup, warningKey, warningArgs, warningLines } from "../lib/warnings";
import type { InstalledArtifact, IssuedPlan, ManagerInstance, OpRequest, Snapshot, Warning } from "../lib/types";
import { NO_FACTS } from "../lib/types";

// The same strings `test_keeps_data_is_the_json_the_typescript_mirror_reads`
// pins in crates/banager-core/src/model.rs.
const MEASURED_WIRE =
  '{"KeepsData":{"path":"~/.claude","what":"ToolData","size":{"bytes":432013312,"partial":false,"at_least":true},"left_out":[]}}';
const UNKNOWN_WIRE = '{"KeepsData":{"path":"~/.ollama/models","what":"Models","size":null,"left_out":[]}}';
const LEFT_OUT_WIRE =
  '{"KeepsData":{"path":"~/.codex","what":"ToolData","size":{"bytes":38400000,"partial":false,"at_least":false},"left_out":["~/.codex/packages/standalone"]}}';

const npmClaude: OpRequest = {
  kind: "Uninstall",
  instance_id: "npm:/opt/homebrew",
  artifact_kind: "Package",
  name: "@anthropic-ai/claude-code",
};

function issued(warnings: Warning[], request: OpRequest = npmClaude): IssuedPlan {
  return {
    id: "1",
    plan: {
      request,
      action: {
        Command: { program: "/opt/homebrew/bin/npm", args: ["uninstall", "-g", request.name], env: [] },
      },
      needs_password: false,
      locks: [request.instance_id],
      cancel_policy: "KillThenReconcile",
      warnings,
      affected: [],
      timeout_secs: 1800,
    },
    issued_at: 1758000000,
  };
}

const claudeKept: Warning[] = [
  { UninstallScope: { what: "Npm" } },
  {
    KeepsData: {
      path: "~/.claude",
      what: "ToolData",
      size: { bytes: 432_013_312, partial: false, at_least: false },
      left_out: [],
    },
  },
  { KeepsData: { path: "~/.claude.json", what: "ToolData", size: null, left_out: [] } },
];

describe("KeepsData on the wire", () => {
  it("reads what Rust sends and sends it back the same", () => {
    for (const wire of [MEASURED_WIRE, UNKNOWN_WIRE, LEFT_OUT_WIRE]) {
      const warning = JSON.parse(wire) as Warning;
      expect(JSON.stringify(warning)).toBe(wire);
    }
    expect(
      keptDataOf([
        JSON.parse(MEASURED_WIRE) as Warning,
        JSON.parse(UNKNOWN_WIRE) as Warning,
        JSON.parse(LEFT_OUT_WIRE) as Warning,
        // An older line, before `left_out`.
        JSON.parse('{"KeepsData":{"path":"~/.gemini","what":"ToolData","size":null}}') as Warning,
      ]),
    ).toEqual([
      { path: "~/.claude", what: "ToolData", size: { bytes: 432013312, partial: false, at_least: true }, leftOut: [] },
      { path: "~/.ollama/models", what: "Models", size: null, leftOut: [] },
      {
        path: "~/.codex",
        what: "ToolData",
        size: { bytes: 38400000, partial: false, at_least: false },
        leftOut: ["~/.codex/packages/standalone"],
      },
      { path: "~/.gemini", what: "ToolData", size: null, leftOut: [] },
    ]);
  });

  it("says what it leaves out behind the ⓘ of what it holds when its size is not known", () => {
    const unsized = '{"KeepsData":{"path":"~/.codex","what":"ToolData","size":null,"left_out":["~/.codex/packages/standalone"]}}';
    const { container } = renderWithProviders(<KeptDataGroup warnings={[JSON.parse(unsized) as Warning]} />);
    expect(container.querySelector("[data-kept-size]")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Details: ~/.codex" }));
    expect(
      screen.getByText("Not counting ~/.codex/packages/standalone, which holds another copy installed on its own."),
    ).toBeInTheDocument();
  });

  it("says behind an ⓘ by the size what it leaves out: Codex's own install inside ~/.codex", () => {
    renderWithProviders(<KeptDataGroup warnings={[JSON.parse(LEFT_OUT_WIRE) as Warning]} />);
    const info = screen.getByRole("button", { name: "About the size: ~/.codex" });
    fireEvent.click(info);
    expect(
      screen.getByText("Not counting ~/.codex/packages/standalone, which holds another copy installed on its own."),
    ).toBeInTheDocument();
  });

  it("is its own group, plain, never a deletion", () => {
    const warning = JSON.parse(UNKNOWN_WIRE) as Warning;
    expect(warningGroup(warning)).toBe("data");
    // No line: only its own group shows it (`KeptDataGroup`).
    expect(warningKey(warning)).toBeNull();
    expect(warningArgs(warning)).toEqual({});
    expect(warningLines((key) => key, [warning]).data).toEqual([]);
    expect(warningDetailKey(warning)).toBeNull();
    expect(isCaution(warning)).toBe(false);
    expect(deletesForGood(warning)).toBe(false);
  });
});

describe("the uninstall dialog's 「卸载后会保留」 group", () => {
  let writeText: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
  });

  afterEach(async () => {
    Object.defineProperty(navigator, "clipboard", { value: undefined, configurable: true });
    await i18n.changeLanguage("en");
  });

  function open(warnings: Warning[], request: OpRequest = npmClaude, name = "Claude Code") {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "plan_operation") return issued(warnings, request);
      return null;
    });
    renderWithProviders(<UninstallDialog open onOpenChange={() => {}} request={request} displayName={name} />);
  }

  it("lists each path that stays, about how big where known, what it holds, and Copy Path", async () => {
    open(claudeKept);
    const group = await screen.findByRole("region", { name: "Stays after uninstalling" });
    const rows = within(group).getAllByRole("listitem");
    expect(rows.map((row) => row.textContent)).toEqual([
      "~/.claude · About 432 MBThis tool's settings and dataCopy Path",
      // Size unknown: the path alone.
      "~/.claude.jsonThis tool's settings and dataCopy Path",
    ]);
    const copy = within(group).getByRole("button", { name: "Copy path: ~/.claude" });
    fireEvent.click(copy);
    await waitFor(() => expect(writeText).toHaveBeenCalledWith("~/.claude"));
    // Beside the button that was pressed, as the details' copy buttons say it; the other row says nothing.
    expect(await within(rows[0]).findByRole("status")).toHaveTextContent(/^Copied$/);
    expect(within(rows[0]).getByRole("status").parentElement).toBe(copy.parentElement);
    expect(within(rows[1]).getByRole("status")).toBeEmptyDOMElement();
  });

  it("offers nothing that deletes what stays", async () => {
    open(claudeKept);
    const group = await screen.findByRole("region", { name: "Stays after uninstalling" });
    const buttons = within(group).getAllByRole("button");
    expect(buttons.map((button) => button.textContent)).toEqual(["Copy Path", "Copy Path"]);
    expect(group.textContent).not.toMatch(/delete|remove|trash|删除|移除|废纸篓/i);
    // Nor does it make the uninstall a permanent one.
    expect(screen.getByRole("button", { name: "Uninstall" })).toBeInTheDocument();
  });

  it("says a budget cut short as at least, and the models as models, in Chinese", async () => {
    await i18n.changeLanguage("zh-CN");
    const ollama: OpRequest = {
      kind: "Uninstall",
      instance_id: "brew:/opt/homebrew",
      artifact_kind: "Formula",
      name: "ollama",
    };
    open(
      [
        {
          KeepsData: {
            path: "~/.ollama/models",
            what: "Models",
            size: { bytes: 6_600_000_000, partial: false, at_least: true },
            left_out: [],
          },
        },
      ],
      ollama,
      "ollama",
    );
    const group = await screen.findByRole("region", { name: "卸载后会保留" });
    const [row] = within(group).getAllByRole("listitem");
    expect(row.textContent).toContain("~/.ollama/models");
    expect(row.textContent).toContain("6.6 GB以上");
    expect(row.textContent).toContain("下载的模型");
    expect(within(group).getByRole("button", { name: "拷贝路径：~/.ollama/models" })).toHaveTextContent("拷贝路径");
  });

  it("drops the scope sentence's general \"settings and data elsewhere\" where the group names what stays", async () => {
    open([{ UninstallScope: { what: "Npm" } }, ...claudeKept]);
    await screen.findByRole("region", { name: "Stays after uninstalling" });
    expect(
      screen.getByText("Deletes Claude Code's folder in npm's global folder and its commands; npm runs none of its code."),
    ).toBeInTheDocument();
    expect(document.body.textContent).not.toMatch(/settings and data outside that folder are not deleted/);
  });

  it("keeps the general sentence where nothing is named", async () => {
    open([{ UninstallScope: { what: "Npm" } }]);
    expect(await screen.findByText(/its settings and data outside that folder are not deleted/)).toBeInTheDocument();
  });

  it("has no group when nothing stays", async () => {
    open([{ UninstallScope: { what: "Npm" } }]);
    await screen.findByRole("button", { name: "Show Command" });
    expect(screen.queryByRole("region", { name: "Stays after uninstalling" })).toBeNull();
  });
});

describe("the uninstall dialog's notes on what else stays", () => {
  const base = (key: InstalledArtifact["key"], over: Partial<InstalledArtifact>): InstalledArtifact => ({
    key,
    display_name: key.name,
    version: "1.0",
    reason: "Requested",
    description: null,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
    facts: NO_FACTS,
    ...over,
  });
  const instance = (id: string, adapter: string): ManagerInstance => ({
    id,
    adapter_id: adapter,
    exe_path: "/x",
    prefix: "/x",
    scope: "User",
    version: "1",
    unverified_version: null,
    read_only_reason: null,
    status: { unavailable: null, notes: [] },
  });

  function openWith(snapshot: Snapshot, request: OpRequest, name: string) {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "plan_operation") return issued([], request);
      if (cmd === "get_snapshot") return snapshot;
      return null;
    });
    renderWithProviders(<UninstallDialog open onOpenChange={() => {}} request={request} displayName={name} />);
  }

  const snapshotOf = (instances: ManagerInstance[], artifacts: InstalledArtifact[]): Snapshot => ({
    generation: 1,
    round: 1,
    detect: "Found",
    instances,
    artifacts,
    updates: [],
    refreshed_at: 1,
    stale: false,
    errors: [],
  });

  it("says that Codex's own copy stays and codex still works, for the npm copy Terminal does not run", async () => {
    const own = base(
      { instance_id: "standalone-codex", kind: "Binary", name: "codex" },
      { facts: { ...NO_FACTS, family: "codex", commands: [{ name: "codex", state: "Runs" }] } },
    );
    const npmKey = { instance_id: "npm:/opt/homebrew", kind: "Package" as const, name: "@openai/codex" };
    const npmCodex = base(npmKey, {
      facts: { ...NO_FACTS, family: "codex", commands: [{ name: "codex", state: { ShadowedBy: { by: own.key } } }] },
    });
    openWith(
      snapshotOf([instance("standalone-codex", "standalone-codex"), instance("npm:/opt/homebrew", "npm")], [own, npmCodex]),
      { kind: "Uninstall", instance_id: npmKey.instance_id, artifact_kind: "Package", name: npmKey.name },
      "@openai/codex",
    );
    expect(
      await screen.findByText("The copy from Codex's own installer stays, and codex still works in Terminal."),
    ).toBeInTheDocument();
  });

  it("says about how much removing a model frees, less what another model shares", async () => {
    const key = { instance_id: "ollama:/opt/homebrew", kind: "Model" as const, name: "llama3.2:3b" };
    openWith(
      snapshotOf([instance("ollama:/opt/homebrew", "ollama")], [base(key, { size_bytes: 2_019_393_189 })]),
      { kind: "Uninstall", instance_id: key.instance_id, artifact_kind: "Model", name: key.name },
      "llama3.2:3b",
    );
    expect(await screen.findByText("Frees about 2 GB, less any part other models share.")).toBeInTheDocument();
  });
});
