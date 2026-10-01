import { describe, expect, it } from "vitest";
import { commandsKnown } from "./commandsKnown";
import { NO_FACTS, type CommandFact } from "./types";

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
