import { describe, expect, it } from "vitest";
import { commandMatching, searchMatch } from "./searchMatch";
import { NO_FACTS, type ArtifactKey, type CommandFact } from "./types";

const tool = (name: string, commands: string[], displayName = name) => ({
  display_name: displayName,
  key: { instance_id: "brew-1", kind: "Formula", name } as ArtifactKey,
  facts: { ...NO_FACTS, commands: commands.map((command): CommandFact => ({ name: command, state: "Runs" })) },
});

const ripgrep = tool("ripgrep", ["rg"]);
const python = tool("python@3.13", ["idle3.13", "pip3.13", "pydoc3.13", "python3.13"]);
const claude = tool("@anthropic-ai/claude-code", ["claude"], "Claude Code");
const grokBuild = tool("grok-build", ["agent"]);
const gh = tool("gh", ["gh"]);

describe("searchMatch", () => {
  it("finds ripgrep by rg, a command it puts on the Mac, and says so", () => {
    expect(searchMatch(ripgrep, "rg")).toEqual({ by: "command", command: "rg" });
  });

  it("finds python@3.13 by pip3.13, and by a command's start", () => {
    expect(searchMatch(python, "pip3.13")).toEqual({ by: "command", command: "pip3.13" });
    expect(searchMatch(python, "pip")).toEqual({ by: "command", command: "pip3.13" });
    expect(searchMatch(python, "pydoc")).toEqual({ by: "command", command: "pydoc3.13" });
  });

  it("finds a tool whose package name says nothing of its command", () => {
    expect(searchMatch(grokBuild, "agent")).toEqual({ by: "command", command: "agent" });
    expect(searchMatch(claude, "claude")).toEqual({ by: "name" });
  });

  it("matches by name first: no command hint where the name already matches", () => {
    expect(searchMatch(gh, "gh")).toEqual({ by: "name" });
    // "python" is in the name too: the name's match, anywhere in it, as before.
    expect(searchMatch(python, "python")).toEqual({ by: "name" });
    // An empty search matches every tool, by its name.
    expect(searchMatch(ripgrep, "")).toEqual({ by: "name" });
  });

  it("does not match a command by letters in its middle or end", () => {
    // "g" is in "rg", but the command starts with "r".
    expect(searchMatch(tool("finder", ["rg"]), "g")).toBeNull();
    expect(searchMatch(python, "3.13x")).toBeNull();
    expect(searchMatch(python, "ip3")).toBeNull();
    expect(searchMatch(grokBuild, "gent")).toBeNull();
  });

  it("is a match case aside, naming the command as the facts spell it", () => {
    expect(searchMatch(tool("thing", ["Thing-CLI"]), "thing-c")).toEqual({ by: "command", command: "Thing-CLI" });
  });

  it("matches nothing for a tool whose commands are not known", () => {
    expect(searchMatch(tool("ripgrep", []), "rg")).toBeNull();
  });
});

describe("commandMatching", () => {
  const facts = (names: string[]): CommandFact[] => names.map((name) => ({ name, state: null }));

  it("prefers the command that is exactly the search over one that starts with it", () => {
    expect(commandMatching(facts(["pip3", "pip3.13"]), "pip3.13")).toBe("pip3.13");
    expect(commandMatching(facts(["git", "git-shell"]), "git")).toBe("git");
    expect(commandMatching(facts(["git-shell", "gitk"]), "git")).toBe("git-shell");
  });

  it("matches nothing for an empty search", () => {
    expect(commandMatching(facts(["rg"]), "")).toBeNull();
  });
});
