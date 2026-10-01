import { describe, expect, it } from "vitest";
import {
  batchSizeOf,
  classify,
  countedTicks,
  hosting,
  itemSizeOf,
  MAX_BATCH_UNINSTALL,
  mergeKept,
  runOrder,
  selectAllAction,
  sizeCaveats,
  terminalCommands,
  tickable,
  toolsGoFirst,
  uninstalledWhileBusy,
  uninstallHeld,
  type BatchCandidate,
  type UninstallHolds,
} from "./batchUninstall";
import { artifactKeyId } from "../store/ui";
import type {
  ArtifactKind,
  CommandFact,
  InstalledArtifact,
  ManagerInstance,
  Measured,
  OpSummary,
  Plan,
  Sizes,
  Warning,
} from "./types";
import { NO_FACTS, NO_SIZES } from "./types";

const brew: ManagerInstance = {
  id: "brew:/opt/homebrew",
  adapter_id: "brew",
  exe_path: "/opt/homebrew/bin/brew",
  prefix: "/opt/homebrew",
  scope: "User",
  version: "7.0.3",
  unverified_version: null,
  read_only_reason: null,
  status: { unavailable: null, notes: [] },
};
const pipx: ManagerInstance = { ...brew, id: "pipx", adapter_id: "pipx", exe_path: "/opt/homebrew/bin/pipx" };
const ollama: ManagerInstance = {
  ...brew,
  id: "ollama:http://127.0.0.1:11434",
  adapter_id: "ollama",
  exe_path: "/opt/homebrew/bin/ollama",
};
const npm: ManagerInstance = { ...brew, id: "npm:/opt/homebrew", adapter_id: "npm", exe_path: "/opt/homebrew/bin/npm" };
const claude: ManagerInstance = {
  ...brew,
  id: "standalone-claude",
  adapter_id: "standalone-claude",
  exe_path: "/Users/you/.local/bin/claude",
};
const rustup: ManagerInstance = {
  ...brew,
  id: "standalone-rustup",
  adapter_id: "standalone-rustup",
  exe_path: "/Users/you/.cargo/bin/rustup",
};
const cargo: ManagerInstance = { ...brew, id: "cargo:/Users/you/.cargo", adapter_id: "cargo", exe_path: "/Users/you/.cargo/bin/cargo" };
const uv: ManagerInstance = { ...brew, id: "uv", adapter_id: "uv", exe_path: "/Users/you/.local/bin/uv" };

const runs = (...names: string[]): CommandFact[] => names.map((name) => ({ name, state: "Runs" }));

function artifact(
  instance: ManagerInstance,
  kind: ArtifactKind,
  name: string,
  more: Partial<InstalledArtifact> = {},
): InstalledArtifact {
  return {
    key: { instance_id: instance.id, kind, name },
    display_name: name,
    version: "1.0",
    reason: "Requested",
    description: null,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
    facts: NO_FACTS,
    ...more,
  };
}

function plan(target: InstalledArtifact, more: Partial<Plan> = {}): Plan {
  return {
    request: {
      kind: "Uninstall",
      instance_id: target.key.instance_id,
      artifact_kind: target.key.kind,
      name: target.key.name,
    },
    action: { Command: { program: "/opt/homebrew/bin/brew", args: ["uninstall", target.key.name], env: [] } },
    needs_password: false,
    locks: [target.key.instance_id],
    cancel_policy: "KillThenReconcile",
    warnings: [],
    affected: [],
    timeout_secs: 600,
    ...more,
  };
}

const instances = [brew, pipx, ollama, npm, claude, rustup, cargo, uv];
const instanceOf = (a: InstalledArtifact) => instances.find((i) => i.id === a.key.instance_id)!;

/** A ticked tool with its preview, as the sheet has it once every preview is back. */
function candidate(target: InstalledArtifact, preview: Partial<Plan> | string = {}): BatchCandidate {
  return {
    id: artifactKeyId(target.key),
    artifact: target,
    instance: instanceOf(target),
    name: target.display_name,
    plan: typeof preview === "string" ? null : plan(target, preview),
    planError: typeof preview === "string" ? preview : null,
  };
}

