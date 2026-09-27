import { describe, expect, it } from "vitest";
import { displayToken, elapsedSince, formatBytes, outcomeArgs, outcomeDetailKey, outcomeKey } from "./format";
import type { Fault, Outcome } from "./types";
import en from "../i18n/en.json";
import zhCN from "../i18n/zh-CN.json";

describe("displayToken", () => {
  it("leaves a plain token alone", () => {
    expect(displayToken("--cask")).toBe("--cask");
    expect(displayToken("/opt/homebrew/bin/brew")).toBe("/opt/homebrew/bin/brew");
  });

  it("quotes a program path that contains a space", () => {
    expect(displayToken("/Users/Alice Smith/bin/brew")).toBe("'/Users/Alice Smith/bin/brew'");
  });

  it("shows an empty argument as ''", () => {
    expect(displayToken("")).toBe("''");
  });

  it("escapes a single quote inside a quoted token", () => {
    expect(displayToken("it's")).toBe("'it'\\''s'");
  });

  it("quotes shell metacharacters so a copied preview cannot mean something else", () => {
    expect(displayToken("$HOME")).toBe("'$HOME'");
    expect(displayToken("a;b")).toBe("'a;b'");
    expect(displayToken("a|b")).toBe("'a|b'");
    expect(displayToken("*.rb")).toBe("'*.rb'");
    expect(displayToken("~/bin")).toBe("'~/bin'");
  });

  it("leaves an ordinary package name unquoted", () => {
    expect(displayToken("jq")).toBe("jq");
    expect(displayToken("gautham-v/tap/claudebar")).toBe("gautham-v/tap/claudebar");
    expect(displayToken("--formula")).toBe("--formula");
    expect(displayToken("python@3.13")).toBe("python@3.13");
  });
});

describe("formatBytes", () => {
  it("uses 1000-based units, the ones Finder shows", () => {
    // The number on the row should match Get Info in Finder, which
    // counts a kilobyte as 1000 bytes on macOS.
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(999)).toBe("999 B");
    expect(formatBytes(1000)).toBe("1 KB");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(12_000_000)).toBe("12 MB");
    expect(formatBytes(144_300_000)).toBe("144.3 MB");
    expect(formatBytes(4_400_000_000)).toBe("4.4 GB");
  });

  it("does not print a thousand of the smaller unit", () => {
    // 999,970 bytes is 999.97 KB, which one decimal rounds to 1000.0 KB;
    // that is 1 MB.
    expect(formatBytes(999_970)).toBe("1 MB");
  });
});

