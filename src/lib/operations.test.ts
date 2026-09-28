import { describe, expect, it } from "vitest";
import {
  cancelState,
  currentOf,
  isWaitingForBrewUpdate,
  outcomeTone,
  runsToItsEnd,
  trackRun,
  type OperationRun,
} from "./operations";
import type { LogLine } from "../store/ui";
import type { OpStatus, OpSummary } from "./types";

function op(id: number, status: OpStatus, extra: Partial<OpSummary> = {}): OpSummary {
  return {
    id,
    kind: "Upgrade",
    instance_id: "brew:/opt/homebrew",
    artifact_kind: "Formula",
    name: `tool-${id}`,
    status,
    outcome: status === "Done" ? "Succeeded" : null,
    argv_preview: [],
    cancel_policy: "KillThenReconcile",
    ...extra,
  };
}

/** The operations `run` covers. */
function inRun(run: OperationRun, operations: OpSummary[]): number[] {
  return operations.filter((each) => each.id > run.floor).map((each) => each.id);
}

describe("trackRun", () => {
  it("starts a run with the first operation of the session", () => {
    const idle = trackRun(null, []);
    const running = [op(1, "Running")];
    expect(inRun(trackRun(idle, running), running)).toEqual([1]);
  });

  it("keeps operations started while others run in the same run", () => {
    let run = trackRun(null, []);
    let list = [op(1, "Running")];
    run = trackRun(run, list);
    list = [op(3, "Queued"), op(2, "Queued"), op(1, "Running")];
    run = trackRun(run, list);
    list = [op(3, "Running"), op(2, "Done"), op(1, "Done")];
    run = trackRun(run, list);
    expect(inRun(run, list)).toEqual([3, 2, 1]);
    list = [op(3, "Done"), op(2, "Done"), op(1, "Done")];
    run = trackRun(run, list);
    // Done: still the same run, to say what it came to.
    expect(inRun(run, list)).toEqual([3, 2, 1]);
  });

  it("begins a new run with the first operation after everything had finished", () => {
    let list = [op(2, "Done"), op(1, "Done")];
    let run = trackRun(trackRun(null, []), [op(2, "Running"), op(1, "Running")]);
    run = trackRun(run, list);
    list = [op(3, "Queued"), op(2, "Done"), op(1, "Done")];
    run = trackRun(run, list);
    expect(inRun(run, list)).toEqual([3]);
  });

  it("begins a new run for an operation that had already finished by the time it was seen", () => {
    let list = [op(1, "Done")];
    let run = trackRun(trackRun(null, []), [op(1, "Running")]);
    run = trackRun(run, list);
    // Started and finished between two looks at the list.
    list = [op(2, "Done"), op(1, "Done")];
    run = trackRun(run, list);
    expect(inRun(run, list)).toEqual([2]);
  });

  it("takes the newest operation alone at a first look with nothing under way, and what is under way otherwise", () => {
    const finished = [op(5, "Done"), op(4, "Done")];
    expect(inRun(trackRun(null, finished), finished)).toEqual([5]);
    const busy = [op(7, "Queued"), op(6, "Running"), op(5, "Done")];
    expect(inRun(trackRun(null, busy), busy)).toEqual([7, 6]);
  });

  it("hands back the same run when nothing changed, so a component can keep it while rendering", () => {
    const list = [op(2, "Queued"), op(1, "Running")];
    const run = trackRun(trackRun(null, []), list);
    expect(trackRun(run, [...list])).toBe(run);
  });
});

describe("currentOf", () => {
  it("names the oldest operation actually working, ahead of older ones waiting their turn", () => {
    expect(currentOf([op(3, "Queued"), op(2, "Running"), op(1, "Queued")])?.id).toBe(2);
    expect(currentOf([op(3, "Verifying"), op(2, "Running")])?.id).toBe(2);
    expect(currentOf([op(3, "Queued"), op(2, "Queued")])?.id).toBe(2);
    expect(currentOf([])).toBeUndefined();
  });
});

describe("runsToItsEnd", () => {
  it("is true of a NoCancel operation from the moment it starts until it is done, and of nothing else", () => {
    const noCancel = { cancel_policy: "NoCancel" } as const;
    expect(runsToItsEnd(op(1, "Running", noCancel))).toBe(true);
    expect(runsToItsEnd(op(1, "Verifying", noCancel))).toBe(true);
    // Waiting its turn, it has started nothing, and can still be cancelled.
    expect(runsToItsEnd(op(1, "Queued", noCancel))).toBe(false);
    expect(runsToItsEnd(op(1, "Done", noCancel))).toBe(false);
    expect(runsToItsEnd(op(1, "Running"))).toBe(false);
  });
});

describe("cancelState", () => {
  it("offers Cancel while an operation runs, except once a NoCancel one has started", () => {
    expect(cancelState(op(1, "Running"))).toBe("enabled");
    expect(cancelState(op(1, "Queued", { cancel_policy: "NoCancel" }))).toBe("enabled");
    expect(cancelState(op(1, "Running", { cancel_policy: "NoCancel" }))).toBe("none");
    expect(cancelState(op(1, "Done"))).toBe("none");
  });

  it("keeps it on screen but unpressable while a cancel is on its way or the result is being checked", () => {
    expect(cancelState(op(1, "CancelRequested"))).toBe("disabled");
    expect(cancelState(op(1, "Cancelling"))).toBe("disabled");
    expect(cancelState(op(1, "Verifying"))).toBe("disabled");
  });
});

describe("isWaitingForBrewUpdate", () => {
  const waiting: LogLine = { opId: 1, note: { WaitingForBrewUpdate: { minutes: 10 } }, seq: 1 };
  const printed: LogLine = { opId: 1, stream: "Stdout", line: "==> Upgrading jq", seq: 2 };
  const other: LogLine = { opId: 2, stream: "Stdout", line: "something else", seq: 3 };

  it("is true while the operation's last word is the wait, whatever other operations print", () => {
    expect(isWaitingForBrewUpdate(op(1, "Running"), [waiting, other])).toBe(true);
  });

  it("is false once the tool has printed anything after the wait, or once the operation is no longer Running", () => {
    expect(isWaitingForBrewUpdate(op(1, "Running"), [waiting, printed])).toBe(false);
    expect(isWaitingForBrewUpdate(op(1, "CancelRequested"), [waiting])).toBe(false);
    expect(isWaitingForBrewUpdate(op(1, "Running"), [])).toBe(false);
  });
});

describe("outcomeTone", () => {
  it("sorts every ending by what the bar does with it", () => {
    expect(outcomeTone("Succeeded")).toBe("success");
    expect(outcomeTone("Cancelled")).toBe("cancelled");
    expect(outcomeTone("Unconfirmed")).toBe("attention");
    expect(outcomeTone({ NeedsAttention: "GoneAfterUpgrade" })).toBe("attention");
    expect(outcomeTone({ Failed: { exit_code: 1, summary: "" } })).toBe("failure");
    expect(outcomeTone({ CanagerFailed: "Internal" })).toBe("failure");
    // Never sent, and so claims nothing either way.
    expect(outcomeTone(null)).toBe("attention");
  });
});
