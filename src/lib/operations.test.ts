import { describe, expect, it } from "vitest";
import i18n from "../i18n";
import {
  OP_FAILED_KEYS,
  OP_KIND_KEYS,
  OP_RUNNING_KEYS,
  OP_SUCCEEDED_KEYS,
  cancelState,
  currentOf,
  isWaitingForBrewUpdate,
  operationWords,
  outcomeTone,
  outcomeWords,
  runsToItsEnd,
  statusKey,
  trackRun,
  type OperationRun,
} from "./operations";
import type { LogLine } from "../store/ui";
import type { OpKind, OpStatus, OpSummary, Outcome } from "./types";
import { failureCause } from "./failureCause";

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

describe("statusKey", () => {
  it("names the running operation's kind, then its actual phase instead of an old wait note", () => {
    const waiting: LogLine = { opId: 1, note: { WaitingForBrewUpdate: { minutes: 10 } }, seq: 1 };
    for (const [kind, key] of [
      ["Install", "operations.running.Install"],
      ["Uninstall", "operations.running.Uninstall"],
      ["Upgrade", "updates.progress.running"],
    ] as const) {
      expect(statusKey(op(1, "Running", { kind }), [])).toBe(key);
      expect(statusKey(op(1, "Running", { kind }), [waiting])).toBe("operations.status.waitingForBrewUpdate");
      expect(statusKey(op(1, "Running", { kind }), [{ ...waiting, opId: 2 }])).toBe(key);
    }
    for (const status of ["Queued", "CancelRequested", "Cancelling", "Verifying"] as const) {
      expect(statusKey(op(1, status), [waiting])).toBe(`operations.status.${status}`);
    }
    expect(statusKey(op(1, "Done"), [waiting])).toBeNull();
  });
});

describe("outcomeWords", () => {
  const en = i18n.getFixedT("en");

  it("shows the tool's failure only with technical details on, including a recognised cause", () => {
    for (const [kind, plain] of [
      ["Install", "Couldn't install"],
      ["Uninstall", "Couldn't uninstall"],
      ["Upgrade", "Couldn't update"],
    ] as const) {
      const summary = "Error: something odd happened";
      const failed: Outcome = { Failed: { exit_code: 1, summary, cause: failureCause(summary) } };
      expect(outcomeWords(en, failed, kind, false)).toBe(plain);
      expect(outcomeWords(en, failed, kind, true)).toBe(`Couldn't finish: ${summary}`);
      const network = "curl: (6) Could not resolve host: ghcr.io";
      const unreachable: Outcome = { Failed: { exit_code: 1, summary: network, cause: failureCause(network) } };
      expect(outcomeWords(en, unreachable, kind, false)).toBe("Connection failed");
      expect(outcomeWords(en, unreachable, kind, true)).toBe(`Couldn't finish: ${network}`);
    }
  });

  it("says the program gave no reason when its failure summary contains only whitespace", () => {
    for (const summary of ["", " \t\n "]) {
      for (const technical of [false, true]) {
        expect(outcomeWords(en, { Failed: { exit_code: 1, summary, cause: failureCause(summary) } }, "Upgrade", technical)).toBe(
          "Couldn't finish, and the program didn't say why",
        );
      }
    }
  });
});

describe("outcomeTone", () => {
  it("sorts every ending by what the bar does with it", () => {
    expect(outcomeTone("Succeeded")).toBe("success");
    expect(outcomeTone("Cancelled")).toBe("cancelled");
    expect(outcomeTone("Unconfirmed")).toBe("attention");
    expect(outcomeTone({ NeedsAttention: "GoneAfterUpgrade" })).toBe("attention");
    expect(outcomeTone({ Failed: { exit_code: 1, summary: "", cause: failureCause("") } })).toBe("failure");
    expect(outcomeTone({ BanagerFailed: "Internal" })).toBe("failure");
    // Never sent, and so claims nothing either way.
    expect(outcomeTone(null)).toBe("attention");
  });
});

