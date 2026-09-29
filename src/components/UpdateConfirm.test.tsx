import { describe, expect, it, vi } from "vitest";
import { within } from "@testing-library/react";
import { renderWithProviders } from "../test/setup";
import type { UpdateCandidate } from "../lib/types";
import { SheetTool } from "./SheetParts";
import { UpdateConfirmDialog, type Batch, type BatchItem, type UpdateConfirm } from "./UpdateConfirm";

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
