import { describe, expect, it } from "vitest";
import {
  displayToken,
  elapsedSince,
  formatBytes,
  outcomeArgs,
  outcomeDetailKey,
  outcomeKey,
  outcomeStepKey,
  stepFailedOf,
} from "./format";
import type { Fault, HistoryResult, Outcome } from "./types";
import en from "../i18n/en.json";
import zhCN from "../i18n/zh-CN.json";
import zhHant from "../i18n/zh-Hant.json";
import { failureCause } from "./failureCause";

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
    expect(zhCN.operations.outcome.NeedsAttention.GoneAfterUpgrade).toBe("显示已更新，但它不见了");
    for (const sentence of Object.values(zhCN.operations.outcome.NeedsAttention)) {
      expect(sentence).not.toContain("需要留意");
    }
    for (const sentence of Object.values(en.operations.outcome.NeedsAttention)) {
      expect(sentence).not.toMatch(/needs attention/i);
    }
  });

  it("says an uninstall seemed to succeed, naming no command, since a path-list uninstall runs none", () => {
    // `run_operation` (crates/banager-core/src/ops/mod.rs) sends this
    // whenever `execute` answered Succeeded and the reading after still
    // finds the item installed: after an uninstall command that exited 0,
    // and after a path-list uninstall (`execute_removal` in
    // crates/banager-core/src/adapters/standalone/removal.rs) that moved
    // every listed path to the Trash and ran no command at all. The
    // Chinese used to say "卸载命令显示成功" -- the uninstall *command*
    // showed success -- which is false on the second route.
    const still: Outcome = { NeedsAttention: "StillInstalledAfterUninstall" };
    expect(outcomeKey(still)).toBe("NeedsAttention.StillInstalledAfterUninstall");
    expect(outcomeArgs(still)).toEqual({});
    expect(en.operations.outcome.NeedsAttention.StillInstalledAfterUninstall).toBe(
      "Reported removed, but it's still there",
    );
    expect(zhCN.operations.outcome.NeedsAttention.StillInstalledAfterUninstall).toBe("显示已卸载，但它仍然存在");
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
    // (crates/banager-core/src/events.rs), and an operation Banager lost
    // to a panic may have run its command (`Fault::Panicked`,
    // crates/banager-core/src/model.rs). The refusals that stop before
    // anything runs may say nothing changed, and do; these two may not --
    // nor what the drawer says next about a crash, nor the row's own word
    // for a cancelled update.
    const claimsNothingChanged = /nothing (was |has been )?changed|changed nothing|no changes were made|没有改动|未做任何改动/i;
    const panicked: Outcome = { BanagerFailed: "Panicked" };
    expect(outcomeKey("Cancelled")).toBe("Cancelled");
    expect(outcomeKey(panicked)).toBe("BanagerFailed.Panicked");
    expect(outcomeDetailKey(panicked)).toBe("operations.outcome.BanagerFailed.PanickedDetail");
    for (const locale of [en, zhCN]) {
      for (const sentence of [
        locale.operations.outcome.Cancelled,
        locale.operations.outcome.BanagerFailed.Panicked,
        locale.operations.outcome.BanagerFailed.PanickedDetail,
        locale.updates.progress.cancelled,
      ]) {
        expect(sentence).not.toMatch(claimsNothingChanged);
      }
    }
    // What a crash says instead, in the drawer: look at the list.
    expect(en.operations.outcome.BanagerFailed.PanickedDetail).toBe("Check the list to see whether anything changed.");
    expect(zhCN.operations.outcome.BanagerFailed.PanickedDetail).toBe("请查看列表，确认是否有变化。");
    // The guard itself: it does catch the claim an operation that never
    // got to run makes. (The refusals no longer make it: they stop before
    // anything starts, and the polish-3 copy rules keep 「没有改动」 for an
    // operation that had started.)
    expect(en.operations.outcome.BanagerFailed.Internal).toMatch(claimsNothingChanged);
    expect(zhCN.operations.outcome.BanagerFailed.Internal).toMatch(claimsNothingChanged);
  });

  it("says an update that changed nothing changed nothing, and what to do next", () => {
    // Rust sends this when the tool exited 0 and the installed version
    // read before the update equals the one read after
    // (`run_operation` in crates/banager-core/src/ops/mod.rs). It used to
    // arrive as plain "Succeeded". The bar says what happened; the drawer,
    // over the log, says what to do: Retry, then Copy Log -- not "the
    // operation log shows what it printed", said over that log (r24 W4).
    const unchanged: Outcome = { NeedsAttention: "UnchangedAfterUpgrade" };
    expect(outcomeKey(unchanged)).toBe("NeedsAttention.UnchangedAfterUpgrade");
    expect(outcomeArgs(unchanged)).toEqual({});
    expect(en.operations.outcome.NeedsAttention.UnchangedAfterUpgrade).toBe(
      "Update reported success, but the version didn't change",
    );
    expect(zhCN.operations.outcome.NeedsAttention.UnchangedAfterUpgrade).toBe("显示已更新，但版本没有变化");
    expect(outcomeDetailKey(unchanged)).toBe("operations.outcome.NeedsAttention.UnchangedAfterUpgradeDetail");
    expect(en.operations.outcome.NeedsAttention.UnchangedAfterUpgradeDetail).toBe(
      "You can click Retry. If the version still doesn't change, click Copy Log and send the log to someone who can help.",
    );
    expect(zhCN.operations.outcome.NeedsAttention.UnchangedAfterUpgradeDetail).toBe(
      "可以点按“重试”；版本还是没有变化，就点按“拷贝日志”，发给懂的人看。",
    );
    expect(zhHant.operations.outcome.NeedsAttention.UnchangedAfterUpgradeDetail).toBe(
      "可以點按「再試一次」；版本還是沒有變化，就點按「拷貝記錄」，傳給懂的人看。",
    );
  });

  it("says an update installed though a step after it failed, with the version it moved to, and where to look (r35 U2)", () => {
    // Rust sends this when the tool exited non-zero and the installed
    // version it read after the update had moved from the one before
    // (`run_operation`): Homebrew's post-install step failing after the
    // new keg was linked. Not "Couldn't update": the update is installed.
    const stepped: Outcome = { NeedsAttention: { UpdatedButStepFailed: { version: "3.13.8", cause: null, detail: null } } };
    expect(outcomeKey(stepped)).toBe("NeedsAttention.UpdatedButStepFailed");
    expect(outcomeArgs(stepped)).toEqual({ version: "3.13.8" });
    expect(en.operations.outcome.NeedsAttention.UpdatedButStepFailed).toBe(
      "Updated to {{version}}, but a step after it failed; see the log",
    );
    expect(zhCN.operations.outcome.NeedsAttention.UpdatedButStepFailed).toBe(
      "已更新到{{version}}，但之后有一步失败了，请查看日志",
    );
    expect(zhHant.operations.outcome.NeedsAttention.UpdatedButStepFailed).toBe(
      "已更新到{{version}}，但之後有一步失敗了，請查看記錄",
    );
    // A model's "version" is a digest, never shown: the same, without it.
    const model: Outcome = { NeedsAttention: { UpdatedButStepFailed: { version: null, cause: null, detail: null } } };
    expect(outcomeKey(model)).toBe("NeedsAttention.UpdatedButStepFailedNoVersion");
    expect(outcomeArgs(model)).toEqual({});
    expect(en.operations.outcome.NeedsAttention.UpdatedButStepFailedNoVersion).toBe(
      "Updated, but a step after it failed; see the log",
    );
    expect(zhCN.operations.outcome.NeedsAttention.UpdatedButStepFailedNoVersion).toBe(
      "已更新，但之后有一步失败了，请查看日志",
    );
    expect(zhHant.operations.outcome.NeedsAttention.UpdatedButStepFailedNoVersion).toBe(
      "已更新，但之後有一步失敗了，請查看記錄",
    );
    // Under the log: the new version is in, the log says which step, and
    // Copy Log for help -- never "Retry", which would not run that step.
    for (const each of [stepped, model]) {
      expect(outcomeDetailKey(each)).toBe("operations.outcome.NeedsAttention.UpdatedButStepFailedDetail");
      expect(outcomeStepKey(each, true)).toBe("operations.outcome.NeedsAttention.UpdatedButStepFailedDetail");
      // Over a log with no line, Copy Log is off, and nothing is to be tried again.
      expect(outcomeStepKey(each, false)).toBeNull();
    }
    for (const locale of [en, zhCN, zhHant]) {
      const detail = locale.operations.outcome.NeedsAttention.UpdatedButStepFailedDetail;
      expect(detail).toContain(locale.operations.copyLog);
      expect(detail).not.toContain(locale.updates.retry);
    }
    expect(en.operations.outcome.NeedsAttention.UpdatedButStepFailedDetail).toBe(
      "The new version is installed, and the log says which step failed. If you're not sure what to do, click Copy Log and send the log to someone who can help.",
    );
    expect(zhCN.operations.outcome.NeedsAttention.UpdatedButStepFailedDetail).toBe(
      "新版本已经装好，日志里写着哪一步失败了。不知道怎么办，就点按“拷贝日志”，发给懂的人看。",
    );
    expect(zhHant.operations.outcome.NeedsAttention.UpdatedButStepFailedDetail).toBe(
      "新版本已經裝好，記錄裡寫著哪一步失敗了。不知道怎麼辦，就點按「拷貝記錄」，傳給懂的人看。",
    );
  });

  it("says an update whose link step failed is installed but not linked, and that the log says which file (skeptic of r35 U2, 1)", () => {
    // Homebrew's link step fails only after the new keg is poured, so the
    // core says it as installed with that step failed, keeping the cause
    // it read: the words of what not linked means stay with it.
    const unlinked: Outcome = {
      NeedsAttention: { UpdatedButStepFailed: { version: "22.23.3_1", cause: "notLinked", detail: null } },
    };
    expect(outcomeKey(unlinked)).toBe("NeedsAttention.UpdatedButNotLinked");
    expect(outcomeArgs(unlinked)).toEqual({ version: "22.23.3_1" });
    expect(en.operations.outcome.NeedsAttention.UpdatedButNotLinked).toBe(
      "Updated to {{version}}, but the new version isn't linked; see the log",
    );
    expect(zhCN.operations.outcome.NeedsAttention.UpdatedButNotLinked).toBe(
      "已更新到{{version}}，但新版本没有链接到终端，请查看日志",
    );
    expect(zhHant.operations.outcome.NeedsAttention.UpdatedButNotLinked).toBe(
      "已更新到{{version}}，但新版本沒有連結到終端機，請查看記錄",
    );
    // Under the log: what that means in Terminal, where the log says which
    // file -- and, as for any step after the update, never Retry.
    expect(outcomeDetailKey(unlinked)).toBe("operations.outcome.NeedsAttention.UpdatedButNotLinkedDetail");
    expect(outcomeStepKey(unlinked, true)).toBe("operations.outcome.NeedsAttention.UpdatedButNotLinkedDetail");
    expect(outcomeStepKey(unlinked, false)).toBeNull();
    for (const locale of [en, zhCN, zhHant]) {
      const detail = locale.operations.outcome.NeedsAttention.UpdatedButNotLinkedDetail;
      expect(detail).toContain(locale.operations.copyLog);
      expect(detail).not.toContain(locale.updates.retry);
    }
    expect(en.operations.outcome.NeedsAttention.UpdatedButNotLinkedDetail).toBe(
      "Its commands may not be found in Terminal, usually because a file of the same name is in the way; the log says which file. If you're not sure what to do, click Copy Log and send the log to someone who can help.",
    );
    expect(zhCN.operations.outcome.NeedsAttention.UpdatedButNotLinkedDetail).toBe(
      "在终端里输入它的命令可能找不到它；通常是有同名的文件挡住了，日志里写着是哪个文件。不知道怎么办，就点按“拷贝日志”，发给懂的人看。",
    );
    expect(zhHant.operations.outcome.NeedsAttention.UpdatedButNotLinkedDetail).toBe(
      "在終端機裡輸入它的指令可能找不到它；通常是有同名的檔案擋住了，記錄裡寫著是哪個檔案。不知道怎麼辦，就點按「拷貝記錄」，傳給懂的人看。",
    );
    // Any other cause keeps the general words: the words of the failure
    // causes say "then try again", and its line says which step.
    const permission: Outcome = {
      NeedsAttention: {
        UpdatedButStepFailed: { version: "2.18.4", cause: "permission", detail: "Permission denied @ rb_sysopen" },
      },
    };
    expect(outcomeKey(permission)).toBe("NeedsAttention.UpdatedButStepFailed");
    expect(outcomeDetailKey(permission)).toBe("operations.outcome.NeedsAttention.UpdatedButStepFailedDetail");
    // As the history hands it back, and from a record kept before the cause was.
    expect(stepFailedOf(unlinked)).toEqual({ version: "22.23.3_1", cause: "notLinked", detail: null });
    const older = { NeedsAttention: { UpdatedButStepFailed: { version: "3.13.8" } } } as unknown as HistoryResult;
    expect(stepFailedOf(older)).toEqual({ version: "3.13.8", cause: null, detail: null });
    expect(stepFailedOf({ NeedsAttention: "UnchangedAfterUpgrade" })).toBeNull();
  });

  it("ends the three steps that point at the log at Copy Log, by its own name, and never over an empty log (r24 W4)", () => {
    const pointing: Outcome[] = [
      { NeedsAttention: "UnchangedAfterUpgrade" },
      { NeedsAttention: "NotLinkedAfterLink" },
      { Failed: { exit_code: 1, summary: "", cause: null } },
    ];
    for (const [locale, copyLog, retry, fix] of [
      [en, en.operations.copyLog, en.updates.retry, en.noAnswer.fix],
      [zhCN, zhCN.operations.copyLog, zhCN.updates.retry, zhCN.noAnswer.fix],
      [zhHant, zhHant.operations.copyLog, zhHant.updates.retry, zhHant.noAnswer.fix],
    ] as const) {
      const outcome = locale.operations.outcome;
      for (const detail of [
        outcome.NeedsAttention.UnchangedAfterUpgradeDetail,
        outcome.NeedsAttention.NotLinkedAfterLinkDetail,
        outcome.FailedSilentDetail,
      ]) {
        expect(detail).toContain(copyLog);
        // Not the log the sentence is said over, by another name.
        expect(detail).not.toMatch(/operation log|操作日志|操作記錄/);
      }
      expect(outcome.NeedsAttention.UnchangedAfterUpgradeDetail).toContain(retry);
      expect(outcome.NeedsAttention.NotLinkedAfterLinkDetail).toContain(fix);
      // How to try again by what was tried (`TRY_AGAIN_KEYS`).
      expect(outcome.FailedSilentDetail).toContain("{{again}}");
      expect(outcome.emptyLogDetail).toContain("{{again}}");
      expect(outcome.emptyLogDetail).not.toContain(copyLog);
    }
    for (const each of pointing) {
      // A log with lines: the step, ending at Copy Log.
      expect(outcomeStepKey(each, true)).toBe(outcomeDetailKey(each));
      // Copy Log is off over a log with none: only how to try again.
      expect(outcomeStepKey(each, false)).toBe("operations.outcome.emptyLogDetail");
    }
    // Every other step is the same over an empty log: none ends at Copy Log.
    for (const other of ["Unconfirmed", { NeedsAttention: "BackAfterUninstall" }, { BanagerFailed: "Panicked" }] as Outcome[]) {
      expect(outcomeStepKey(other, false)).toBe(outcomeDetailKey(other));
    }
    expect(outcomeStepKey("Succeeded", false)).toBeNull();
  });

  it("says files showed up again after a path-list uninstall, that the log names them, and what to do", () => {
    // `execute_removal` (crates/banager-core/src/adapters/standalone/removal.rs)
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
    expect(zhCN.operations.outcome.NeedsAttention.BackAfterUninstall).toBe("移走后，原处又出现了文件");
    expect(outcomeDetailKey(back)).toBe("operations.outcome.NeedsAttention.BackAfterUninstallDetail");
    expect(en.operations.outcome.NeedsAttention.BackAfterUninstallDetail).toBe(
      "Quit the tool first. If it's still listed, uninstall it again; otherwise move the files named in the log to the Trash yourself.",
    );
    expect(zhCN.operations.outcome.NeedsAttention.BackAfterUninstallDetail).toBe(
      "请先退出此工具。如果列表中仍有它，请再卸载一次；否则请将日志中列出的文件手动移到废纸篓。",
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
      { NeedsAttention: "NotLinkedAfterLink" },
      { NeedsAttention: { UpdatedButStepFailed: { version: "3.13.8", cause: null, detail: null } } },
      { NeedsAttention: { UpdatedButStepFailed: { version: "22.23.3_1", cause: "notLinked", detail: null } } },
      { Failed: { exit_code: 1, summary: "Error: No such keg", cause: failureCause("Error: No such keg") } },
      { Failed: { exit_code: 1, summary: " ", cause: failureCause(" ") } },
      { BanagerFailed: "Panicked" },
      { BanagerFailed: { ProgramMissing: { program: "/x/brew" } } },
      { BanagerFailed: { SpawnFailed: { detail: "EACCES" } } },
      { BanagerFailed: { HomebrewStillUpdating: { minutes: 10 } } },
      { BanagerFailed: { PathChanged: { path: "~/.local/bin/claude" } } },
      { BanagerFailed: { FormulaChanged: { name: "wget" } } },
      { BanagerFailed: "HomebrewSettingsChanged" },
      { BanagerFailed: { LinkTaken: { name: "node@22", paths: ["/opt/homebrew/bin/npm"] } } },
      { BanagerFailed: "Internal" },
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
      ["NeedsAttention.NotLinkedAfterLink", "operations.outcome.NeedsAttention.NotLinkedAfterLinkDetail"],
      ["NeedsAttention.UpdatedButStepFailed", "operations.outcome.NeedsAttention.UpdatedButStepFailedDetail"],
      ["NeedsAttention.UpdatedButNotLinked", "operations.outcome.NeedsAttention.UpdatedButNotLinkedDetail"],
      ["FailedSilent", "operations.outcome.FailedSilentDetail"],
      ["BanagerFailed.Panicked", "operations.outcome.BanagerFailed.PanickedDetail"],
      ["BanagerFailed.HomebrewStillUpdating", "operations.outcome.BanagerFailed.HomebrewStillUpdatingDetail"],
      ["BanagerFailed.PathChanged", "operations.outcome.BanagerFailed.PathChangedDetail"],
      ["BanagerFailed.FormulaChanged", "operations.outcome.BanagerFailed.FormulaChangedDetail"],
      ["BanagerFailed.HomebrewSettingsChanged", "operations.outcome.BanagerFailed.HomebrewSettingsChangedDetail"],
      ["BanagerFailed.LinkTaken", "operations.outcome.BanagerFailed.LinkTakenDetail"],
    ]);
    for (const [, detail] of withStep) {
      expect(typeof lookup(en, detail as string), detail as string).toBe("string");
      expect(typeof lookup(zhCN, detail as string), detail as string).toBe("string");
    }
  });
});

