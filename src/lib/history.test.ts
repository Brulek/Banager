import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { failureCause, type FailureCause } from "./failureCause";
import { RECENT_DAYS, recentUpdates, verifiedHere } from "./history";
import type { HistoryRecord, HistoryView, OpSummary } from "./types";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const DAY = 24 * 60 * 60 * 1000;
const NOW = new Date(2026, 9, 1, 12, 0).getTime();

function record(name: string, fields: Partial<HistoryRecord> = {}): HistoryRecord {
  return {
    run: "earlier",
    op_id: 1,
    finished_at: NOW - DAY,
    key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name },
    display_name: name,
    adapter_id: "brew",
    kind: "Update",
    from_version: "1.0",
    to_version: "2.0",
    result: "Succeeded",
    verified: true,
    ...fields,
  };
}

function view(records: HistoryRecord[], fields: Partial<HistoryView> = {}): HistoryView {
  return { run: "now", cleared_before: null, records, ...fields };
}

function op(id: number, name: string): OpSummary {
  return {
    id,
    kind: "Upgrade",
    instance_id: "brew:/opt/homebrew",
    artifact_kind: "Formula",
    name,
    status: "Done",
    outcome: "Succeeded",
    argv_preview: [],
    env_preview: [],
    cancel_policy: "KillThenReconcile",
  };
}

describe("the history's wire shape", () => {
  it("reads the record Rust writes (test_record_wire_shape_matches_the_hand_written_ts_mirror)", () => {
    const wire =
      '{"run":"run1","op_id":4,"finished_at":1790000000000,"key":{"instance_id":"brew:/opt/homebrew","kind":"Formula","name":"cmake"},"display_name":"cmake","adapter_id":"brew","kind":"Update","from_version":"3.31.6","to_version":"4.0.0","result":"Succeeded","verified":true}';
    const parsed: HistoryRecord = JSON.parse(wire);
    expect(parsed).toEqual(
      record("cmake", {
        run: "run1",
        op_id: 4,
        finished_at: 1_790_000_000_000,
        from_version: "3.31.6",
        to_version: "4.0.0",
      }),
    );
    const failed: HistoryRecord["result"] = JSON.parse('{"Failed":{"cause":"diskFull"}}');
    expect(failed).toEqual({ Failed: { cause: "diskFull" } });
  });

  it("names a failure's cause as the window does: the cases Rust's reading is tested on", () => {
    const cases: Array<{ name: string; text: string; cause: FailureCause | null }> = JSON.parse(
      readFileSync(path.join(ROOT, "crates/banager-core/src/history/failure_cause_cases.json"), "utf-8"),
    );
    expect(cases.length).toBeGreaterThanOrEqual(30);
    for (const { name, text, cause } of cases) {
      expect(failureCause(text), name).toBe(cause);
    }
  });
});

describe("recentUpdates", () => {
  it("lists each tool's newest update that worked, newest first", () => {
    const listed = recentUpdates(
      view([
        record("cmake", { finished_at: NOW - 3 * DAY, to_version: "3.0" }),
        record("cmake", { finished_at: NOW - DAY, to_version: "4.0" }),
        record("git", { finished_at: NOW - 2 * DAY }),
      ]),
      [],
      NOW,
    );
    expect(listed.map((r) => [r.key.name, r.to_version])).toEqual([
      ["cmake", "4.0"],
      ["git", "2.0"],
    ]);
  });

  it(`lists nothing older than ${RECENT_DAYS} days, nothing from before Clear, and nothing but an update that worked`, () => {
    const listed = recentUpdates(
      view(
        [
          record("old", { finished_at: NOW - 31 * DAY }),
          record("cleared", { finished_at: NOW - 5 * DAY }),
          record("failed", { result: { Failed: { cause: "network" } } }),
          record("checkit", { result: { NeedsAttention: "UnchangedAfterUpgrade" } }),
          // Updated, then uninstalled: its newest is the uninstall.
          record("gone", { finished_at: NOW - 3 * DAY }),
          record("gone", { kind: "Uninstall", finished_at: NOW - 2 * DAY }),
          record("kept", { finished_at: NOW - 3 * DAY }),
        ],
        { cleared_before: NOW - 4 * DAY },
      ),
      [],
      NOW,
    );
    expect(listed.map((r) => r.key.name)).toEqual(["kept"]);
  });

  it("lists no tool this window has an operation of: that operation decides, so nothing is listed twice", () => {
    const history = view([
      record("cmake", { run: "now", op_id: 7 }),
      record("git", { run: "earlier", op_id: 7, finished_at: NOW - 9 * DAY }),
      record("jq"),
    ]);
    expect(recentUpdates(history, [op(7, "cmake"), op(8, "git")], NOW).map((r) => r.key.name)).toEqual(["jq"]);
  });

  it("finds whether this launch's operation was verified, by its run and id", () => {
    const history = view([
      record("cmake", { run: "now", op_id: 7 }),
      record("git", { run: "earlier", op_id: 8 }),
      record("jq", { run: "now", op_id: 9, verified: false }),
    ]);
    expect(verifiedHere(history, 7)).toBe(true);
    expect(verifiedHere(history, 8)).toBe(false);
    expect(verifiedHere(history, 9)).toBe(false);
  });
});
