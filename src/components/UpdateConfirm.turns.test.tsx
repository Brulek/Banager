import { Profiler } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, waitFor, within } from "@testing-library/react";
import { renderWithProviders } from "../test/setup";
import type { IssuedPlan, UpdateCandidate } from "../lib/types";
import { TOOLS_DRAWN_FIRST, TOOLS_DRAWN_PER_TURN } from "./SheetParts";
import { UpdateConfirmDialog, type Batch, type BatchItem, type UpdateConfirm } from "./UpdateConfirm";

// Update all over a long list draws its tools a turn at a time
// (`useToolsInTurn`), as it opens and again once its plans are back; what
// it shows once every turn has run is what it showed when it drew them all
// at once.

const COUNT = 200;
const names = Array.from({ length: COUNT }, (_, i) => `tool-${String(i).padStart(3, "0")}`);
/** The one whose plan was refused, and those whose plans have a note: every other one. */
const REFUSED = "tool-150";
const noted = (name: string) => name !== REFUSED && Number(name.slice(5)) % 2 === 0;

const candidate = (name: string): UpdateCandidate => ({
  key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name },
  current: "1.0.0",
  target: "1.1.0",
  channel: "Native",
  checkable: true,
  warnings: [],
  blocked: null,
});

const issued = (name: string, index: number): IssuedPlan => ({
  id: String(index + 1).padStart(32, "0"),
  plan: {
    request: { kind: "Upgrade", instance_id: "brew:/opt/homebrew", artifact_kind: "Formula", name },
    action: { Command: { program: "/opt/homebrew/bin/brew", args: ["upgrade", "--formula", name], env: [] } },
    needs_password: false,
    locks: ["brew:/opt/homebrew"],
    // "Can't be cancelled once it starts", with its why behind an ⓘ.
    cancel_policy: noted(name) ? "NoCancel" : "KillThenReconcile",
    warnings: [],
    affected: [],
    timeout_secs: 1800,
  },
  issued_at: 1758000000,
});

const blank = (name: string): BatchItem => ({
  candidate: candidate(name),
  name,
  issued: null,
  planError: null,
  planErrorDetail: null,
  submittedOpId: null,
  submitError: null,
  submitErrorDetail: null,
});

const planning: Batch = { id: 1, phase: "planning", items: names.map(blank) };
const ready: Batch = {
  id: 1,
  phase: "ready",
  items: names.map((name, index) =>
    name === REFUSED ? { ...blank(name), planError: `${name} is pinned` } : { ...blank(name), issued: issued(name, index) },
  ),
};
/** The list's order once every plan is back: the refusal, then the tools with a note, then the rest. */
const finalOrder = [REFUSED, ...names.filter(noted), ...names.filter((name) => name !== REFUSED && !noted(name))];

const confirmOf = (batch: Batch): UpdateConfirm => ({
  openConfirm: vi.fn(),
  afterClose: vi.fn(),
  returnFocusTo: { current: null },
  dialogOpen: true,
  pageErrors: [],
  refusalOf: (item) => (item.planError === null ? null : { text: `Couldn't prepare the update: ${item.planError}`, detail: null }),
  batch,
  submitting: batch.phase === "submitting",
  confirmAndSubmit: vi.fn(),
  close: vi.fn(),
});

const rowsOf = (dialog: HTMLElement) => [...dialog.querySelectorAll<HTMLElement>("[data-sheet-tool]")];
const namesOf = (dialog: HTMLElement) => rowsOf(dialog).map((row) => row.querySelector("[data-sheet-name]")!.textContent);
/** A tool with something to say under its name: its lines are a list of their own. */
const sayingOf = (dialog: HTMLElement) => rowsOf(dialog).filter((row) => row.querySelector("li, [role='alert']") !== null);

/** After each drawing the dialog commits: how many tools its list has, how many say something, and whether it is busy. */
let commits: Array<{ rows: number; saying: number; busy: boolean }> = [];
function recordCommit() {
  const dialog = document.querySelector<HTMLElement>("[role='alertdialog']");
  const list = dialog?.querySelector("[data-sheet-tools]");
  if (dialog && list) {
    commits.push({ rows: rowsOf(dialog).length, saying: sayingOf(dialog).length, busy: list.getAttribute("aria-busy") === "true" });
  }
}
/** The dialog, each of its drawings recorded (`commits`). */
const watched = (batch: Batch) => (
  <Profiler id="update-confirm" onRender={recordCommit}>
    <UpdateConfirmDialog confirm={confirmOf(batch)} />
  </Profiler>
);