const formula = (name: string, more: Partial<InstalledArtifact> = {}) => artifact(brew, "Formula", name, more);
const pipxFormula = formula("pipx", { facts: { ...NO_FACTS, commands: runs("pipx") } });
const python = formula("python@3.13", { reason: "Dependency" });
const node = formula("node@22");
const openssl = formula("openssl@3", { reason: "Dependency" });
const wget = formula("wget");
const htop = formula("htop");
const git = formula("git");
const ollamaFormula = formula("ollama", { facts: { ...NO_FACTS, commands: runs("ollama") } });
const httpie = artifact(pipx, "Tool", "httpie");
const poetry = artifact(pipx, "Tool", "poetry");
const llama = artifact(ollama, "Model", "llama3.2:3b", { size_bytes: 2_019_393_189 });
const claudeCode = artifact(claude, "Binary", "claude", { display_name: "Claude Code" });
const rustupTool = artifact(rustup, "Binary", "rustup");

const everything = [pipxFormula, python, node, openssl, wget, htop, git, ollamaFormula, httpie, poetry, llama, claudeCode];

const ids = (items: Array<{ candidate: BatchCandidate }>) => items.map((item) => item.candidate.name);

describe("which rows get a checkbox", () => {
  const free: UninstallHolds = { preview: false, underway: false, uninstalled: false };

  it("is exactly where the row's own Uninstall is there and enabled", () => {
    expect(tickable(git, brew, free)).toBe(true);
    // No Uninstall at all: a read-only source, a tool that refuses.
    expect(tickable(git, { ...brew, read_only_reason: "ByDesign" }, free)).toBe(false);
    expect(tickable({ ...git, uninstall_blocked: "Pinned" }, brew, free)).toBe(false);
    expect(tickable({ ...git, uninstall_blocked: "NoSafeMethod" }, brew, free)).toBe(false);
    // Uninstall held: the source did not answer, it is updating its list,
    // the first check is not done, an uninstall of it is under way, or one
    // has just removed it.
    expect(tickable(git, { ...brew, status: { unavailable: "NotResponding", notes: [] } }, free)).toBe(false);
    expect(tickable(git, { ...brew, status: { unavailable: null, notes: ["IndexUpdating"] } }, free)).toBe(false);
    expect(tickable(git, brew, { ...free, preview: true })).toBe(false);
    expect(tickable(git, brew, { ...free, underway: true })).toBe(false);
    expect(tickable(git, brew, { ...free, uninstalled: true })).toBe(false);
    // A note that holds nothing leaves it.
    expect(tickable(git, { ...brew, status: { unavailable: null, notes: ["IndexMayBeStale"] } }, free)).toBe(true);
  });

  it("says a held Uninstall only of a row that offers one", () => {
    expect(uninstallHeld(git, brew, { ...free, preview: true })).toBe(true);
    expect(uninstallHeld({ ...git, uninstall_blocked: "Pinned" }, brew, { ...free, preview: true })).toBe(false);
    expect(uninstallHeld(git, brew, free)).toBe(false);
  });

  it("counts the ticked rows the list shows, and none it hides", () => {
    const rows = [{ id: "a" }, { id: "b" }, { id: "c" }];
    expect(countedTicks(rows, ["c", "hidden", "a"])).toEqual([{ id: "a" }, { id: "c" }]);
    expect(countedTicks(rows, [])).toEqual([]);
  });

  it("selects every row shown up to the most a batch takes, and otherwise only clears", () => {
    expect(selectAllAction(3, 0)).toBe("select");
    expect(selectAllAction(3, 2)).toBe("select");
    expect(selectAllAction(3, 3)).toBe("clear");
    expect(selectAllAction(MAX_BATCH_UNINSTALL, 0)).toBe("select");
    expect(selectAllAction(MAX_BATCH_UNINSTALL + 1, 0)).toBe("clear");
    expect(selectAllAction(MAX_BATCH_UNINSTALL + 1, 4)).toBe("clear");
  });
});

