import { describe, expect, it } from "vitest";
import {
  commandGroups,
  commandsSaidOnRows,
  judgedCommands,
  stateId,
  twinsByArtifact,
  twinVerdict,
  unusedCopies,
  withoutJudgedPathNotices,
} from "./commands";
import { artifactKeyId } from "../store/ui";
import type { SourceNoticeSpec } from "./sources";
import type { ArtifactKey, CommandFact, InstalledArtifact } from "./types";
import { NO_FACTS } from "./types";

const npmKey: ArtifactKey = { instance_id: "npm:/opt/homebrew", kind: "Package", name: "@anthropic-ai/claude-code" };

function artifact(key: ArtifactKey, family: string | null, commands: CommandFact[]): InstalledArtifact {
  return {
    key,
    display_name: key.name,
    version: "1.0",
    reason: "Requested",
    description: null,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
    facts: { ...NO_FACTS, family, commands },
  };
}

describe("stateId", () => {
  it("is equal exactly for the same verdict", () => {
    expect(stateId("Runs")).toBe(stateId("Runs"));
    expect(stateId({ ShadowedBy: { by: npmKey } })).toBe(stateId({ ShadowedBy: { by: { ...npmKey } } }));
    expect(stateId({ ShadowedBy: { by: npmKey } })).not.toBe(stateId({ ShadowedBy: { by: null } }));
    expect(stateId({ NotOnPath: { dir: "~/.local/bin" } })).not.toBe(stateId({ NotOnPath: { dir: "~/.grok/bin" } }));
    expect(stateId({ NotOnPath: { dir: "~/.local/bin" } })).not.toBe(stateId("Runs"));
  });
});

describe("commandGroups", () => {
  it("puts commands with one verdict on one line, in name order, and leaves out the unjudged", () => {
    // rustup's commands behind Homebrew's `rust` for `cargo` and `rustc`.
    const groups = commandGroups([
      { name: "cargo", state: { ShadowedBy: { by: npmKey } } },
      { name: "cargo-fmt", state: "Runs" },
      { name: "curl", state: null },
      { name: "rustc", state: { ShadowedBy: { by: npmKey } } },
      { name: "rustup", state: "Runs" },
    ]);
    expect(groups).toEqual([
      { names: ["cargo", "rustc"], state: { ShadowedBy: { by: npmKey } } },
      { names: ["cargo-fmt", "rustup"], state: "Runs" },
    ]);
    expect(commandGroups([{ name: "curl", state: null }])).toEqual([]);
    expect(commandGroups([])).toEqual([]);
  });
});

describe("twinsByArtifact", () => {
  const nativeKey: ArtifactKey = { instance_id: "standalone-claude", kind: "Binary", name: "claude" };
  const caskKey: ArtifactKey = { instance_id: "brew:/opt/homebrew", kind: "Cask", name: "claude-code" };
  const id = (key: ArtifactKey) => `${key.instance_id}|${key.kind}|${key.name}`;

  it("pairs copies of one tool that put the same command on the Mac", () => {
    const npm = artifact(npmKey, "claude-code", [{ name: "claude", state: "Runs" }]);
    const native = artifact(nativeKey, "claude-code", [{ name: "claude", state: { ShadowedBy: { by: npmKey } } }]);
    const twins = twinsByArtifact([npm, native]);
    expect(twins.get(id(npmKey))).toEqual([{ artifact: native, commands: ["claude"] }]);
    expect(twins.get(id(nativeKey))).toEqual([{ artifact: npm, commands: ["claude"] }]);
  });

  it("counts a third copy, and a copy with no verdict still shares its name", () => {
    const npm = artifact(npmKey, "claude-code", [{ name: "claude", state: "Runs" }]);
    const native = artifact(nativeKey, "claude-code", [{ name: "claude", state: null }]);
    const cask = artifact(caskKey, "claude-code", [{ name: "claude", state: { ShadowedBy: { by: npmKey } } }]);
    expect(twinsByArtifact([npm, native, cask]).get(id(npmKey))?.map((twin) => twin.artifact.key)).toEqual([
      nativeKey,
      caskKey,
    ]);
  });

  it("does not pair two programs with one name but no shared tool, or no shared name", () => {
    // Homebrew's formula `grok`, a regular-expression tool, and Grok Build.
    const formula = artifact({ instance_id: "brew:/opt/homebrew", kind: "Formula", name: "grok" }, null, [
      { name: "grok", state: "Runs" },
    ]);
    const grok = artifact({ instance_id: "standalone-grok", kind: "Binary", name: "grok" }, "grok-build", [
      { name: "grok", state: { ShadowedBy: { by: formula.key } } },
    ]);
    expect(twinsByArtifact([formula, grok]).size).toBe(0);
    // One tool, two copies with no command in common.
    const npm = artifact(npmKey, "claude-code", [{ name: "claude", state: "Runs" }]);
    const other = artifact(nativeKey, "claude-code", [{ name: "claude-helper", state: "Runs" }]);
    expect(twinsByArtifact([npm, other]).size).toBe(0);
    // A copy with no commands found at all.
    expect(twinsByArtifact([npm, artifact(nativeKey, "claude-code", [])]).size).toBe(0);
  });
});