/** The list's markup, less the ids React makes up for each drawing (`useId`) and what points at them. */
const html = (dialog: HTMLElement) =>
  dialog
    .querySelector("[data-sheet-tools]")!
    .outerHTML.replace(/ (id|for|aria-controls|aria-describedby|aria-labelledby)="[^"]*"/g, " $1");

beforeEach(() => {
  commits = [];
});

afterEach(() => {
  vi.restoreAllMocks();
});

describe("UpdateConfirmDialog over a long Update all", () => {
  it("draws its tools a turn at a time as it opens, every one in the end, in the list's order", async () => {
    const { getByRole } = renderWithProviders(watched(planning));
    const dialog = getByRole("alertdialog", { name: `Update ${COUNT} tools?` });
    await waitFor(() => expect(rowsOf(dialog)).toHaveLength(COUNT));
    expect(namesOf(dialog)).toEqual(names);
    // The first few with the dialog, then never more than a turn's at once.
    const drawn = commits.map(({ rows }) => rows).filter((rows) => rows > 0);
    expect(drawn[0]).toBe(TOOLS_DRAWN_FIRST);
    for (let i = 1; i < drawn.length; i += 1) {
      expect(drawn[i] - drawn[i - 1]).toBeLessThanOrEqual(TOOLS_DRAWN_PER_TURN);
    }
    expect(new Set(drawn).size).toBeGreaterThanOrEqual(1 + Math.ceil((COUNT - TOOLS_DRAWN_FIRST) / TOOLS_DRAWN_PER_TURN));
  });

  it("once its plans are back, shows its first tools as they are at once and the rest a turn at a time, keeping every tool, its title and its Update", async () => {
    const { getByRole, rerender } = renderWithProviders(watched(planning));
    const dialog = getByRole("alertdialog", { name: `Update ${COUNT} tools?` });
    await waitFor(() => expect(rowsOf(dialog)).toHaveLength(COUNT));
    // The sheet holds the focus while Update is off, as it prepares.
    dialog.focus();

    commits = [];
    rerender(watched(ready));
    // At once: what it asks and the count on Update are the plans' --
    // the refused one left out -- and Update has the focus.
    expect(dialog).toHaveAccessibleName(`Update ${COUNT - 1} tools?`);
    const update = within(dialog).getByRole("button", { name: `Update ${COUNT - 1} Tools` });
    expect(update).toBeEnabled();
    await waitFor(() => expect(document.activeElement).toBe(update));
    // Its first tools as they are now, the refusal first; every tool still there.
    expect(rowsOf(dialog)).toHaveLength(COUNT);
    expect(namesOf(dialog).slice(0, TOOLS_DRAWN_FIRST)).toEqual(finalOrder.slice(0, TOOLS_DRAWN_FIRST));
    expect(within(rowsOf(dialog)[0]).getByRole("alert")).toHaveTextContent(`Couldn't prepare the update: ${REFUSED} is pinned`);

    await waitFor(() => expect(namesOf(dialog)).toEqual(finalOrder));
    await waitFor(() => expect(sayingOf(dialog)).toHaveLength(1 + names.filter(noted).length));
    // A turn at a time: every tool there throughout, the first few saying
    // what they have to at once, and never more than a turn's after that.
    expect(commits[0].saying).toBeLessThanOrEqual(TOOLS_DRAWN_FIRST);
    for (let i = 0; i < commits.length; i += 1) {
      expect(commits[i].rows).toBe(COUNT);
      if (i > 0) expect(commits[i].saying - commits[i - 1].saying).toBeLessThanOrEqual(TOOLS_DRAWN_PER_TURN);
    }
    expect(new Set(commits.map(({ saying }) => saying)).size).toBeGreaterThanOrEqual(2);
  });

  it("ends as when it drew every tool at once: the same list, and the same order for the keyboard", async () => {
    // Drawn as it opens and again once its plans are back...
    const turned = renderWithProviders(<UpdateConfirmDialog confirm={confirmOf(planning)} />);
    const first = turned.getByRole("alertdialog");
    await waitFor(() => expect(rowsOf(first)).toHaveLength(COUNT));
    turned.rerender(<UpdateConfirmDialog confirm={confirmOf(ready)} />);
    await waitFor(() => expect(namesOf(first)).toEqual(finalOrder));
    await waitFor(() => expect(sayingOf(first)).toHaveLength(1 + names.filter(noted).length));
    const listHtml = html(first);
    // What takes the focus, in the order Tab goes, by what it is about.
    const focusOrder = (dialog: HTMLElement) =>
      [...dialog.querySelectorAll<HTMLElement>("button:not([disabled]), [tabindex='0']")].map(
        (element) =>
          element.closest("[data-sheet-tool]")?.querySelector("[data-sheet-name]")?.textContent ??
          element.getAttribute("aria-label") ??
          element.textContent,
      );
    const order = focusOrder(first);
    // The list itself, then each note's ⓘ by its tool, in the list's order, then the rest.
    expect(order.slice(1, 1 + names.filter(noted).length)).toEqual(names.filter(noted));
    turned.unmount();

    // ...and drawn once its plans are back, from the start.
    const whole = renderWithProviders(<UpdateConfirmDialog confirm={confirmOf({ ...ready, id: 2 })} />);
    const second = whole.getByRole("alertdialog");
    await waitFor(() => expect(namesOf(second)).toEqual(finalOrder));
    expect(html(second)).toBe(listHtml);
    expect(focusOrder(second)).toEqual(order);
  });

  it("parts its tools by the same hairline as before, every one but the first's drawn by the tool, at every turn", async () => {
    const lines = ["relative", "before:absolute", "before:left-10.5", "before:right-2.5", "before:top-0", "before:h-px", "before:bg-group-separator"];
    const parted = (dialog: HTMLElement) => rowsOf(dialog).map((row) => lines.every((line) => row.classList.contains(line)));
    // The first has none of it.
    const plain = (dialog: HTMLElement) => lines.every((line) => !rowsOf(dialog)[0].classList.contains(line));
    const { getByRole, rerender } = renderWithProviders(watched(planning));
    const dialog = getByRole("alertdialog");
    const list = dialog.querySelector<HTMLElement>("[data-sheet-tools]")!;
    // The list draws none itself: no rule over "every tool after another".
    expect([...list.classList].filter((name) => name.includes("*+*"))).toEqual([]);
    const check = () => {
      expect(parted(dialog)).toEqual(rowsOf(dialog).map((_, index) => index > 0));
      expect(plain(dialog)).toBe(true);
    };
    check();
    await waitFor(() => expect(rowsOf(dialog)).toHaveLength(COUNT));
    check();
    rerender(watched(ready));
    check();
    await waitFor(() => expect(namesOf(dialog)).toEqual(finalOrder));
    check();
  });

  it("tells a screen reader its list is busy while any tool is still to be drawn, as it opens and once its plans are back", async () => {
    const { getByRole, rerender } = renderWithProviders(watched(planning));
    const dialog = getByRole("alertdialog");
    await waitFor(() => expect(rowsOf(dialog)).toHaveLength(COUNT));
    const list = dialog.querySelector("[data-sheet-tools]")!;
    expect(list).not.toHaveAttribute("aria-busy");
    // Busy from the first drawing until the last tool is drawn.
    const drawings = commits.filter((commit) => commit.rows > 0);
    expect(drawings[0]).toMatchObject({ rows: TOOLS_DRAWN_FIRST, busy: true });
    for (const { rows, busy } of drawings) expect(busy).toBe(rows < COUNT);

    commits = [];
    rerender(watched(ready));
    await waitFor(() => expect(namesOf(dialog)).toEqual(finalOrder));
    await waitFor(() => expect(list).not.toHaveAttribute("aria-busy"));
    // Busy while tools are held as first drawn, not once the last turn is.
    expect(commits[0].busy).toBe(true);
    expect(commits[commits.length - 1].busy).toBe(false);
  });

  it("draws a list that fits as before: whole, at once, as it opens and once its plans are back", async () => {
    const few = names.slice(0, TOOLS_DRAWN_FIRST);
    const { getByRole, rerender } = renderWithProviders(
      <UpdateConfirmDialog confirm={confirmOf({ ...planning, items: planning.items.slice(0, TOOLS_DRAWN_FIRST) })} />,
    );
    const dialog = getByRole("alertdialog");
    expect(namesOf(dialog)).toEqual(few);
    await act(async () => {
      rerender(<UpdateConfirmDialog confirm={confirmOf({ ...ready, items: ready.items.slice(0, TOOLS_DRAWN_FIRST) })} />);
    });
    expect(namesOf(dialog)).toEqual([...few.filter(noted), ...few.filter((name) => !noted(name))]);
    expect(sayingOf(dialog)).toHaveLength(few.filter(noted).length);
  });
});
