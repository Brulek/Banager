import { describe, expect, it } from "vitest";
import { nameKey, namesUnderSeveralSources } from "./names";

describe("namesUnderSeveralSources", () => {
  it("names only what two sources or more list, case aside", () => {
    const names = namesUnderSeveralSources([
      { name: "black", instanceId: "brew:/opt/homebrew" },
      { name: "Black", instanceId: "pipx" },
      { name: "jq", instanceId: "brew:/opt/homebrew" },
      // One source listing a name twice is not two sources.
      { name: "node", instanceId: "brew:/opt/homebrew" },
      { name: "node", instanceId: "brew:/opt/homebrew" },
    ]);
    expect([...names]).toEqual(["black"]);
    expect(names.has(nameKey("BLACK"))).toBe(true);
  });

  it("counts two instances of one kind of source as two sources", () => {
    // Homebrew in /opt/homebrew and in /usr/local.
    const names = namesUnderSeveralSources([
      { name: "git", instanceId: "brew:/opt/homebrew" },
      { name: "git", instanceId: "brew:/usr/local" },
    ]);
    expect([...names]).toEqual(["git"]);
  });
});