describe("operationWords, for an update already at its target (r6 y3-batch)", () => {
  const en = i18n.getFixedT("en");

  it("says how an update another one already did was done, in place of 「已更新」", () => {
    expect(operationWords(en, op(1, "Done", { already_updated: "ByEarlierUpdate" }), [], false)).toBe(
      "Done by an earlier update",
    );
    expect(operationWords(en, op(1, "Done", { already_updated: "BeforeItsTurn" }), [], false)).toBe(
      "Already up to date",
    );
    // Only of one that is done and worked.
    expect(operationWords(en, op(1, "Done", { already_updated: null }), [], false)).toBe("Updated");
    expect(
      operationWords(
        en,
        op(1, "Done", { already_updated: "BeforeItsTurn", outcome: { NeedsAttention: "UnchangedAfterUpgrade" } }),
        [],
        false,
      ),
    ).toBe("Update reported success, but the version didn't change");
  });
});

describe("operationWords", () => {
  // The words that say what the operation does themselves: running, a
  // plain success, a plain failure, and what did not add up after it.
  // Every other status or outcome word needs the kind in front.
  const namingKind = new Set([
    ...Object.values(OP_RUNNING_KEYS),
    ...Object.values(OP_SUCCEEDED_KEYS),
    ...Object.values(OP_FAILED_KEYS),
  ]);
  const namesKind = (key: string) => namingKind.has(key) || key.startsWith("operations.outcome.NeedsAttention.");
  // Each key as itself, so which words were chosen shows.
  const keyT = (key: string, options?: Record<string, unknown>) =>
    key === "operations.kindStatus" ? `${String(options?.kind)} · ${String(options?.status)}` : key;
  const outcomes: (Outcome | null)[] = [
    null,
    "Succeeded",
    "Cancelled",
    "Unconfirmed",
    { NeedsAttention: "NotInstalledAfterInstall" },
    { NeedsAttention: "StillInstalledAfterUninstall" },
    { NeedsAttention: "GoneAfterUpgrade" },
    { NeedsAttention: "UnchangedAfterUpgrade" },
    { NeedsAttention: "BackAfterUninstall" },
    { Failed: { exit_code: 1, summary: "", cause: failureCause("") } },
    { Failed: { exit_code: 1, summary: "   ", cause: failureCause("   ") } },
    { Failed: { exit_code: 1, summary: "Error: something odd happened", cause: failureCause("Error: something odd happened") } },
    { Failed: { exit_code: 1, summary: "curl: (6) Could not resolve host: ghcr.io", cause: failureCause("curl: (6) Could not resolve host: ghcr.io") } },
    { Failed: { exit_code: 1, summary: "sudo: a terminal is required to read the password", cause: failureCause("sudo: a terminal is required to read the password") } },
    { Failed: { exit_code: null, summary: "Operation not permitted", cause: failureCause("Operation not permitted") } },
    { BanagerFailed: "Panicked" },
    { BanagerFailed: "Internal" },
    { BanagerFailed: { ProgramMissing: { program: "brew" } } },
    { BanagerFailed: { SpawnFailed: { detail: "Permission denied" } } },
    { BanagerFailed: { HomebrewStillUpdating: { minutes: 10 } } },
    { BanagerFailed: { PathChanged: { path: "/opt/homebrew/bin/jq" } } },
    { BanagerFailed: { FormulaChanged: { name: "wget" } } },
    { BanagerFailed: "HomebrewSettingsChanged" },
  ];
  const waiting: LogLine = { opId: 1, note: { WaitingForBrewUpdate: { minutes: 10 } }, seq: 1 };
  const kinds: OpKind[] = ["Install", "Uninstall", "Upgrade"];
  const states: [OpSummary, LogLine[]][] = kinds.flatMap((kind) => [
    ...(["Queued", "Running", "CancelRequested", "Cancelling", "Verifying"] as const).flatMap(
      (status): [OpSummary, LogLine[]][] => [
        [op(1, status, { kind }), []],
        [op(1, status, { kind }), [waiting]],
      ],
    ),
    ...outcomes.map((outcome): [OpSummary, LogLine[]] => [op(1, "Done", { kind, outcome }), []]),
  ]);

  it("names what the operation does exactly once, whatever the status or outcome", () => {
    expect(states.length).toBe(3 * (10 + outcomes.length));
    for (const technical of [false, true]) {
      for (const [each, logs] of states) {
        const words = operationWords(keyT, each, logs, technical);
        const prefix = `${OP_KIND_KEYS[each.kind]} · `;
        const shown = words.startsWith(prefix) ? words.slice(prefix.length) : words;
        const label = `${each.kind} ${each.status} ${JSON.stringify(each.outcome)} logs=${logs.length} technical=${technical}: ${words}`;
        expect(namesKind(shown), label).toBe(shown === words);
      }
    }
  });
});
