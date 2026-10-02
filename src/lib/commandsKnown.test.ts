import { describe, expect, it } from "vitest";
import { commandsKnown, toolsNotJudged } from "./commandsKnown";
import { NO_FACTS, type CommandFact, type InstallReason } from "./types";

const row = (commands: CommandFact[]) => ({ facts: { ...NO_FACTS, commands } });

describe("commandsKnown", () => {
  const judged = row([{ name: "rg", state: "Runs" }]);
  const named = row([{ name: "rg", state: null }]);
  const none = row([]);

  it("is previewing while the first check's list is on screen, whatever the rows carry", () => {
    expect(commandsKnown([judged], true, "verdicts")).toBe("previewing");
    expect(commandsKnown([named], true, "names")).toBe("previewing");
  });

  it("is unjudged when no row has a verdict (no login PATH) or no name (the budget ran out)", () => {
    // The login shell's PATH not restored: names, so twins are known, but no verdict.
    expect(commandsKnown([named, none], false, "verdicts")).toBe("unjudged");
    expect(commandsKnown([named, none], false, "names")).toBe("known");
    // The folder read past its budget: nothing at all.
    expect(commandsKnown([none, none], false, "names")).toBe("unjudged");
    expect(commandsKnown([none, none], false, "verdicts")).toBe("unjudged");
  });

  it("is known once one row has what is needed, and with no rows at all", () => {
    expect(commandsKnown([none, judged], false, "verdicts")).toBe("known");
    expect(commandsKnown([], true, "verdicts")).toBe("known");
  });
});

describe("toolsNotJudged", () => {
  const tool = (commands: CommandFact[], reason: InstallReason = "Requested") => ({
    reason,
    facts: { ...NO_FACTS, commands },
  });

  it("counts each tool with a command it said nothing about, once however many it has", () => {
    expect(
      toolsNotJudged([
        tool([{ name: "rg", state: "Runs" }]),
        tool([{ name: "claude", state: null }]),
        tool([
          { name: "eslint", state: "Runs" },
          { name: "eslint-a", state: null },
          { name: "eslint-b", state: null },
        ]),
        tool([]),
      ]),
    ).toBe(2);
  });

  it("leaves out a Homebrew dependency, never judged, and a tool already counted as not found", () => {
    expect(
      toolsNotJudged([
        tool([{ name: "openssl", state: null }], "Dependency"),
        tool([
          { name: "tsx", state: { NotOnPath: { dir: "~/.npm-global/bin" } } },
          { name: "tsx-watch", state: null },
        ]),
        tool([{ name: "pip", state: null }], "Unknown"),
      ]),
    ).toBe(1);
  });
});
