import { describe, expect, it, vi } from "vitest";
import { fireEvent, screen, within } from "@testing-library/react";
import { renderWithProviders } from "../test/setup";
import { BUTTON } from "./ui/controls";
import { JUST_UPDATED_SHOWN, JustUpdated, finishedText, justUpdatedOps } from "./JustUpdated";
import type { OpSummary } from "../lib/types";

function upgrade(id: number, name: string, fields: Partial<OpSummary> = {}): OpSummary {
  return {
    id,
    kind: "Upgrade",
    instance_id: "brew:/opt/homebrew",
    artifact_kind: "Formula",
    name,
    status: "Done",
    outcome: "Succeeded",
    argv_preview: [],
    cancel_policy: "KillThenReconcile",
    ...fields,
  };
}

const none = { shownInRows: new Set<number>(), cleared: [], finishedAt: {} };

describe("justUpdatedOps", () => {
  it("takes each tool's newest operation, and only a finished update that succeeded", () => {
    const operations = [
      upgrade(1, "jq"),
      upgrade(5, "jq"),
      upgrade(2, "wget"),
      upgrade(6, "wget", { kind: "Uninstall" }),
      upgrade(3, "glib", { outcome: { Failed: { exit_code: 1, summary: "no bottle" } } }),
      upgrade(4, "gh", { status: "Running", outcome: null }),
      upgrade(7, "fd", { outcome: "Unconfirmed" }),
    ];
    expect(justUpdatedOps(operations, none).map((op) => op.id)).toEqual([5]);
  });

  it("leaves out what a row still shows and what Clear took off", () => {
    const operations = [upgrade(1, "jq"), upgrade(2, "wget"), upgrade(3, "glib")];
    const listed = justUpdatedOps(operations, { ...none, shownInRows: new Set([1]), cleared: [2] });
    expect(listed.map((op) => op.id)).toEqual([3]);
  });

  it("puts the newest finished first, and the ones it did not see finish after them, newest first", () => {
    const operations = [upgrade(1, "jq"), upgrade(2, "wget"), upgrade(3, "glib"), upgrade(4, "gh")];
    const listed = justUpdatedOps(operations, { ...none, finishedAt: { 3: 1000, 2: 3000 } });
    expect(listed.map((op) => op.id)).toEqual([2, 3, 4, 1]);
  });
});

describe("finishedText", () => {
  const morning = new Date(2026, 8, 28, 8, 5).getTime();

  it("says the time for today, and the date and time in full in its title", () => {
    const now = new Date(2026, 8, 28, 23, 59).getTime();
    expect(finishedText(morning, now, "zh-CN")).toEqual({
      text: new Intl.DateTimeFormat("zh-CN", { timeStyle: "short" }).format(morning),
      title: new Intl.DateTimeFormat("zh-CN", { dateStyle: "medium", timeStyle: "short" }).format(morning),
      today: true,
    });
  });

  it("says the date for another day: 9月28日, Sep 28", () => {
    const nextDay = new Date(2026, 8, 29, 0, 1).getTime();
    expect(finishedText(morning, nextDay, "en")).toMatchObject({ text: "Sep 28", today: false });
    expect(finishedText(morning, nextDay, "zh-CN").text).toBe("9月28日");
  });
});

