import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import i18n from "../i18n";
import { UninstallDialog } from "./UninstallDialog";
import { KeptDataGroup } from "./KeptDataGroup";
import { keptDataOf } from "../lib/keptData";
import { deletesForGood, isCaution, warningDetailKey, warningGroup, warningKey, warningArgs, warningLines } from "../lib/warnings";
import type { IssuedPlan, OpRequest, Warning } from "../lib/types";

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

  it("has no group when nothing stays", async () => {
    open([{ UninstallScope: { what: "Npm" } }]);
    await screen.findByRole("button", { name: "Show Command" });
    expect(screen.queryByRole("region", { name: "Stays after uninstalling" })).toBeNull();
  });
});