describe("outcomeKey", () => {
  it("gives the user's own cancel its own words in both languages", () => {
    const cancelled: Outcome = "Cancelled";
    expect(outcomeKey(cancelled)).toBe("Cancelled");
    expect(outcomeArgs(cancelled)).toEqual({});
    expect(en.operations.outcome.Cancelled).toBe("Cancelled");
    expect(zhCN.operations.outcome.Cancelled).toBe("已取消");
    // Nothing to add in the drawer: the log shows what it had done.
    expect(outcomeDetailKey(cancelled)).toBeNull();
  });

  it("words each NeedsAttention from the locale files, not from Rust's English, and with no prefix of its own", () => {
    // It used to carry Rust's own sentence ("package disappeared after
    // upgrade") and the drawer and the operation bar printed it inside the
    // translated frame, so a Chinese user read English there. Each used to
    // open with 「需要留意：」 too; the warning sign stands for that now
    // (`OutcomeIcon`, the copy table's C2).
    const gone: Outcome = { NeedsAttention: "GoneAfterUpgrade" };
    expect(outcomeKey(gone)).toBe("NeedsAttention.GoneAfterUpgrade");
    expect(outcomeArgs(gone)).toEqual({});
    expect(en.operations.outcome.NeedsAttention.GoneAfterUpgrade).toBe("Update reported success, but it's gone");
    expect(zhCN.operations.outcome.NeedsAttention.GoneAfterUpgrade).toBe("更新显示成功，但它不见了");
    for (const sentence of Object.values(zhCN.operations.outcome.NeedsAttention)) {
      expect(sentence).not.toContain("需要留意");
    }
    for (const sentence of Object.values(en.operations.outcome.NeedsAttention)) {
      expect(sentence).not.toMatch(/needs attention/i);
    }
  });

  it("says an uninstall seemed to succeed, naming no command, since a path-list uninstall runs none", () => {
    // `run_operation` (crates/canager-core/src/ops/mod.rs) sends this
    // whenever `execute` answered Succeeded and the reading after still
    // finds the item installed: after an uninstall command that exited 0,
    // and after a path-list uninstall (`execute_removal` in
    // crates/canager-core/src/adapters/standalone/removal.rs) that moved
    // every listed path to the Trash and ran no command at all. The
    // Chinese used to say "卸载命令显示成功" -- the uninstall *command*
    // showed success -- which is false on the second route.
    const still: Outcome = { NeedsAttention: "StillInstalledAfterUninstall" };
    expect(outcomeKey(still)).toBe("NeedsAttention.StillInstalledAfterUninstall");
    expect(outcomeArgs(still)).toEqual({});
    expect(en.operations.outcome.NeedsAttention.StillInstalledAfterUninstall).toBe(
      "Reported removed, but it's still there",
    );
    expect(zhCN.operations.outcome.NeedsAttention.StillInstalledAfterUninstall).toBe("显示卸载了，但它还在");
    expect(en.operations.outcome.NeedsAttention.StillInstalledAfterUninstall).not.toMatch(
      /command/i,
    );
    expect(zhCN.operations.outcome.NeedsAttention.StillInstalledAfterUninstall).not.toContain(
      "命令",
    );
  });

  it("never says a cancelled or crashed operation changed nothing, in either language (T9)", () => {
    // A path-list uninstall cancelled between two of its items has moved
    // the first to the Trash and still ends `Cancelled`
    // (crates/canager-core/src/events.rs), and an operation Canager lost
    // to a panic may have run its command (`Fault::Panicked`,
    // crates/canager-core/src/model.rs). The refusals that stop before
    // anything runs may say nothing changed, and do; these two may not --
    // nor what the drawer says next about a crash, nor the row's own word
    // for a cancelled update.
    const claimsNothingChanged = /nothing (was |has been )?changed|changed nothing|no changes were made|没有改动|未做任何改动/i;
    const panicked: Outcome = { CanagerFailed: "Panicked" };
    expect(outcomeKey("Cancelled")).toBe("Cancelled");
    expect(outcomeKey(panicked)).toBe("CanagerFailed.Panicked");
    expect(outcomeDetailKey(panicked)).toBe("operations.outcome.CanagerFailed.PanickedDetail");
    for (const locale of [en, zhCN]) {
      for (const sentence of [
        locale.operations.outcome.Cancelled,
        locale.operations.outcome.CanagerFailed.Panicked,
        locale.operations.outcome.CanagerFailed.PanickedDetail,
        locale.updates.progress.cancelled,
      ]) {
        expect(sentence).not.toMatch(claimsNothingChanged);
      }
    }
    // What a crash says instead, in the drawer: look at the list.
    expect(en.operations.outcome.CanagerFailed.PanickedDetail).toBe("Check the list to see whether anything changed.");
    expect(zhCN.operations.outcome.CanagerFailed.PanickedDetail).toBe("请看列表，确认有没有变化。");
    // The guard itself: it does catch the claim the refusals make.
    expect(en.planRefused.refused).toMatch(claimsNothingChanged);
    expect(zhCN.planRefused.refused).toMatch(claimsNothingChanged);
  });

  it("says an update that changed nothing changed nothing, and points to the log", () => {
    // Rust sends this when the tool exited 0 and the installed version
    // read before the update equals the one read after
    // (`run_operation` in crates/canager-core/src/ops/mod.rs). It used to
    // arrive as plain "Succeeded". The bar says what happened; the drawer
    // says where to look.
    const unchanged: Outcome = { NeedsAttention: "UnchangedAfterUpgrade" };
    expect(outcomeKey(unchanged)).toBe("NeedsAttention.UnchangedAfterUpgrade");
    expect(outcomeArgs(unchanged)).toEqual({});
    expect(en.operations.outcome.NeedsAttention.UnchangedAfterUpgrade).toBe(
      "Update reported success, but the version didn't change",
    );
    expect(zhCN.operations.outcome.NeedsAttention.UnchangedAfterUpgrade).toBe("更新显示成功，但版本没变");
    expect(outcomeDetailKey(unchanged)).toBe("operations.outcome.NeedsAttention.UnchangedAfterUpgradeDetail");
    expect(en.operations.outcome.NeedsAttention.UnchangedAfterUpgradeDetail).toBe(
      "The operation log shows what it printed.",
    );
    expect(zhCN.operations.outcome.NeedsAttention.UnchangedAfterUpgradeDetail).toBe(
      "可以在操作日志里看它输出了什么。",
    );
  });

  it("says files showed up again after a path-list uninstall, that the log names them, and what to do", () => {
    // `execute_removal` (crates/canager-core/src/adapters/standalone/removal.rs)
    // sends this itself when, after the pause that follows its last move,
    // part of what its list names is there. Each such path gets a log line
    // of its own (`operations.logNote.backAfterUninstall`), carrying the
    // path; that line says only what the last look found, since a path
    // there need not be the same thing it moved.
    const back: Outcome = { NeedsAttention: "BackAfterUninstall" };
    expect(outcomeKey(back)).toBe("NeedsAttention.BackAfterUninstall");
    expect(outcomeArgs(back)).toEqual({});
    expect(en.operations.outcome.NeedsAttention.BackAfterUninstall).toBe(
      "Files showed up again after the uninstall",
    );
    expect(zhCN.operations.outcome.NeedsAttention.BackAfterUninstall).toBe("移完后，原处又出现了文件");
    expect(outcomeDetailKey(back)).toBe("operations.outcome.NeedsAttention.BackAfterUninstallDetail");
    expect(en.operations.outcome.NeedsAttention.BackAfterUninstallDetail).toBe(
      "Quit the tool first. If it's still listed, uninstall it again; otherwise move the files named in the log to the Trash yourself.",
    );
    expect(zhCN.operations.outcome.NeedsAttention.BackAfterUninstallDetail).toBe(
      "先退出这个工具。列表里还有它就再卸载一次，否则把日志里列出的文件自己移到废纸篓。",
    );
    expect(en.operations.logNote.backAfterUninstall).toContain("{{path}}");
    expect(zhCN.operations.logNote.backAfterUninstall).toContain("{{path}}");
    expect(en.operations.logNote.backAfterUninstall).not.toMatch(/came back/);
    expect(zhCN.operations.logNote.backAfterUninstall).not.toContain("又回来");
  });

  it("gives the drawer a next step only where one follows, and every one exists in both languages", () => {
    const outcomes: Outcome[] = [
      "Succeeded",
      "Cancelled",
      "Unconfirmed",
      { NeedsAttention: "NotInstalledAfterInstall" },
      { NeedsAttention: "StillInstalledAfterUninstall" },
      { NeedsAttention: "GoneAfterUpgrade" },
      { NeedsAttention: "UnchangedAfterUpgrade" },
      { NeedsAttention: "BackAfterUninstall" },
      { Failed: { exit_code: 1, summary: "Error: No such keg" } },
      { Failed: { exit_code: 1, summary: " " } },
      { CanagerFailed: "Panicked" },
      { CanagerFailed: { ProgramMissing: { program: "/x/brew" } } },
      { CanagerFailed: { SpawnFailed: { detail: "EACCES" } } },
      { CanagerFailed: { HomebrewStillUpdating: { minutes: 10 } } },
      { CanagerFailed: { PathChanged: { path: "~/.local/bin/claude" } } },
      { CanagerFailed: "Internal" },
    ];
    const lookup = (locale: unknown, key: string): unknown =>
      key.split(".").reduce<unknown>(
        (node, part) => (node && typeof node === "object" ? (node as Record<string, unknown>)[part] : undefined),
        locale,
      );
    const withStep = outcomes
      .map((outcome) => [outcomeKey(outcome), outcomeDetailKey(outcome)] as const)
      .filter(([, detail]) => detail !== null);
    expect(withStep).toEqual([
      ["Unconfirmed", "operations.outcome.UnconfirmedDetail"],
      ["NeedsAttention.UnchangedAfterUpgrade", "operations.outcome.NeedsAttention.UnchangedAfterUpgradeDetail"],
      ["NeedsAttention.BackAfterUninstall", "operations.outcome.NeedsAttention.BackAfterUninstallDetail"],
      ["FailedSilent", "operations.outcome.FailedSilentDetail"],
      ["CanagerFailed.Panicked", "operations.outcome.CanagerFailed.PanickedDetail"],
      ["CanagerFailed.HomebrewStillUpdating", "operations.outcome.CanagerFailed.HomebrewStillUpdatingDetail"],
      ["CanagerFailed.PathChanged", "operations.outcome.CanagerFailed.PathChangedDetail"],
      // The same words as the refusal that says Canager itself went wrong.
      ["CanagerFailed.Internal", "common.canagerFaultDetail"],
    ]);
    for (const [, detail] of withStep) {
      expect(typeof lookup(en, detail as string), detail as string).toBe("string");
      expect(typeof lookup(zhCN, detail as string), detail as string).toBe("string");
    }
  });
});

