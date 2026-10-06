import { describe, expect, it } from "vitest";
import i18n from "../i18n";
import { setupAttention, toolSetupCheck, type ToolSetupInput } from "./toolSetupCheck";
import type { ArtifactKey, CommandFact, InstalledArtifact, ManagerInstance, Snapshot, SystemFacts } from "./types";
import { NO_FACTS } from "./types";

/**
 * I4 (decisions round, 2026-10-06): the Overview's 「工具环境」 row says
 * 「N项需要查看」 when the sheet would open on N ⚠︎ lines, and only then.
 * Counted without building the sheet (`setupAttention`), by the same rules
 * that give its lines the ⚠︎: so the row and the sheet's own summary can
 * never say different numbers.
 */

const en = i18n.getFixedT("en");

function instance(id: string, more: Partial<ManagerInstance> = {}): ManagerInstance {
  return {
    id,
    adapter_id: id.split(":")[0],
    exe_path: "/opt/homebrew/bin/x",
    prefix: "/opt/homebrew",
    scope: "User",
    version: "1.0",
    answered_at: null,
    unverified_version: null,
    read_only_reason: null,
    status: { unavailable: null, notes: [] },
    ...more,
  };
}

function artifact(key: ArtifactKey, commands: CommandFact[]): InstalledArtifact {
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
    facts: { ...NO_FACTS, commands },
  };
}

const BREW = "brew:/opt/homebrew";
const NPM = "npm:/opt/homebrew";
const OLLAMA = "ollama:http://127.0.0.1:11434";

const jq = artifact({ instance_id: BREW, kind: "Formula", name: "jq" }, [{ name: "jq", state: "Runs" }]);
const prettier = artifact({ instance_id: NPM, kind: "Package", name: "prettier" }, [
  { name: "prettier", state: { NotOnPath: { dir: "~/.npm-global/bin" } } },
]);

function snapshot(more: Partial<Snapshot> = {}): Snapshot {
  return {
    generation: 3,
    round: 4,
    detect: "Found",
    instances: [instance(BREW), instance(NPM)],
    artifacts: [jq],
    updates: [],
    refreshed_at: 1_790_000_000,
    stale: false,
    errors: [],
    ...more,
  };
}

const FACTS: SystemFacts = {
  macos_version: "27.0",
  chip: "Apple M2 Pro",
  arch: "aarch64",
  login_path: true,
  path_dirs: ["/opt/homebrew/bin", "/usr/bin"],
  sources: [],
  path_folders: { read: 2, unread: [] },
};

function input(more: Partial<ToolSetupInput> = {}): ToolSetupInput {
  return {
    snapshot: snapshot(),
    pending: false,
    facts: FACTS,
    sizes: null,
    technicalDetails: false,
    language: "en",
    nowMs: Date.now(),
    ...more,
  };
}

const cases: Array<[string, ToolSetupInput, number]> = [
  ["nothing wrong", input(), 0],
  ["the facts not yet in", input({ facts: undefined }), 0],
  ["the facts that could not be had", input({ facts: null }), 1],
  ["the login shell's settings unread", input({ facts: { ...FACTS, login_path: false } }), 1],
  [
    "two sources that did not answer, and one whose check did not finish",
    input({
      snapshot: snapshot({
        instances: [
          instance(BREW, { status: { unavailable: "NotResponding", notes: [] } }),
          instance(NPM),
          instance(OLLAMA, { status: { unavailable: "NotRunning", notes: [] } }),
        ],
        errors: [
          { instance_id: NPM, message: "npm ls exited 1" },
          { instance_id: NPM, message: "npm outdated exited 1" },
        ],
      }),
    }),
    3,
  ],
  // Read-only and an untested version are notes, not warnings.
  [
    "a read-only source on a version not tested",
    input({ snapshot: snapshot({ instances: [instance(BREW), instance(NPM, { read_only_reason: "PrefixNotWritable", unverified_version: "12.0.0" })] }) }),
    0,
  ],
  ["a tool Terminal can't find", input({ snapshot: snapshot({ artifacts: [jq, prettier] }) }), 1],
  // While the first check runs, the commands are not judged yet.
  ["a tool Terminal can't find, during the first check", input({ snapshot: snapshot({ artifacts: [jq, prettier] }), pending: true }), 0],
  ["no snapshot yet", input({ snapshot: null, pending: true }), 0],
];

describe("setupAttention", () => {
  it.each(cases)("counts the ⚠︎ lines the sheet would have, with %s", (_name, given, count) => {
    expect(setupAttention(given)).toBe(count);
    expect(toolSetupCheck(en, given).attention).toBe(count);
  });
});