describe("a row a batch has already uninstalled while its source is busy", () => {
  const op = (id: number, more: Partial<OpSummary>): OpSummary => ({
    id,
    kind: "Uninstall",
    instance_id: brew.id,
    artifact_kind: "Formula",
    name: "git",
    status: "Done",
    outcome: "Succeeded",
    argv_preview: [],
    cancel_policy: "KillThenReconcile",
    ...more,
  });

  it("is said uninstalled while another operation on its source still runs", () => {
    const running = op(2, { name: "wget", status: "Running", outcome: null });
    const gitId = artifactKeyId(git.key);
    expect(uninstalledWhileBusy([op(1, {}), running])).toEqual(new Set([gitId]));
    // Its source free again: the next check reads it.
    expect(uninstalledWhileBusy([op(1, {})])).toEqual(new Set());
    // Busy with another source's operation: not this one's.
    expect(uninstalledWhileBusy([op(1, {}), { ...running, instance_id: npm.id }])).toEqual(new Set());
    // Its newest uninstall did not succeed.
    const failed = op(3, { outcome: { Failed: { exit_code: 1, summary: "" } } });
    expect(uninstalledWhileBusy([op(1, {}), failed, running])).toEqual(new Set());
    // The one running is no row that has gone.
    expect(uninstalledWhileBusy([running])).toEqual(new Set());
  });
});