describe("withoutJudgedPathNotices", () => {
  const nativeKey: ArtifactKey = { instance_id: "standalone-claude", kind: "Binary", name: "claude" };
  const notice = (titleKey: string, command?: string): SourceNoticeSpec => ({
    id: titleKey,
    variant: "info",
    titleKey,
    descriptionKey: titleKey.replace(".title", ".description"),
    values: command === undefined ? { source: "Claude Code" } : { source: "Claude Code", command },
  });
  const pathNotices = [
    notice("sourceNotice.notOnPath.title", "claude"),
    notice("sourceNotice.shadowedByHomebrew.title", "claude"),
    notice("sourceNotice.shadowedByNpm.title", "claude"),
    notice("sourceNotice.shadowedByOther.title", "claude"),
  ];
  const others = [notice("sourceNotice.launcherOnly.title", "claude"), notice("sourceNotice.notRunning.title")];

  it("drops the launcher's PATH sentence when the command group judged that command", () => {
    const native = artifact(nativeKey, "claude-code", [{ name: "claude", state: { ShadowedBy: { by: npmKey } } }]);
    expect(withoutJudgedPathNotices([...pathNotices, ...others], judgedCommands(native))).toEqual(others);
  });

  it("keeps it when nothing was judged about the launcher, or the verdict is for another command", () => {
    const unjudged = artifact(nativeKey, "claude-code", [{ name: "claude", state: null }]);
    expect(withoutJudgedPathNotices(pathNotices, judgedCommands(unjudged))).toEqual(pathNotices);
    expect(withoutJudgedPathNotices(pathNotices, undefined)).toEqual(pathNotices);
    const elsewhere = artifact(nativeKey, "claude-code", [
      { name: "claude", state: null },
      { name: "claude-helper", state: "Runs" },
    ]);
    expect(withoutJudgedPathNotices(pathNotices, judgedCommands(elsewhere))).toEqual(pathNotices);
  });

  // r24 W8: the lists give a source's notices, held to what the rows of
  // its tools say, as the inspector holds them to its command group.
  it("holds a source's notices to the commands its rows say, a copy's verdict, and to no other source's", () => {
    const native = artifact(nativeKey, "claude-code", [{ name: "claude", state: { ShadowedBy: { by: npmKey } } }]);
    const npm = artifact(npmKey, "claude-code", [{ name: "claude", state: "Runs" }]);
    const said = commandsSaidOnRows([npm, native]);
    // 「运行的是npm装的那一份」 on Claude Code's row, 「运行的是这一份」 on npm's.
    expect(said.get("standalone-claude")).toEqual(new Set(["claude"]));
    expect(said.get("npm:/opt/homebrew")).toEqual(new Set(["claude"]));
    expect(withoutJudgedPathNotices([...pathNotices, ...others], said.get("standalone-claude"))).toEqual(others);
  });

  // r36 V3: Grok Build from Homebrew's cask (`grok` and `agent` in
  // /opt/homebrew/bin) and from its own installer (~/.grok/bin), with
  // Cursor's install script's `~/.local/bin/agent` first on PATH. `grok`
  // runs the cask's copy and `agent` Cursor's, so no one verdict covers
  // both shared commands -- and the rows still say 「装了两份」, the
  // inspector, for `grok`, 「运行的是Homebrew装的那一份」. Each command is
  // counted on its own: the notice about `grok` goes, never "couldn't
  // confirm whether it's another copy" beside them.
  it("counts each shared command on its own where the copies' commands run different programs", () => {
    const caskKey: ArtifactKey = { instance_id: "brew:/opt/homebrew", kind: "Cask", name: "grok-build" };
    const ownKey: ArtifactKey = { instance_id: "standalone-grok", kind: "Binary", name: "grok" };
    const cask = artifact(caskKey, "grok-build", [
      { name: "agent", state: { ShadowedBy: { by: null } } },
      { name: "grok", state: "Runs" },
    ]);
    const own = artifact(ownKey, "grok-build", [
      { name: "agent", state: { ShadowedBy: { by: null } } },
      { name: "grok", state: { ShadowedBy: { by: caskKey } } },
    ]);
    const said = commandsSaidOnRows([cask, own]);
    expect(said.get("standalone-grok")).toEqual(new Set(["grok"]));
    expect(said.get("brew:/opt/homebrew")).toEqual(new Set(["grok"]));
    const grokNotice: SourceNoticeSpec = {
      ...notice("sourceNotice.shadowedByHomebrew.title", "grok"),
      values: { source: "Grok Build", command: "grok" },
    };
    expect(withoutJudgedPathNotices([grokNotice, ...others], said.get("standalone-grok"))).toEqual(others);
    // Another program first for both of them: nothing is said of either.
    const behindOthers = artifact(ownKey, "grok-build", [
      { name: "agent", state: { ShadowedBy: { by: null } } },
      { name: "grok", state: { ShadowedBy: { by: null } } },
    ]);
    expect(commandsSaidOnRows([cask, behindOthers]).has("standalone-grok")).toBe(false);
  });

  it("keeps the notice where the rows say nothing of the command: no copy, no one verdict, or not on PATH", () => {
    // W2-9: npm's `claude` is a wrapper of no family, no copy of Claude Code;
    // the row says nothing, and the notice, with its Show, is all there is.
    const wrapperKey: ArtifactKey = { ...npmKey, name: "cc-wrapper" };
    const wrapper = artifact(wrapperKey, null, [{ name: "claude", state: "Runs" }]);
    const native = artifact(nativeKey, "claude-code", [{ name: "claude", state: { ShadowedBy: { by: wrapperKey } } }]);
    expect(commandsSaidOnRows([wrapper, native]).has("standalone-claude")).toBe(false);
    // Two copies, nothing judged: 「装了两份」 with no word on which runs.
    const unjudged = [
      artifact(npmKey, "claude-code", [{ name: "claude", state: null }]),
      artifact(nativeKey, "claude-code", [{ name: "claude", state: null }]),
    ];
    expect(commandsSaidOnRows(unjudged).size).toBe(0);
    // Terminal can't find Claude Code's copy: its 「终端里找不到」 agrees with
    // the notice, which stays.
    const lost = [
      artifact(npmKey, "claude-code", [{ name: "claude", state: "Runs" }]),
      artifact(nativeKey, "claude-code", [{ name: "claude", state: { NotOnPath: { dir: "~/.local/bin" } } }]),
    ];
    const said = commandsSaidOnRows(lost);
    expect(said.has("standalone-claude")).toBe(false);
    expect(withoutJudgedPathNotices(pathNotices, said.get("standalone-claude"))).toEqual(pathNotices);
  });
});

