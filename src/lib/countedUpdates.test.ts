import { describe, expect, it } from "vitest";
import { twinsByArtifact, unusedCopies } from "./commands";
import { actionableUpdatesOf, countedUpdatesOf, updatesSummary, type HidingSettings } from "./updateState";
import type { CommandState, InstalledArtifact, ManagerInstance, UpdateCandidate } from "./types";
import { NO_FACTS } from "./types";

/**
 * U4 (decisions round, 2026-10-06): an update of a copy Terminal does not
 * run -- the 「终端用另一份」 row of a tool installed twice -- is listed with
 * its word and its checkbox, but Update all leaves it unticked and no
 * number counts it: not the sidebar's, the Dock's, the notification's,
 * the toolbar's or the Overview's. A major-version update stays in, as
 * before.
 */

function instance(id: string, adapterId: string): ManagerInstance {
  return {
    id,
    adapter_id: adapterId,
    exe_path: `/opt/homebrew/bin/${adapterId}`,
    prefix: "/opt/homebrew",
    scope: "User",
    version: "1.0.0",
    answered_at: null,
    unverified_version: null,
    read_only_reason: null,
    status: { unavailable: null, notes: [] },
  };
}

const brew = instance("brew:/opt/homebrew", "brew");
const npm = instance("npm:/opt/homebrew", "npm");
const codexOwn = instance("standalone-codex", "standalone-codex");

function copy(instanceId: string, name: string, family: string | null, state: CommandState | null): InstalledArtifact {
  return {
    key: { instance_id: instanceId, kind: instanceId.startsWith("npm") ? "Package" : "Binary", name },
    display_name: name,
    version: "1.0.0",
    reason: "Requested",
    description: null,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
    facts: { ...NO_FACTS, family, commands: [{ name: family ?? name, state }] },
  };
}

// Codex twice: its own installer's copy runs when `codex` is typed, npm's does not.
const ownCodex = copy(codexOwn.id, "codex", "codex", "Runs");
const npmCodex = copy(npm.id, "@openai/codex", "codex", { ShadowedBy: { by: ownCodex.key } });
const glib = copy(brew.id, "glib", null, "Runs");

function update(artifact: InstalledArtifact, over: Partial<UpdateCandidate> = {}): UpdateCandidate {
  return {
    key: artifact.key,
    current: "1.0.0",
    target: "1.1.0",
    channel: "Native",
    checkable: true,
    warnings: [],
    blocked: null,
    ...over,
  };
}

const hiding = (over: Partial<HidingSettings> = {}): HidingSettings => ({
  ignored_updates: [],
  skipped_versions: [],
  snoozed_updates: [],
  ...over,
});

describe("unusedCopies", () => {
  it("is every copy Terminal does not run, by key id, and nothing else", () => {
    const artifacts = [ownCodex, npmCodex, glib];
    expect([...unusedCopies(artifacts)]).toEqual(["npm:/opt/homebrew|Package|@openai/codex"]);
    // Given the twins already worked out, the same.
    expect([...unusedCopies(artifacts, twinsByArtifact(artifacts))]).toEqual([
      "npm:/opt/homebrew|Package|@openai/codex",
    ]);
  });

  it("leaves out a copy whose commands have no one verdict, and a namesake that is no copy", () => {
    const unknown = copy(npm.id, "@openai/codex", "codex", null);
    expect(unusedCopies([ownCodex, unknown]).size).toBe(0);
    // Homebrew's `grok` is a regular-expression tool, not Grok Build.
    const brewGrok = copy(brew.id, "grok", null, "Runs");
    const grokBuild: InstalledArtifact = {
      ...copy("standalone-grok", "grok", "grok-build", { ShadowedBy: { by: brewGrok.key } }),
    };
    expect(unusedCopies([brewGrok, grokBuild]).size).toBe(0);
  });
});

describe("countedUpdatesOf", () => {
  const snapshot = {
    instances: [brew, npm, codexOwn],
    artifacts: [ownCodex, npmCodex, glib],
    updates: [update(glib), update(npmCodex), update(ownCodex)],
  };

  it("leaves out the update of a copy Terminal does not run, which keeps its checkbox", () => {
    expect(actionableUpdatesOf(snapshot, hiding()).map((u) => u.key.name)).toEqual([
      "glib",
      "@openai/codex",
      "codex",
    ]);
    expect(countedUpdatesOf(snapshot, hiding()).map((u) => u.key.name)).toEqual(["glib", "codex"]);
  });

  it("keeps a major-version update in", () => {
    const major = { ...snapshot, updates: [update(glib, { current: "2.88.3", target: "3.0.0" })] };
    expect(countedUpdatesOf(major, hiding()).map((u) => u.key.name)).toEqual(["glib"]);
  });

  it("goes by what actionableUpdatesOf offers: a hidden or pinned update is not counted either", () => {
    const settings = hiding({ ignored_updates: [glib.key] });
    expect(countedUpdatesOf(snapshot, settings).map((u) => u.key.name)).toEqual(["codex"]);
    const pinned = { ...snapshot, updates: [update(glib, { blocked: "Pinned" })] };
    expect(countedUpdatesOf(pinned, hiding())).toEqual([]);
  });
});

describe("updatesSummary with a copy Terminal does not run", () => {
  it("counts the other updates only, and offers those to Review Updates", () => {
    const snapshot = {
      instances: [brew, npm, codexOwn],
      artifacts: [ownCodex, npmCodex, glib],
      updates: [update(glib), update(npmCodex)],
      errors: [],
    };
    const summary = updatesSummary(snapshot, hiding());
    expect(summary.kind).toBe("updates");
    expect(summary.kind === "updates" ? summary.actionable.map((u) => u.key.name) : null).toEqual(["glib"]);
  });

  it("is not \"N tools can be updated\" when that copy's update is the only one", () => {
    const snapshot = {
      instances: [brew, npm, codexOwn],
      artifacts: [ownCodex, npmCodex, glib],
      updates: [update(npmCodex)],
      errors: [],
    };
    expect(updatesSummary(snapshot, hiding()).kind).not.toBe("updates");
  });
});
