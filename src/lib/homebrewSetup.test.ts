import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { HOMEBREW_INSTALL_COMMAND, isEmptyMac, olderThanHomebrewSupports } from "./homebrewSetup";
import { NO_FACTS, type InstalledArtifact, type ManagerInstance, type Snapshot } from "./types";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

function instance(adapter_id: string, id: string): ManagerInstance {
  return {
    id,
    adapter_id,
    exe_path: "/usr/bin/true",
    prefix: "/",
    scope: "User",
    version: null,
    answered_at: null,
    unverified_version: null,
    read_only_reason: null,
    status: { unavailable: null, notes: [] },
  };
}

function artifact(instance_id: string, name: string): InstalledArtifact {
  return {
    key: { instance_id, kind: "Package", name },
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
  };
}

function mac(instances: ManagerInstance[], artifacts: InstalledArtifact[], refreshed_at: number | null = 1): Snapshot {
  return {
    generation: 1,
    round: 1,
    detect: instances.length === 0 ? "Missing" : "Found",
    instances,
    artifacts,
    updates: [],
    refreshed_at,
    stale: false,
    errors: [],
  };
}

describe("the empty Mac", () => {
  it("has no Homebrew, and nothing listed but by pip", () => {
    const pip = instance("pip", "pip:/usr/bin/python3");
    expect(isEmptyMac(mac([], []))).toBe(true);
    expect(isEmptyMac(mac([pip], [artifact(pip.id, "pip"), artifact(pip.id, "six")]))).toBe(true);
    // Homebrew, even with nothing installed, is somewhere to start.
    expect(isEmptyMac(mac([instance("brew", "brew:/opt/homebrew")], []))).toBe(false);
    // Anything another source lists: Claude Code from its own installer.
    const claude = instance("standalone-claude", "standalone-claude");
    expect(isEmptyMac(mac([pip, claude], [artifact(claude.id, "claude")]))).toBe(false);
    // A source with nothing listed is not something to manage either.
    expect(isEmptyMac(mac([instance("npm", "npm:/usr/local")], []))).toBe(true);
    // Before a check has finished, nothing is known.
    expect(isEmptyMac(mac([], [], null))).toBe(false);
  });
});

describe("Homebrew's supported macOS", () => {
  it("is 15 and later", () => {
    expect(olderThanHomebrewSupports("14.7.1")).toBe(14);
    expect(olderThanHomebrewSupports("13.3")).toBe(13);
    expect(olderThanHomebrewSupports("15.0")).toBeNull();
    expect(olderThanHomebrewSupports("27.0")).toBeNull();
    expect(olderThanHomebrewSupports(null)).toBeNull();
    expect(olderThanHomebrewSupports("")).toBeNull();
  });
});

describe("Homebrew's install command", () => {
  it("is the one docs/what-we-run.md quotes, word for word", () => {
    const doc = readFileSync(path.resolve(__dirname, "../../docs/what-we-run.md"), "utf8");
    expect(doc).toContain(`\n    ${HOMEBREW_INSTALL_COMMAND}\n`);
  });
});