describe("JustUpdated", () => {
  const entry = {
    id: "op:4",
    opId: 4,
    key: { instance_id: "brew:/opt/homebrew", kind: "Formula" as const, name: "git" },
    adapterId: "brew",
    sourceLabel: "Homebrew",
    name: "git",
    version: "2.55.1",
    finishedAt: Date.now(),
    verified: false,
  };

  it("is a grouped container under its 13 bold title, with a small grey Clear beside the title", () => {
    const onClear = vi.fn();
    renderWithProviders(<JustUpdated entries={[entry]} onClear={onClear} />);

    const section = screen.getByRole("region", { name: "Recently Updated" });
    const title = within(section).getByRole("heading", { name: "Recently Updated" });
    expect(title).toHaveClass("text-title");
    const clear = within(section).getByRole("button", { name: "Clear the Recently Updated list" });
    expect(clear.className).toBe(BUTTON.small.grey);
    // Beside the title, not at the far end.
    expect(clear.parentElement).toBe(title.parentElement);
    expect(title.parentElement?.className).not.toMatch(/justify-between/);
    // No card: no edge and no white; the group's fill and corners.
    expect(section.className).not.toMatch(/border|bg-surface/);
    const list = within(section).getByRole("list");
    expect(list).toHaveClass("bg-group", "rounded-group");
    fireEvent.click(clear);
    expect(onClear).toHaveBeenCalledTimes(1);
  });

  it("draws each tool as a quiet 28 line: a 20 icon, the name in 13, then 11 for the rest", () => {
    renderWithProviders(<JustUpdated entries={[entry]} onClear={() => {}} />);

    const line = screen.getByRole("listitem");
    expect(line).toHaveClass("h-7");
    expect(line.querySelector("[aria-hidden]")?.className).toMatch(/h-5 w-5/);
    expect(within(line).getByText("git")).toHaveClass("text-body");
    expect(within(line).getByText("2.55.1")).toHaveClass("text-small", "text-muted");
    const done = within(line).getByText("Updated");
    expect(done).toHaveClass("text-small", "text-foreground");
    expect(done.querySelector("svg")).toHaveAttribute("width", "12");
    expect(line.querySelector("time")?.parentElement).toHaveClass("text-small", "text-muted");
  });

  it("says each line's source to a screen reader, so two copies of one tool are two lines apart", () => {
    const npm = {
      ...entry,
      id: "op:5",
      opId: 5,
      key: { ...entry.key, instance_id: "npm:/opt/homebrew" },
      adapterId: "npm",
      sourceLabel: "npm",
    };
    renderWithProviders(<JustUpdated entries={[entry, npm]} onClear={() => {}} />);

    const lines = screen.getAllByRole("listitem");
    expect(lines.map((line) => line.querySelector("[data-just-updated-source]")?.textContent)).toEqual([
      "Homebrew",
      "npm",
    ]);
    // Heard, not seen: the avatar's mark says it in sight.
    expect(within(lines[1]).getByText("npm")).toHaveClass("sr-only");
  });

  it("says Today and the time for one that finished today, and Verified where Banager read the change", () => {
    renderWithProviders(<JustUpdated entries={[{ ...entry, verified: true }]} onClear={() => {}} />);

    const line = screen.getByRole("listitem");
    const time = line.querySelector("time");
    expect(time?.textContent).toBe(
      `Today ${new Intl.DateTimeFormat("en", { timeStyle: "short" }).format(entry.finishedAt)}`,
    );
    const done = within(line).getByText("Verified");
    expect(done).toHaveAttribute(
      "title",
      "The installed version was read before and after the update, and it had changed.",
    );
    expect(done.querySelector("svg")).toHaveAttribute("width", "12");
    expect(within(line).queryByText("Updated")).toBeNull();
  });

  it("says for a model that the model, not a version, was read and had changed", () => {
    const model = {
      ...entry,
      key: { instance_id: "ollama:http://127.0.0.1:11434", kind: "Model" as const, name: "qwen3:8b" },
      adapterId: "ollama",
      name: "qwen3:8b",
      version: null,
      verified: true,
    };
    renderWithProviders(<JustUpdated entries={[model]} onClear={() => {}} />);
    expect(screen.getByText("Verified")).toHaveAttribute(
      "title",
      "The model was read before and after the update, and it had changed.",
    );
  });

  it(`shows the newest ${JUST_UPDATED_SHOWN} and folds the rest under a line that shows them`, () => {
    const many = Array.from({ length: JUST_UPDATED_SHOWN + 3 }, (_, i) => ({
      ...entry,
      id: `op:${i}`,
      opId: i,
      name: `tool${i}`,
      key: { ...entry.key, name: `tool${i}` },
    }));
    renderWithProviders(<JustUpdated entries={many} onClear={() => {}} />);

    expect(screen.getAllByRole("listitem")).toHaveLength(JUST_UPDATED_SHOWN);
    expect(screen.getByText(`tool${JUST_UPDATED_SHOWN - 1}`)).toBeInTheDocument();
    expect(screen.queryByText(`tool${JUST_UPDATED_SHOWN}`)).toBeNull();
    const more = screen.getByRole("button", { name: "3 More" });
    expect(more).toHaveAttribute("aria-expanded", "false");
    expect(more).toHaveClass("text-small", "text-muted");

    fireEvent.click(more);
    expect(screen.getAllByRole("listitem")).toHaveLength(JUST_UPDATED_SHOWN + 3);
    const fewer = screen.getByRole("button", { name: "Show Fewer" });
    expect(fewer).toHaveAttribute("aria-expanded", "true");
    fireEvent.click(fewer);
    expect(screen.getAllByRole("listitem")).toHaveLength(JUST_UPDATED_SHOWN);
  });

  it(`has no fold line for ${JUST_UPDATED_SHOWN} or fewer`, () => {
    const few = Array.from({ length: JUST_UPDATED_SHOWN }, (_, i) => ({ ...entry, id: `op:${i}`, opId: i }));
    renderWithProviders(<JustUpdated entries={few} onClear={() => {}} />);
    expect(screen.getAllByRole("listitem")).toHaveLength(JUST_UPDATED_SHOWN);
    expect(screen.queryByRole("button", { name: /More|Show Fewer/ })).toBeNull();
  });
});
