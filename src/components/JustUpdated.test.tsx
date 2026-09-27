import { describe, expect, it } from "vitest";
import { finishedText, justUpdatedOps } from "./JustUpdated";
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
    });
  });

  it("says the date for another day", () => {
    const nextDay = new Date(2026, 8, 29, 0, 1).getTime();
    expect(finishedText(morning, nextDay, "en").text).toBe(
      new Intl.DateTimeFormat("en", { month: "numeric", day: "numeric" }).format(morning),
    );
  });
});