describe("outcomeKey for Banager's own failures", () => {
  // Every `Fault` variant, as serde sends it (see model.rs's
  // `test_banager_failed_is_externally_tagged_on_the_wire`).
  const faults: Fault[] = [
    { LinkRollbackRisk: { name: "node@22" } },
    "Panicked",
    { ProgramMissing: { program: "/opt/homebrew/bin/brew" } },
    { SpawnFailed: { detail: "Permission denied (os error 13)" } },
    { HomebrewStillUpdating: { minutes: 10 } },
    { PathChanged: { path: "~/.local/bin/claude" } },
    { FormulaChanged: { name: "wget" } },
    "HomebrewSettingsChanged",
    { LinkTaken: { name: "node@22", paths: ["/opt/homebrew/bin/npm"] } },
    { LinkTaken: { name: "node@22", paths: ["/opt/homebrew/bin/npm", "/opt/homebrew/bin/npx"] } },
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
      const key = `operations.outcome.${outcomeKey({ BanagerFailed: fault })}`;
      keys.add(key);
      expect(typeof lookup(en, key), key).toBe("string");
      expect(typeof lookup(zhCN, key), key).toBe("string");
    }
    expect(keys.size).toBe(faults.length);
  });

  it("calls the sheet a refusal points back to the confirmation, never a preview, as every sibling does (r21 C3)", () => {
    for (const [locale, word] of [
      [en, /preview/i],
      [zhCN, /预览/],
      [zhHant, /預覽/],
    ] as const) {
      for (const text of Object.values(locale.operations.outcome.BanagerFailed)) {
        expect(text).not.toMatch(word);
      }
    }
    expect(en.operations.outcome.BanagerFailed.LinkRollbackRiskDetail).toBe(
      "Links to it appeared after the confirmation opened, and Homebrew removes them too if linking stops partway. Nothing was linked; they're as they were.",
    );
    expect(zhCN.operations.outcome.BanagerFailed.LinkRollbackRiskDetail).toBe(
      "确认窗口打开后出现了指向它的链接，如果链接中途停止，Homebrew也会删除它们。没有开始链接，它们保持原样。",
    );
    expect(zhHant.operations.outcome.BanagerFailed.LinkRollbackRiskDetail).toBe(
      "確認視窗開啟後出現了指向它的連結，若連結中途停止，Homebrew也會刪除它們。沒有開始連結，它們維持原樣。",
    );
  });

  it("never says 「没有链接」 for nothing was linked, which reads as there being no links (r21 C3, skeptic)", () => {
    // 链接 / 連結 is a noun as well as a verb, and the sentence before it
    // says links to the tool appeared: 「没有链接」 there reads as "there
    // are no links". 「没有开始链接」 can only be the linking that did not
    // start, and still opens with 没有 as 「没有更新」 and 「没有改动」 do.
    for (const [locale, words] of [
      [zhCN, /(^|。)没有链接/],
      [zhHant, /(^|。)沒有連結/],
    ] as const) {
      for (const text of Object.values(locale.operations.outcome.BanagerFailed)) {
        expect(text).not.toMatch(words);
      }
    }
  });

  it("passes a fault's data, never a sentence, to its translation", () => {
    expect(outcomeKey({ BanagerFailed: { ProgramMissing: { program: "/x/brew" } } })).toBe(
      "BanagerFailed.ProgramMissing",
    );
    expect(outcomeArgs({ BanagerFailed: { ProgramMissing: { program: "/x/brew" } } })).toEqual({
      program: "/x/brew",
    });
    expect(outcomeArgs({ BanagerFailed: { SpawnFailed: { detail: "EACCES" } } })).toEqual({
      detail: "EACCES",
    });
    expect(outcomeKey({ BanagerFailed: "Panicked" })).toBe("BanagerFailed.Panicked");
    expect(outcomeArgs({ BanagerFailed: "Panicked" })).toEqual({});
    expect(outcomeKey({ BanagerFailed: { HomebrewStillUpdating: { minutes: 10 } } })).toBe(
      "BanagerFailed.HomebrewStillUpdating",
    );
    expect(
      outcomeArgs({ BanagerFailed: { HomebrewStillUpdating: { minutes: 10 } } }),
    ).toEqual({ minutes: 10 });
    expect(en.operations.outcome.BanagerFailed.ProgramMissing).toContain("{{program}}");
    expect(zhCN.operations.outcome.BanagerFailed.ProgramMissing).toContain("{{program}}");
    expect(en.operations.outcome.BanagerFailed.SpawnFailed).toContain("{{detail}}");
    expect(zhCN.operations.outcome.BanagerFailed.SpawnFailed).toContain("{{detail}}");
    // Item (1) of the loose-ends pass: the "10" in these two sentences
    // must come from `BrewAdapter::OP_UPDATE_WAIT`
    // (`Fault::HomebrewStillUpdating`'s `minutes` field), never be a
    // second, independently-typed copy of the number.
    expect(en.operations.outcome.BanagerFailed.HomebrewStillUpdating).toContain("{{minutes}}");
    expect(zhCN.operations.outcome.BanagerFailed.HomebrewStillUpdating).toContain("{{minutes}}");
    // Phase 4 step C: the path a path-list uninstall stopped at, and the
    // two lines it writes in the log.
    expect(outcomeKey({ BanagerFailed: { PathChanged: { path: "~/.local/bin/claude" } } })).toBe(
      "BanagerFailed.PathChanged",
    );
    expect(outcomeArgs({ BanagerFailed: { PathChanged: { path: "~/.local/bin/claude" } } })).toEqual({
      path: "~/.local/bin/claude",
    });
    expect(en.operations.outcome.BanagerFailed.PathChanged).toContain("{{path}}");
    expect(zhCN.operations.outcome.BanagerFailed.PathChanged).toContain("{{path}}");
    // Review F3 (r6): an uninstall of every version of a formula whose
    // Cellar or pin changed since the preview ran nothing; it names the
    // formula and asks for a new look.
    expect(outcomeKey({ BanagerFailed: { FormulaChanged: { name: "wget" } } })).toBe(
      "BanagerFailed.FormulaChanged",
    );
    expect(outcomeArgs({ BanagerFailed: { FormulaChanged: { name: "wget" } } })).toEqual({ name: "wget" });
    expect(outcomeDetailKey({ BanagerFailed: { FormulaChanged: { name: "wget" } } })).toBe(
      "operations.outcome.BanagerFailed.FormulaChangedDetail",
    );
    // Its first sentence says the formula changed, so the next must not
    // open with "Nothing changed" (review of v1-brew's fixes, r6): it says
    // what Banager did not do instead.
    expect(en.operations.outcome.BanagerFailed.FormulaChangedDetail).toBe(
      "Nothing was removed. Open the confirmation again to see which versions it removes.",
    );
    // Review of v1-brew's fixes (r6): an install or update found Homebrew
    // would now delete more after it than its preview said, and ran
    // nothing; a cask's uninstall (f30a) ends the same way when what it
    // would remove changed. A bare string naming what the two have in
    // common, what Homebrew would delete -- never an uninstall, which an
    // update refused this way is not (r21 C1) -- then what did not run,
    // and a new look.
    expect(outcomeKey({ BanagerFailed: "HomebrewSettingsChanged" })).toBe(
      "BanagerFailed.HomebrewSettingsChanged",
    );
    expect(outcomeArgs({ BanagerFailed: "HomebrewSettingsChanged" })).toEqual({});
    expect(outcomeDetailKey({ BanagerFailed: "HomebrewSettingsChanged" })).toBe(
      "operations.outcome.BanagerFailed.HomebrewSettingsChangedDetail",
    );
    expect(en.operations.outcome.BanagerFailed.HomebrewSettingsChanged).toBe(
      "Couldn't start: what Homebrew would delete changed after the confirmation opened, or couldn't be read",
    );
    expect(en.operations.outcome.BanagerFailed.HomebrewSettingsChangedDetail).toBe(
      "Homebrew wasn't run. Open the confirmation again to see what it deletes now.",
    );
    expect(zhCN.operations.outcome.BanagerFailed.HomebrewSettingsChanged).toBe(
      "未能开始：确认窗口打开后，Homebrew要删除的内容有了变化，或无法读取",
    );
    expect(zhCN.operations.outcome.BanagerFailed.HomebrewSettingsChangedDetail).toBe(
      "没有运行Homebrew。请重新打开确认窗口，查看它现在会删除什么。",
    );
    expect(zhHant.operations.outcome.BanagerFailed.HomebrewSettingsChanged).toBe(
      "未能開始：確認視窗開啟後，Homebrew要刪除的內容有了變化，或無法讀取",
    );
    expect(zhHant.operations.outcome.BanagerFailed.HomebrewSettingsChangedDetail).toBe(
      "沒有執行Homebrew。請重新開啟確認視窗，查看它現在會刪除什麼。",
    );
    // y1-keg: a keg-only formula's update found another program in its
    // commands' places and ran nothing. It names the first place, and how
    // many there are when more than one; the next sentence says why that
    // stops an update, and that the tool still works.
    const npm: Outcome = { BanagerFailed: { LinkTaken: { name: "node@22", paths: ["/opt/homebrew/bin/npm"] } } };
    const both: Outcome = {
      BanagerFailed: { LinkTaken: { name: "node@22", paths: ["/opt/homebrew/bin/npm", "/opt/homebrew/bin/npx"] } },
    };
    expect(outcomeKey(npm)).toBe("BanagerFailed.LinkTaken");
    expect(outcomeKey(both)).toBe("BanagerFailed.LinkTakenMany");
    expect(outcomeArgs(both)).toEqual({ name: "node@22", path: "/opt/homebrew/bin/npm", number: 2, others: 1 });
    expect(outcomeDetailKey(both)).toBe("operations.outcome.BanagerFailed.LinkTakenDetail");
    expect(en.operations.outcome.BanagerFailed.LinkTaken).toBe("Couldn't start: another program is using {{path}}");
    expect(en.operations.outcome.BanagerFailed.LinkTakenMany).toBe(
      "Couldn't start: another program is using {{path}} and {{others}} more",
    );
    expect(zhCN.operations.outcome.BanagerFailed.LinkTaken).toBe("未能开始：{{path}}已被另一个程序占用");
    expect(zhCN.operations.outcome.BanagerFailed.LinkTakenMany).toBe(
      "未能开始：{{path}}等{{number}}个文件已被另一个程序占用",
    );
    expect(zhCN.operations.outcome.BanagerFailed.LinkTakenDetail).toBe(
      "更新会先解除这个工具的链接，被占用的文件会挡住它重新链接，终端里就会找不到它的命令。没有更新，它仍可在终端里使用。",
    );
    expect(en.operations.logNote.movedToTrash).toContain("{{trashedTo}}");
    expect(zhCN.operations.logNote.movedToTrash).toContain("{{trashedTo}}");
    expect(en.operations.logNote.trashFailed).toContain("{{error}}");
    expect(zhCN.operations.logNote.trashFailed).toContain("{{error}}");
    expect(en.operations.logNote.waitingForBrewUpdate).toContain("{{minutes}}");
    expect(zhCN.operations.logNote.waitingForBrewUpdate).toContain("{{minutes}}");
  });

  it("keeps a tool's own stderr as Failed, and words a silent failure instead of a blank", () => {
    expect(outcomeKey({ Failed: { exit_code: 1, summary: "Error: No such keg\n", cause: failureCause("Error: No such keg\n") } })).toBe(
      "Failed",
    );
    expect(outcomeArgs({ Failed: { exit_code: 1, summary: "Error: No such keg\n", cause: failureCause("Error: No such keg\n") } })).toEqual({
      summary: "Error: No such keg",
    });
    expect(outcomeKey({ Failed: { exit_code: 1, summary: "  \n", cause: failureCause("  \n") } })).toBe("FailedSilent");
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

it("renders the pre-upgrade absence wire outcome in all three languages", () => {
  const wire = '{"NeedsAttention":"GoneBeforeUpgrade"}';
  const outcome: Outcome = { NeedsAttention: "GoneBeforeUpgrade" };
  expect(JSON.stringify(outcome)).toBe(wire);
  expect(JSON.parse(wire)).toEqual(outcome);
  expect(outcomeKey(outcome)).toBe("NeedsAttention.GoneBeforeUpgrade");
  expect(outcomeDetailKey(outcome)).toBeNull();
  expect(en.operations.outcome.NeedsAttention.GoneBeforeUpgrade).toBe("Update didn't start: it's no longer installed");
  expect(zhCN.operations.outcome.NeedsAttention.GoneBeforeUpgrade).toBe("未开始更新：已找不到它");
  expect(zhHant.operations.outcome.NeedsAttention.GoneBeforeUpgrade).toBe("未開始更新：已找不到它");
});

it("says a saved operation whose installation changed since its preview did not start", () => {
  const outcome = { BanagerFailed: "ChangedSinceShown" } as const;
  expect(outcomeKey(outcome)).toBe("BanagerFailed.ChangedSinceShown");
  expect(outcomeDetailKey(outcome)).toBe("operations.outcome.BanagerFailed.ChangedSinceShownDetail");
  expect(outcomeArgs(outcome)).toEqual({});
  expect(en.operations.outcome.BanagerFailed.ChangedSinceShown).toBe(
    "Couldn't start: where or how it's installed changed after the confirmation opened, or couldn't be read",
  );
  expect(zhCN.operations.outcome.BanagerFailed.ChangedSinceShown).toBe(
    "未能开始：确认窗口打开后，它的安装位置或安装方式有了变化，或无法读取",
  );
  expect(zhHant.operations.outcome.BanagerFailed.ChangedSinceShown).toBe(
    "未能開始：確認視窗開啟後，它的安裝位置或安裝方式有了變化，或無法讀取",
  );
  for (const locale of [en, zhCN, zhHant]) {
    expect(locale.operations.outcome.BanagerFailed.ChangedSinceShownDetail).toBeTruthy();
  }
  // The batch's own sentence for a plan worked out again stays its own.
  expect(en.planAgain.changed).toBe("This update changed after it was shown, so it didn't run. Open it again and confirm.");
});