describe("which ticked tools a batch uninstalls", () => {
  it("includes the plain ones, in the list's order", () => {
    const result = classify([candidate(wget), candidate(git), candidate(claudeCode)], everything);
    expect(ids(result.included)).toEqual(["wget", "git", "Claude Code"]);
    expect(result.excluded).toEqual([]);
  });

  it("leaves out a refused preview with the backend's words (X1)", () => {
    const refused = JSON.stringify({ kind: "uninstall_blocked", reason: "Pinned" });
    const result = classify([candidate(wget, refused), candidate(git)], everything);
    expect(ids(result.included)).toEqual(["git"]);
    expect(result.excluded).toEqual([{ candidate: candidate(wget, refused), reason: { kind: "refused", raw: refused } }]);
  });

  it("leaves out what cannot be cancelled once started (X2), what deletes for good (X3), and a cask's unseen steps (X4)", () => {
    const rustupPlan: Partial<Plan> = {
      cancel_policy: "NoCancel",
      warnings: [{ DeletesCargoHome: { path: "~/.cargo" } }],
    };
    const cask = artifact(brew, "Cask", "adobe-creative-cloud");
    const deleting = artifact(brew, "Cask", "zoom");
    const result = classify(
      [
        candidate(rustupTool, rustupPlan),
        candidate(deleting, {
          warnings: [{ CaskUninstallStep: { step: "Deletes", items: ["/Library/Zoom"] } }],
        }),
        candidate(cask, { warnings: [{ UninstallScope: { what: "HomebrewCaskStepsUnseen" } }] }),
        candidate(artifact(brew, "Cask", "unreadable"), { warnings: [{ UninstallScope: { what: "HomebrewCask" } }] }),
        candidate(artifact(brew, "Cask", "pkg"), { warnings: [{ UninstallScope: { what: "HomebrewCaskStepsOnlyUnseen" } }] }),
        candidate(artifact(brew, "Cask", "iterm2"), { warnings: [{ UninstallScope: { what: "HomebrewCaskSteps" } }] }),
      ],
      everything,
    );
    expect(ids(result.included)).toEqual(["iterm2"]);
    expect(result.excluded.map((item) => [item.candidate.name, item.reason.kind])).toEqual([
      // The first that holds: rustup's own uninstall is both.
      ["rustup", "noCancel"],
      ["zoom", "permanent"],
      ["adobe-creative-cloud", "unseen"],
      ["unreadable", "unseen"],
      ["pkg", "unseen"],
    ]);
  });

  describe("a package other sources run on (X1b, Warning.NeededBySource)", () => {
    const npmOnIt: Warning = { NeededBySource: { instance_id: npm.id, program: true, tools: 4 } };
    const pipxToolsOnIt: Warning = { NeededBySource: { instance_id: pipx.id, program: false, tools: 2 } };

    it("leaves it out with the sources and what Homebrew names, before any other reason", () => {
      const result = classify(
        [
          candidate(node, { warnings: [npmOnIt], cancel_policy: "NoCancel" }),
          candidate(python, { affected: ["pipx"], warnings: [{ WouldBreak: { names: ["pipx"] } }, pipxToolsOnIt] }),
          candidate(git),
        ],
        everything,
      );
      expect(ids(result.included)).toEqual(["git"]);
      expect(result.excluded.map((item) => [item.candidate.name, item.reason])).toEqual([
        ["node@22", { kind: "neededBy", sources: [npmOnIt.NeededBySource], dependents: [] }],
        ["python@3.13", { kind: "neededBy", sources: [pipxToolsOnIt.NeededBySource], dependents: ["pipx"] }],
      ]);
    });

    it("leaves it out even with every tool of that source ticked: their uninstalls run beside it", () => {
      const codex = artifact(npm, "Package", "@openai/codex");
      const nodeLinked = formula("node", { facts: { ...NO_FACTS, commands: runs("node", "npm", "npx") } });
      const result = classify(
        [candidate(codex), candidate(nodeLinked, { warnings: [{ NeededBySource: { instance_id: npm.id, program: true, tools: 1 } }] })],
        [...everything, codex, nodeLinked],
      );
      expect(ids(result.included)).toEqual(["@openai/codex"]);
      // Not X5's reason, though X5 would leave it out too: the preview's own.
      expect(result.excluded.map((item) => item.reason.kind)).toEqual(["neededBy"]);
      // Its reason can say that, once this batch has run, it can go.
      const reason = result.excluded[0].reason;
      if (reason.kind !== "neededBy") throw new Error("expected neededBy");
      expect(toolsGoFirst(reason, result.included)).toBe(true);
      // Not with fewer of its tools in the batch, nor with a Homebrew
      // dependent named, nor for some of a pipx's tools, which are not known.
      expect(toolsGoFirst({ ...reason, sources: [{ ...reason.sources[0], tools: 2 }] }, result.included)).toBe(false);
      expect(toolsGoFirst({ ...reason, dependents: ["yarn"] }, result.included)).toBe(false);
      expect(toolsGoFirst({ ...reason, sources: [{ ...reason.sources[0], program: false }] }, result.included)).toBe(false);
      expect(toolsGoFirst(reason, [])).toBe(false);
    });

    it("keeps what it depends on, as any package left out does (X6)", () => {
      // Homebrew's pipx, which the pipx source runs, and python@3.13, which
      // only it still needs among what is ticked.
      const result = classify(
        [
          candidate(pipxFormula, { warnings: [{ NeededBySource: { instance_id: pipx.id, program: true, tools: 2 } }] }),
          candidate(python, { affected: ["pipx"] }),
        ],
        everything,
      );
      expect(ids(result.included)).toEqual([]);
      expect(result.excluded.map((item) => [item.candidate.name, item.reason.kind])).toEqual([
        ["pipx", "neededBy"],
        ["python@3.13", "neededByExcluded"],
      ]);
    });
  });

  it("includes a tool whose dependents could not be checked, after the rest of its source", () => {
    const result = classify(
      [candidate(htop, { warnings: ["DependentsUnknown"] }), candidate(claudeCode), candidate(git)],
      everything,
    );
    expect(ids(result.included)).toEqual(["Claude Code", "git", "htop"]);
  });

  it("puts a ticked dependent first and its dependency after it (pipx, then python@3.13)", () => {
    const result = classify([candidate(python, { affected: ["pipx"] }), candidate(pipxFormula)], everything);
    expect(ids(result.included)).toEqual(["pipx", "python@3.13"]);
    expect(result.included[1].after).toEqual([artifactKeyId(pipxFormula.key)]);
    expect(result.included[0].after).toEqual([]);
  });

  it("leaves out a dependency some software not ticked still needs, naming it (openssl@3)", () => {
    const result = classify(
      [candidate(openssl, { affected: ["node@22", "postgresql@17", "python@3.13", "wget"] }), candidate(node)],
      [...everything, formula("postgresql@17")],
    );
    expect(ids(result.included)).toEqual(["node@22"]);
    expect(result.excluded[0].reason).toEqual({
      kind: "stillNeeded",
      names: ["postgresql@17", "python@3.13", "wget"],
    });
  });

  it("names every dependent that stays, a ticked one left out among them", () => {
    // python@3.13 is ticked, and left out (pipx, not ticked, needs it):
    // it still needs openssl@3 too.
    const result = classify(
      [
        candidate(openssl, { affected: ["node@22", "postgresql@17", "python@3.13", "wget"] }),
        candidate(node),
        candidate(python, { affected: ["pipx"] }),
      ],
      [...everything, formula("postgresql@17")],
    );
    expect(ids(result.included)).toEqual(["node@22"]);
    expect(result.excluded.map((item) => [item.candidate.name, item.reason])).toEqual([
      ["openssl@3", { kind: "stillNeeded", names: ["postgresql@17", "python@3.13", "wget"] }],
      ["python@3.13", { kind: "stillNeeded", names: ["pipx"] }],
    ]);
  });

  it("names a dependent Homebrew gave that the list does not have, as Homebrew spells it", () => {
    const result = classify([candidate(python, { affected: ["somethingnew"] })], everything);
    expect(result.excluded[0].reason).toEqual({ kind: "stillNeeded", names: ["somethingnew"] });
  });

  it("leaves out an Ollama model's program and a pipx tool's, and what only they still needed (X5, then X6)", () => {
    const result = classify(
      [
        candidate(llama),
        candidate(ollamaFormula),
        candidate(httpie),
        candidate(pipxFormula),
        candidate(python, { affected: ["pipx"] }),
      ],
      everything,
    );
    expect(ids(result.included)).toEqual(["llama3.2:3b", "httpie"]);
    expect(result.excluded.map((item) => [item.candidate.name, item.reason])).toEqual([
      ["ollama", { kind: "host", by: [artifactKeyId(llama.key)] }],
      ["pipx", { kind: "host", by: [artifactKeyId(httpie.key)] }],
      ["python@3.13", { kind: "neededByExcluded", ids: [artifactKeyId(pipxFormula.key)] }],
    ]);
  });

  it("does not take one copy of a tool for the other's program", () => {
    const npmClaude = artifact(npm, "Package", "@anthropic-ai/claude-code", { facts: { ...NO_FACTS, commands: runs("claude") } });
    const own = { ...claudeCode, facts: { ...NO_FACTS, commands: runs("claude") } };
    const result = classify([candidate(own), candidate(npmClaude)], [own, npmClaude]);
    expect(ids(result.included)).toEqual(["Claude Code", "@anthropic-ai/claude-code"]);
  });

  it("matches Homebrew's names within the tool's own source only", () => {
    const intel: ManagerInstance = { ...brew, id: "brew:/usr/local" };
    instances.push(intel);
    try {
      const otherPipx = artifact(intel, "Formula", "pipx");
      // `pipx` ticked on the other Homebrew: not the one python@3.13 means.
      const result = classify(
        [candidate(python, { affected: ["pipx"] }), candidate(otherPipx)],
        [...everything, otherPipx],
      );
      expect(result.excluded.map((item) => item.reason)).toEqual([{ kind: "stillNeeded", names: ["pipx"] }]);
    } finally {
      instances.pop();
    }
  });

  it("matches a tapped name by its last part, as Homebrew does", () => {
    const tapped = artifact(brew, "Cask", "gautham-v/tap/claudebar");
    const lib = formula("libbar");
    const result = classify([candidate(lib, { affected: ["claudebar"] }), candidate(tapped)], [...everything, tapped, lib]);
    expect(ids(result.included)).toEqual(["gautham-v/tap/claudebar", "libbar"]);
  });

  it("covers a name a formula and a cask share only when both are ticked", () => {
    const fooFormula = formula("foo");
    const fooCask = artifact(brew, "Cask", "foo");
    const lib = formula("libfoo");
    const all = [...everything, fooFormula, fooCask, lib];
    const one = classify([candidate(lib, { affected: ["foo"] }), candidate(fooFormula)], all);
    expect(one.excluded.map((item) => item.reason)).toEqual([{ kind: "stillNeeded", names: ["foo"] }]);
    const both = classify([candidate(lib, { affected: ["foo"] }), candidate(fooFormula), candidate(fooCask)], all);
    expect(ids(both.included)).toEqual(["foo", "foo", "libfoo"]);
  });

  it("cascades through a chain: a dependency left out keeps what it depends on", () => {
    // c uses b, b uses a. Ticking a and b, not c: b is still needed, and so a.
    const a = formula("liba");
    const b = formula("libb");
    const all = [...everything, a, b, formula("c")];
    const result = classify([candidate(a, { affected: ["libb"] }), candidate(b, { affected: ["c"] })], all);
    expect(result.included).toEqual([]);
    expect(result.excluded.map((item) => [item.candidate.name, item.reason.kind])).toEqual([
      ["liba", "neededByExcluded"],
      ["libb", "stillNeeded"],
    ]);
    // All three: c, then b, then a.
    const c = formula("c");
    const chain = classify([candidate(a, { affected: ["libb"] }), candidate(b, { affected: ["c"] }), candidate(c)], all);
    expect(ids(chain.included)).toEqual(["c", "libb", "liba"]);
  });

  it("leaves out tools whose dependencies loop, and what needs them (X7)", () => {
    const a = formula("loopa");
    const b = formula("loopb");
    const result = classify(
      [candidate(a, { affected: ["loopb"] }), candidate(b, { affected: ["loopa"] }), candidate(git)],
      [...everything, a, b],
    );
    expect(ids(result.included)).toEqual(["git"]);
    expect(result.excluded.map((item) => item.reason.kind)).toEqual(["cycle", "cycle"]);
  });

  it("orders stably: dependents first, otherwise the list's order", () => {
    const a = formula("a-lib");
    const z = formula("z-app");
    const result = classify(
      [candidate(a, { affected: ["z-app"] }), candidate(git), candidate(z), candidate(wget)],
      [...everything, a, z],
    );
    expect(ids(result.included)).toEqual(["git", "z-app", "a-lib", "wget"]);
  });

  it("runs a tool with unknown dependents first when a known dependent of another needs it gone", () => {
    const order = runOrder(
      [
        { id: "y", instance: brew },
        { id: "u", instance: brew },
      ],
      new Map([["y", ["u"]]]),
      (item) => item.id === "u",
    );
    expect(order.map((item) => item.id)).toEqual(["u", "y"]);
  });
});

