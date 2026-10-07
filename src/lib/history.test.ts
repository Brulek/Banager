import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { failureCause, type FailureCause } from "./failureCause";
import { RECENT_DAYS, clearedHere, recentUpdates, verifiedHere } from "./history";
import { artifactKeyId } from "../store/ui";
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

/** The tools the last check still offers an update for, by name. */
function offered(...names: string[]): Set<string> {
  return new Set(names.map((name) => artifactKeyId(record(name).key)));
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
      '{"run":"run1","op_id":4,"finished_at":1790000000000,"key":{"instance_id":"brew:/opt/homebrew","kind":"Formula","name":"cmake"},"display_name":"cmake","adapter_id":"brew","kind":"Update","from_version":"3.31.6","to_version":"4.0.0","result":"Succeeded","verified":true,"dismissed":false}';
    const parsed: HistoryRecord = JSON.parse(wire);
    expect(parsed).toEqual(
      record("cmake", {
        run: "run1",
        op_id: 4,
        finished_at: 1_790_000_000_000,
        from_version: "3.31.6",
        to_version: "4.0.0",
        dismissed: false,
      }),
    );
    const failed: HistoryRecord["result"] = JSON.parse('{"Failed":{"cause":"diskFull"}}');
    expect(failed).toEqual({ Failed: { cause: "diskFull" } });
    // r6 y3-batch: a line where no cause is named, and how an update
    // already at its target was done (Rust's
    // test_every_failure_keeps_a_cause_and_one_no_cause_names_keeps_its_first_error_line,
    // test_an_update_already_at_its_target_is_kept_as_succeeded_and_says_how).
    const other: HistoryRecord["result"] = JSON.parse('{"Failed":{"cause":null,"detail":"SHA256 mismatch"}}');
    expect(other).toEqual({ Failed: { cause: null, detail: "SHA256 mismatch" } });
    const appMissing: HistoryRecord["result"] = JSON.parse('{"Failed":{"cause":"appMissing"}}');
    expect(appMissing).toEqual({ Failed: { cause: "appMissing" } });
    const already: HistoryRecord = JSON.parse(
      wire.replace('"verified":true', '"verified":false,"already_updated":"ByEarlierUpdate"'),
    );
    expect(already.already_updated).toBe("ByEarlierUpdate");
    expect(JSON.parse(JSON.stringify(already))).toEqual(already);
    // Rust writes the warnings after `verified` and before `dismissed`.
    const warned: HistoryRecord = JSON.parse(wire.replace('"verified":true,',
      '"verified":true,"follow_up_warnings":[{"OldVersionsNotCleanedUp":{"name":"cmake","exit_code":1}},{"NoLongerLinked":{"name":"cmake","commands":["cmake"]}}],'));
    expect(warned.result).toBe("Succeeded");
    expect(warned.follow_up_warnings).toHaveLength(2);
    expect(JSON.parse(JSON.stringify(warned))).toEqual(warned);
    expect(parsed.follow_up_warnings ?? []).toEqual([]);
    expect(recentUpdates(view([warned]), [], warned.finished_at + DAY, new Set())).toEqual([warned]);
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
  it("matches a redacted Ollama history key to live operations and offered updates", () => {
    const key = { instance_id: "ollama:http://server:11434", kind: "Model", name: "llama3:latest" } as const;
    const kept = record(key.name, { key, adapter_id: "ollama", result: { Failed: { cause: "network" } } });
    for (const login of ["alice:secret", "alice:s%40cret", "token", ":secret"]) {
      const liveId = `ollama:http://${login}@server:11434`;
      const offered = new Set([artifactKeyId({ ...key, instance_id: liveId })]);
      expect(recentUpdates(view([kept]), [], NOW, offered)).toEqual([kept]);
      const live = { ...op(7, key.name), instance_id: liveId, artifact_kind: key.kind };
      expect(recentUpdates(view([kept]), [live], NOW, offered)).toEqual([]);
      const elsewhere = new Set([artifactKeyId({ ...key, instance_id: "ollama:http://other:11434" })]);
      expect(recentUpdates(view([kept]), [], NOW, elsewhere)).toEqual([]);
    }
  });

  it("lists each tool's newest update that worked, newest first", () => {
    const listed = recentUpdates(
      view([
        record("cmake", { finished_at: NOW - 3 * DAY, to_version: "3.0" }),
        record("cmake", { finished_at: NOW - DAY, to_version: "4.0" }),
        record("git", { finished_at: NOW - 2 * DAY }),
      ]),
      [],
      NOW,
      offered(),
    );
    expect(listed.map((r) => [r.key.name, r.to_version])).toEqual([
      ["cmake", "4.0"],
      ["git", "2.0"],
    ]);
  });

  it(`lists nothing older than ${RECENT_DAYS} days, nothing from before Clear, no uninstall and no update cancelled`, () => {
    const listed = recentUpdates(
      view(
        [
          record("old", { finished_at: NOW - 31 * DAY }),
          record("cleared", { finished_at: NOW - 5 * DAY }),
          record("oldFailure", { finished_at: NOW - 31 * DAY, result: { Failed: { cause: "network" } } }),
          record("clearedFailure", { finished_at: NOW - 5 * DAY, result: { Failed: { cause: null } } }),
          record("stopped", { result: "Cancelled" }),
          // Updated, then uninstalled: its newest is the uninstall.
          record("gone", { finished_at: NOW - 3 * DAY }),
          record("gone", { kind: "Uninstall", finished_at: NOW - 2 * DAY }),
          record("removed", { kind: "Uninstall", finished_at: NOW - 2 * DAY, result: { Failed: { cause: "permission" } } }),
          record("kept", { finished_at: NOW - 3 * DAY }),
        ],
        { cleared_before: NOW - 4 * DAY },
      ),
      [],
      NOW,
      // Every one still offered, so that is not what leaves them out.
      offered("old", "cleared", "oldFailure", "clearedFailure", "stopped", "gone", "removed", "kept"),
    );
    expect(listed.map((r) => r.key.name)).toEqual(["kept"]);
  });

  it("lists an update that failed or asks to be checked too, in the same order, with what the history kept of it", () => {
    const listed = recentUpdates(
      view([
        record("failed", { finished_at: NOW - 2 * DAY, to_version: null, result: { Failed: { cause: "needsPassword" } } }),
        record("unchanged", { finished_at: NOW - DAY, result: { NeedsAttention: "UnchangedAfterUpgrade" } }),
        record("unconfirmed", { finished_at: NOW - 4 * DAY, result: "Unconfirmed" }),
        record("worked", { finished_at: NOW - 3 * DAY }),
      ]),
      [],
      NOW,
      offered("failed", "unchanged", "unconfirmed"),
    );
    expect(listed.map((r) => [r.key.name, r.result])).toEqual([
      ["unchanged", { NeedsAttention: "UnchangedAfterUpgrade" }],
      ["failed", { Failed: { cause: "needsPassword" } }],
      ["worked", "Succeeded"],
      ["unconfirmed", "Unconfirmed"],
    ]);
  });

  it("leaves out a failure once the tool's update has worked since, and lists a failure after a success", () => {
    const failed = { to_version: null, result: { Failed: { cause: "network" } } } as const;
    const listed = recentUpdates(
      view([
        record("cmake", { finished_at: NOW - 3 * DAY, ...failed }),
        record("cmake", { finished_at: NOW - 2 * DAY, to_version: "4.0" }),
        record("git", { finished_at: NOW - 3 * DAY }),
        record("git", { finished_at: NOW - DAY, ...failed }),
        // Failed, then cancelled: the cancel is its newest, and nothing is listed.
        record("jq", { finished_at: NOW - 3 * DAY, ...failed }),
        record("jq", { finished_at: NOW - DAY, result: "Cancelled" }),
      ]),
      [],
      NOW,
      offered("cmake", "git", "jq"),
    );
    expect(listed.map((r) => [r.key.name, r.result])).toEqual([
      ["git", { Failed: { cause: "network" } }],
      ["cmake", "Succeeded"],
    ]);
  });

  it("lists no tool this window has an operation of: that operation decides, so nothing is listed twice", () => {
    const history = view([
      record("cmake", { run: "now", op_id: 7 }),
      record("git", { run: "earlier", op_id: 7, finished_at: NOW - 9 * DAY }),
      record("jq"),
    ]);
    expect(recentUpdates(history, [op(7, "cmake"), op(8, "git")], NOW, offered()).map((r) => r.key.name)).toEqual([
      "jq",
    ]);
  });

  it("lists a failure or one to check only while the last check still offers that tool an update", () => {
    // Each updated in Terminal since, or uninstalled outside Banager: no
    // update offered any more, so 「未能更新」 would no longer be known true.
    const history = view([
      record("failed", { to_version: null, result: { Failed: { cause: "network" } } }),
      record("unchanged", { finished_at: NOW - 2 * DAY, result: { NeedsAttention: "UnchangedAfterUpgrade" } }),
      record("unconfirmed", { finished_at: NOW - 3 * DAY, result: "Unconfirmed" }),
      record("worked", { finished_at: NOW - 4 * DAY }),
    ]);
    expect(recentUpdates(history, [], NOW, offered()).map((r) => r.key.name)).toEqual(["worked"]);
    // A success is listed whether or not a newer update is offered.
    expect(recentUpdates(history, [], NOW, offered("failed", "worked")).map((r) => r.key.name)).toEqual([
      "failed",
      "worked",
    ]);
    // Offered under another source is not this tool.
    const elsewhere = new Set([artifactKeyId({ instance_id: "npm:/usr/local", kind: "Formula", name: "failed" })]);
    expect(recentUpdates(history, [], NOW, elsewhere).map((r) => r.key.name)).toEqual(["worked"]);
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

  it("finds whether a kept Clear came after this launch's operation finished, by its record", () => {
    const records = [
      record("cmake", { run: "now", op_id: 7, finished_at: NOW - 2_000 }),
      record("git", { run: "now", op_id: 8, finished_at: NOW }),
      record("jq", { run: "earlier", op_id: 9, finished_at: NOW - 2_000 }),
    ];
    const cleared = view(records, { cleared_before: NOW - 1_000 });
    expect(clearedHere(cleared, 7)).toBe(true);
    expect(clearedHere(cleared, 8)).toBe(false);
    // Another launch's op 9 is not this window's op 9.
    expect(clearedHere(cleared, 9)).toBe(false);
    // No record yet, or never cleared: not hidden by this.
    expect(clearedHere(cleared, 10)).toBe(false);
    expect(clearedHere(view(records), 7)).toBe(false);
  });
});

describe("persisted dismissal after a backward clock correction", () => {
  it("reads explicit true and false from the Rust wire and ignores the legacy cutoff", () => {
    const old = { ...record("old", { run: "now", op_id: 1, finished_at: NOW + DAY }), dismissed: true };
    const fresh = { ...record("fresh", { run: "now", op_id: 2, finished_at: NOW }), dismissed: false };
    const wire = JSON.stringify(view([old, fresh], { cleared_before: NOW + DAY }));
    const parsed: HistoryView = JSON.parse(wire);
    expect(JSON.stringify(parsed)).toBe(wire);
    expect(recentUpdates(parsed, [], NOW, offered()).map((r) => r.key.name)).toEqual(["fresh"]);
    expect(clearedHere(parsed, 1)).toBe(true);
    expect(clearedHere(parsed, 2)).toBe(false);
  });
  it("ignores an out-of-range date before choosing the latest record for a tool", () => {
    const good = record("git");
    const bad = record("git", { finished_at: 9_000_000_000_000_000 });
    expect(recentUpdates(view([good, bad]), [], NOW, offered())).toEqual([good]);
  });
});

it("a dismissed future-dated record cannot shadow the same tool updated after Clear", () => {
  const old = { ...record("git", { finished_at: NOW + DAY }), dismissed: true };
  const fresh = { ...record("git", { finished_at: NOW }), dismissed: false };
  expect(recentUpdates(view([old, fresh], { cleared_before: NOW + DAY }), [], NOW, offered())).toEqual([fresh]);
});

describe("a recorded password stop is not resolved by Clear (r22 W1)", () => {
  const stop = { Failed: { cause: "needsPassword" } } as const;

  it("lists a dismissed stop when dismissal is ignored, as `get_history` sends it after Clear", () => {
    const cleared = { ...record("onyx", { result: stop, to_version: null, verified: false }), dismissed: true };
    expect(recentUpdates(view([cleared], { cleared_before: NOW }), [], NOW, offered("onyx"))).toEqual([]);
    expect(recentUpdates(view([cleared], { cleared_before: NOW }), [], NOW, offered("onyx"), { includeDismissed: true })).toEqual([cleared]);
    // Legacy wire data (no `dismissed`, a cutoff) reads the same way.
    const legacy = record("onyx", { result: stop, to_version: null, verified: false });
    expect(recentUpdates(view([legacy], { cleared_before: NOW }), [], NOW, offered("onyx"), { includeDismissed: true })).toEqual([legacy]);
  });

  it("still lets a later update supersede the stop, cleared or not", () => {
    const earlier = { ...record("onyx", { finished_at: NOW - 2_000, result: stop }), dismissed: true };
    const later = { ...record("onyx", { finished_at: NOW - 1_000 }), dismissed: true };
    expect(recentUpdates(view([later, earlier]), [], NOW, offered("onyx"), { includeDismissed: true })).toEqual([later]);
  });

  it("never lets a dismissed future-dated stop shadow an update finished after Clear", () => {
    // The clock was ahead when the stop was kept, then put right; the
    // update after Clear has an earlier time but came later.
    const old = { ...record("onyx", { finished_at: NOW + DAY, result: stop }), dismissed: true };
    const fresh = { ...record("onyx", { finished_at: NOW }), dismissed: false };
    expect(recentUpdates(view([fresh, old], { cleared_before: NOW + DAY }), [], NOW, offered("onyx"), { includeDismissed: true })).toEqual([fresh]);
  });
});