describe("outcomeKey for Canager's own failures", () => {
  // Every `Fault` variant, as serde sends it (see model.rs's
  // `test_canager_failed_is_externally_tagged_on_the_wire`).
  const faults: Fault[] = [
    "Panicked",
    { ProgramMissing: { program: "/opt/homebrew/bin/brew" } },
    { SpawnFailed: { detail: "Permission denied (os error 13)" } },
    { HomebrewStillUpdating: { minutes: 10 } },
    { PathChanged: { path: "~/.local/bin/claude" } },
    "Internal",
  ];

  function lookup(locale: unknown, key: string): unknown {
    return key
      .split(".")
      .reduce<unknown>(
        (node, part) =>
          node && typeof node === "object" ? (node as Record<string, unknown>)[part] : undefined,
        locale,
      );
  }

  it("gives every Fault its own sentence in both languages", () => {
    const keys = new Set<string>();
    for (const fault of faults) {
      const key = `operations.outcome.${outcomeKey({ CanagerFailed: fault })}`;
      keys.add(key);
      expect(typeof lookup(en, key), key).toBe("string");
      expect(typeof lookup(zhCN, key), key).toBe("string");
    }
    expect(keys.size).toBe(faults.length);
  });

  it("passes a fault's data, never a sentence, to its translation", () => {
    expect(outcomeKey({ CanagerFailed: { ProgramMissing: { program: "/x/brew" } } })).toBe(
      "CanagerFailed.ProgramMissing",
    );
    expect(outcomeArgs({ CanagerFailed: { ProgramMissing: { program: "/x/brew" } } })).toEqual({
      program: "/x/brew",
    });
    expect(outcomeArgs({ CanagerFailed: { SpawnFailed: { detail: "EACCES" } } })).toEqual({
      detail: "EACCES",
    });
    expect(outcomeKey({ CanagerFailed: "Panicked" })).toBe("CanagerFailed.Panicked");
    expect(outcomeArgs({ CanagerFailed: "Panicked" })).toEqual({});
    expect(outcomeKey({ CanagerFailed: { HomebrewStillUpdating: { minutes: 10 } } })).toBe(
      "CanagerFailed.HomebrewStillUpdating",
    );
    expect(
      outcomeArgs({ CanagerFailed: { HomebrewStillUpdating: { minutes: 10 } } }),
    ).toEqual({ minutes: 10 });
    expect(en.operations.outcome.CanagerFailed.ProgramMissing).toContain("{{program}}");
    expect(zhCN.operations.outcome.CanagerFailed.ProgramMissing).toContain("{{program}}");
    expect(en.operations.outcome.CanagerFailed.SpawnFailed).toContain("{{detail}}");
    expect(zhCN.operations.outcome.CanagerFailed.SpawnFailed).toContain("{{detail}}");
    // Item (1) of the loose-ends pass: the "10" in these two sentences
    // must come from `BrewAdapter::OP_UPDATE_WAIT`
    // (`Fault::HomebrewStillUpdating`'s `minutes` field), never be a
    // second, independently-typed copy of the number.
    expect(en.operations.outcome.CanagerFailed.HomebrewStillUpdating).toContain("{{minutes}}");
    expect(zhCN.operations.outcome.CanagerFailed.HomebrewStillUpdating).toContain("{{minutes}}");
    // Phase 4 step C: the path a path-list uninstall stopped at, and the
    // two lines it writes in the log.
    expect(outcomeKey({ CanagerFailed: { PathChanged: { path: "~/.local/bin/claude" } } })).toBe(
      "CanagerFailed.PathChanged",
    );
    expect(outcomeArgs({ CanagerFailed: { PathChanged: { path: "~/.local/bin/claude" } } })).toEqual({
      path: "~/.local/bin/claude",
    });
    expect(en.operations.outcome.CanagerFailed.PathChanged).toContain("{{path}}");
    expect(zhCN.operations.outcome.CanagerFailed.PathChanged).toContain("{{path}}");
    expect(en.operations.logNote.movedToTrash).toContain("{{trashedTo}}");
    expect(zhCN.operations.logNote.movedToTrash).toContain("{{trashedTo}}");
    expect(en.operations.logNote.trashFailed).toContain("{{error}}");
    expect(zhCN.operations.logNote.trashFailed).toContain("{{error}}");
    expect(en.operations.logNote.waitingForBrewUpdate).toContain("{{minutes}}");
    expect(zhCN.operations.logNote.waitingForBrewUpdate).toContain("{{minutes}}");
  });

  it("keeps a tool's own stderr as Failed, and words a silent failure instead of a blank", () => {
    expect(outcomeKey({ Failed: { exit_code: 1, summary: "Error: No such keg\n" } })).toBe(
      "Failed",
    );
    expect(outcomeArgs({ Failed: { exit_code: 1, summary: "Error: No such keg\n" } })).toEqual({
      summary: "Error: No such keg",
    });
    expect(outcomeKey({ Failed: { exit_code: 1, summary: "  \n" } })).toBe("FailedSilent");
    expect(zhCN.operations.outcome.FailedSilent).not.toContain("{{");
  });
});