describe("what the included tools' commands run once they are gone", () => {
  const npmClaude = artifact(npm, "Package", "@anthropic-ai/claude-code", {
    facts: { ...NO_FACTS, commands: runs("claude") },
  });
  const own: InstalledArtifact = {
    ...claudeCode,
    facts: { ...NO_FACTS, commands: [{ name: "claude", state: { ShadowedBy: { by: npmClaude.key } } }] },
  };

  it("is another copy where one waits behind it, and nothing where none does", () => {
    const gitWithCommands = { ...git, facts: { ...NO_FACTS, commands: runs("git", "scalar") } };
    const { lost, takenOver } = terminalCommands([npmClaude, gitWithCommands], [npmClaude, own, gitWithCommands]);
    expect(lost).toEqual(["git", "scalar"]);
    expect(takenOver.get(artifactKeyId(npmClaude.key))).toEqual([{ command: "claude", by: [own] }]);
  });

  it("is nothing for both copies when both go", () => {
    const { lost, takenOver } = terminalCommands([npmClaude, own], [npmClaude, own]);
    expect(lost).toEqual(["claude"]);
    expect(takenOver.size).toBe(0);
  });

  it("does not call a command Banager did not judge lost, but names it as going with its copy", () => {
    const unjudged = { ...git, facts: { ...NO_FACTS, commands: [{ name: "git", state: null }] } };
    expect(terminalCommands([unjudged], [unjudged])).toEqual({ lost: [], takenOver: new Map(), unjudged: ["git"] });
  });

  it("names nothing for a tool whose commands it could not name", () => {
    const bare = { ...git, facts: { ...NO_FACTS, commands: [] } };
    expect(terminalCommands([bare], [bare])).toEqual({ lost: [], takenOver: new Map(), unjudged: [] });
  });
});

