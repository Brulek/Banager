import { describe, expect, it } from "vitest";
import { commandGroups, stateId, twinsByArtifact, withoutJudgedPathNotices } from "./commands";
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
    expect(withoutJudgedPathNotices([...pathNotices, ...others], native)).toEqual(others);
  });

  it("keeps it when nothing was judged about the launcher, or the verdict is for another command", () => {
    const unjudged = artifact(nativeKey, "claude-code", [{ name: "claude", state: null }]);
    expect(withoutJudgedPathNotices(pathNotices, unjudged)).toEqual(pathNotices);
    const elsewhere = artifact(nativeKey, "claude-code", [
      { name: "claude", state: null },
      { name: "claude-helper", state: "Runs" },
    ]);
    expect(withoutJudgedPathNotices(pathNotices, elsewhere)).toEqual(pathNotices);
  });
});
