import { describe, expect, it } from "vitest";
import { displayToken, outcomeArgs, outcomeKey } from "./format";
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

describe("outcomeKey", () => {
  it("gives the user's own cancel its own words in both languages", () => {
    const cancelled: Outcome = "Cancelled";
    expect(outcomeKey(cancelled)).toBe("Cancelled");
    expect(outcomeArgs(cancelled)).toEqual({});
    expect(en.operations.outcome.Cancelled).toBe("You cancelled this");
    expect(zhCN.operations.outcome.Cancelled).toBe("你已取消");
  });

  it("words each NeedsAttention from the locale files, not from Rust's English", () => {
    // It used to carry Rust's own sentence ("package disappeared after
    // upgrade") and the drawer and the operation bar printed it inside the
    // translated frame, so a Chinese user read English there.
    const gone: Outcome = { NeedsAttention: "GoneAfterUpgrade" };
    expect(outcomeKey(gone)).toBe("NeedsAttention.GoneAfterUpgrade");
    expect(outcomeArgs(gone)).toEqual({});
    expect(en.operations.outcome.NeedsAttention.GoneAfterUpgrade).toBe(
      "Needs attention: the update reported success, but it's no longer installed",
    );
    expect(zhCN.operations.outcome.NeedsAttention.GoneAfterUpgrade).toBe(
      "需要留意：更新命令显示成功，但更新后它已不见了",
    );
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