describe("what the included tools leave behind", () => {
  const about = (bytes: number): Measured => ({ bytes, partial: false, at_least: false });
  const keeps = (path: string, size: Measured | null): Warning => ({
    KeepsData: { path, what: "ToolData", size, left_out: [] },
  });

  it("names each path once, with whose it is and the line that has its size", () => {
    const merged = mergeKept([
      {
        name: "Claude Code",
        warnings: [
          { WillKeep: { path: "~/.claude", what: "SettingsAndHistory" } },
          { WillKeep: { path: "~/.claude.json", what: "Settings" } },
          { WillTrash: { path: "~/.local/bin/claude", what: "Launcher" } },
        ],
      },
      { name: "@anthropic-ai/claude-code", warnings: [keeps("~/.claude", about(412_300_000))] },
      { name: "Codex", warnings: [keeps("~/.codex", about(38_400_000))] },
    ]);
    expect(merged.warnings).toEqual([
      keeps("~/.claude", about(412_300_000)),
      { WillKeep: { path: "~/.claude.json", what: "Settings" } },
      keeps("~/.codex", about(38_400_000)),
    ]);
    expect(merged.owners.get("~/.claude")).toEqual(["Claude Code", "@anthropic-ai/claude-code"]);
    expect(merged.owners.get("~/.codex")).toEqual(["Codex"]);
  });
});

