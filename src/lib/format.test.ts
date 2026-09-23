import { describe, expect, it } from "vitest";
import { displayToken, outcomeArgs, outcomeKey } from "./format";
import type { Outcome } from "./types";
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