describe("elapsedSince", () => {
  const then = 1790586000;
  const at = (seconds: number) => (then + seconds) * 1000;

  it("says just now for under a minute, and for a time after the clock", () => {
    expect(elapsedSince(then, at(0))).toEqual({ unit: "justNow" });
    expect(elapsedSince(then, at(59.9))).toEqual({ unit: "justNow" });
    // A check that finished after the header's clock last ticked.
    expect(elapsedSince(then, at(-30))).toEqual({ unit: "justNow" });
  });

  it("counts whole minutes, then whole hours, then whole days, rounding down", () => {
    expect(elapsedSince(then, at(60))).toEqual({ unit: "minutes", count: 1 });
    expect(elapsedSince(then, at(3 * 60 + 59))).toEqual({ unit: "minutes", count: 3 });
    expect(elapsedSince(then, at(59 * 60 + 59))).toEqual({ unit: "minutes", count: 59 });
    expect(elapsedSince(then, at(60 * 60))).toEqual({ unit: "hours", count: 1 });
    expect(elapsedSince(then, at(23 * 3600 + 3599))).toEqual({ unit: "hours", count: 23 });
    expect(elapsedSince(then, at(24 * 3600))).toEqual({ unit: "days", count: 1 });
    expect(elapsedSince(then, at(9 * 24 * 3600 + 5))).toEqual({ unit: "days", count: 9 });
  });
});