describe("what the included tools take", () => {
  const measured = (bytes: number, more: Partial<Measured> = {}): Measured => ({
    bytes,
    partial: false,
    at_least: false,
    ...more,
  });
  const sizes = (entries: Array<[InstalledArtifact, Measured | null, string?]>): Sizes => ({
    ...NO_SIZES,
    round: 1,
    done: true,
    artifacts: entries.map(([target, size, version]) => ({
      key: target.key,
      version: version ?? target.version,
      measured: size,
      old_versions: null,
    })),
  });

  it("is each tool's size as By Size shows it, or nothing", () => {
    const served = sizes([
      [git, measured(20_000_000)],
      [wget, measured(0)],
      [htop, null],
      [node, measured(5_000_000), "0.9"],
    ]);
    expect(itemSizeOf(served, git)).toEqual(measured(20_000_000));
    expect(itemSizeOf(served, llama)).toEqual(measured(2_019_393_189));
    // A measured 0, one still measured, one measured at another version, one never measured.
    expect(itemSizeOf(served, wget)).toBeNull();
    expect(itemSizeOf(served, htop)).toBeNull();
    expect(itemSizeOf(served, node)).toBeNull();
    expect(itemSizeOf(served, python)).toBeNull();
  });

  it("adds them up, about, or more where one is unknown or cut short, or partly unread", () => {
    const served = sizes([
      [git, measured(20_000_000)],
      [wget, measured(5_000_000, { partial: true })],
      [htop, measured(1_000_000, { at_least: true })],
    ]);
    expect(batchSizeOf(served, [git])).toEqual({ measured: measured(20_000_000), unknown: 0 });
    expect(batchSizeOf(served, [git, llama])).toEqual({ measured: measured(2_039_393_189), unknown: 0 });
    expect(batchSizeOf(served, [git, wget])).toEqual({ measured: measured(25_000_000, { partial: true }), unknown: 0 });
    expect(batchSizeOf(served, [git, htop])).toEqual({ measured: measured(21_000_000, { at_least: true }), unknown: 0 });
    expect(batchSizeOf(served, [git, python])).toEqual({ measured: measured(20_000_000, { at_least: true }), unknown: 1 });
    expect(batchSizeOf(served, [python, node])).toEqual({ measured: null, unknown: 2 });
  });

  it("says why the space freed may be less, only where it applies", () => {
    const trashPlan = plan(claudeCode, {
      action: { TrashPaths: { paths: ["/Users/you/.local/bin/claude"] } },
      warnings: [{ WillKeep: { path: "~/.claude", what: "SettingsAndHistory" } }],
    });
    expect(sizeCaveats([{ artifact: claudeCode, instance: claude, plan: trashPlan }], 0)).toEqual([
      "batchUninstall.takesTrash",
      "batchUninstall.takesKept",
      "sizes.programOnly",
    ]);
    expect(sizeCaveats([{ artifact: git, instance: brew, plan: plan(git) }], 2)).toEqual([
      "batchUninstall.takesUnknown",
      "batchUninstall.takesShared",
    ]);
    expect(sizeCaveats([{ artifact: llama, instance: ollama, plan: plan(llama) }], 0)).toEqual([
      "batchUninstall.takesShared",
    ]);
    const tokei = artifact(cargo, "Binary", "tokei");
    expect(sizeCaveats([{ artifact: tokei, instance: cargo, plan: plan(tokei) }], 0)).toEqual(["sizes.programOnly"]);
    const ruff = artifact(uv, "Tool", "ruff");
    expect(sizeCaveats([{ artifact: ruff, instance: uv, plan: plan(ruff) }], 0)).toEqual(["batchUninstall.takesShared"]);
    expect(sizeCaveats([{ artifact: httpie, instance: pipx, plan: plan(httpie) }], 0)).toEqual([]);
  });
});