describe("twinVerdict and unusedCopies, for a formula Homebrew didn't link (q1b skeptic 5)", () => {
  // npm's `gemini` under Homebrew's node, then `brew install gemini-cli`:
  // the formula's link step stopped at npm's file. Its `gemini`, named
  // from its keg, has no verdict of its own; npm's runs.
  const formulaKey: ArtifactKey = { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "gemini-cli" };
  const npmGeminiKey: ArtifactKey = { instance_id: "npm:/opt/homebrew", kind: "Package", name: "@google/gemini-cli" };
  const npmGemini = artifact(npmGeminiKey, "gemini-cli", [{ name: "gemini", state: "Runs" }]);
  const unlinked: InstalledArtifact = {
    ...artifact(formulaKey, "gemini-cli", [{ name: "gemini", state: null }]),
    facts: { ...NO_FACTS, family: "gemini-cli", commands: [{ name: "gemini", state: null }], unlinked: true },
  };

  it("is the other copy's where its command runs: Terminal does not use this one", () => {
    const all = [npmGemini, unlinked];
    const twins = twinsByArtifact(all);
    expect(twinVerdict(unlinked, twins.get(artifactKeyId(formulaKey)))).toEqual({
      kind: "unused",
      command: "gemini",
      by: npmGemini,
    });
    expect([...unusedCopies(all)]).toEqual([artifactKeyId(formulaKey)]);
    // npm's side is unchanged: typing `gemini` runs it.
    expect(twinVerdict(npmGemini, twins.get(artifactKeyId(npmGeminiKey)))?.kind).toBe("runs");
  });

  it("is nothing where no copy's command runs, or the formula is linked", () => {
    // The login shell's PATH was not read: no verdict anywhere.
    const unjudgedNpm = artifact(npmGeminiKey, "gemini-cli", [{ name: "gemini", state: null }]);
    expect([...unusedCopies([unjudgedNpm, unlinked])]).toEqual([]);
    // Another program comes first for npm's too.
    const behind = artifact(npmGeminiKey, "gemini-cli", [{ name: "gemini", state: { ShadowedBy: { by: null } } }]);
    expect([...unusedCopies([behind, unlinked])]).toEqual([]);
    // A linked formula's command with no verdict says nothing, as before.
    const linked = { ...unlinked, facts: { ...unlinked.facts, unlinked: false } };
    expect([...unusedCopies([npmGemini, linked])]).toEqual([]);
  });
});