describe("hosting (X5's match, by command name)", () => {
  const codex = artifact(npm, "Package", "@openai/codex");
  const nodeLinked = formula("node", { facts: { ...NO_FACTS, commands: runs("node", "npm", "npx") } });

  it("takes Homebrew's pipx for the pipx tools' program, and nothing else for it", () => {
    expect(hosting(pipxFormula, brew, pipx, everything)).toEqual({ manages: true, runs: false });
    expect(hosting(git, brew, pipx, everything)).toBeNull();
    expect(hosting(httpie, pipx, pipx, everything)).toBeNull();
  });

  it("says npm's packages run on a Homebrew node, and that one with npm also manages them", () => {
    expect(hosting(nodeLinked, brew, npm, [...everything, codex, nodeLinked])).toEqual({ manages: true, runs: true });
  });

  it("takes no program outside this Homebrew's folder, nor a keg-only formula's beside the one Terminal finds", () => {
    const usrLocalNpm: ManagerInstance = { ...npm, id: "npm:/usr/local", exe_path: "/usr/local/bin/npm" };
    const node22 = formula("node@22", { facts: { ...NO_FACTS, commands: [{ name: "node", state: null }] } });
    expect(hosting(nodeLinked, brew, usrLocalNpm, [nodeLinked])).toBeNull();
    // Keg-only, with a linked `node` beside it: not the one npm runs on.
    expect(hosting(node22, brew, npm, [node22, nodeLinked, codex])).toBeNull();
    // Alone, it is the only `node` this Homebrew has.
    expect(hosting(node22, brew, npm, [node22, codex])).toEqual({ manages: false, runs: true });
  });

  it("leaves a Homebrew node out of a batch with an npm package, as it does pipx with a pipx tool (X5)", () => {
    const all = [...everything, codex, nodeLinked];
    const result = classify([candidate(codex), candidate(nodeLinked)], all);
    expect(result.included.map((item) => item.candidate.name)).toEqual(["@openai/codex"]);
    expect(result.excluded.map((item) => item.reason)).toEqual([{ kind: "host", by: [artifactKeyId(codex.key)] }]);
  });
});
