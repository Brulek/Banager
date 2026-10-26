import { describe, expect, it } from "vitest";
import i18n from "../i18n";
import { catalogRefusal, endedAfterCheck, installOperationOf, installProgressOf, ITEM_KEYS, stateText } from "./catalog";
import type { CatalogEntry, ManagerInstance, OpSummary } from "./types";

const t = i18n.getFixedT("en");

const node: CatalogEntry = {
  id: "node",
  category: "CommonTools",
  install: { Homebrew: { kind: "Formula", name: "node" } },
  state: "NotInstalled",
  installs_here: true,
};

function op(id: number, over: Partial<OpSummary> = {}): OpSummary {
  return {
    id,
    kind: "Install",
    instance_id: "brew:/opt/homebrew",
    artifact_kind: "Formula",
    name: "node",
    status: "Done",
    outcome: "Succeeded",
    argv_preview: [],
    cancel_policy: "KillThenReconcile",
    ...over,
  };
}

describe("an install from Discover", () => {
  it("is the newest install of the entry's own formula or cask, under any Homebrew", () => {
    const operations = [
      op(2),
      op(5, { instance_id: "brew:/usr/local", status: "Running", outcome: null }),
      // Not this entry's: an upgrade of it, another source's node, a cask
      // of the name, another formula.
      op(9, { kind: "Upgrade" }),
      op(10, { instance_id: "npm:/opt/homebrew" }),
      op(11, { artifact_kind: "Cask" }),
      op(12, { name: "node@22" }),
    ];
    expect(installOperationOf(node, operations, {})?.id).toBe(5);
    expect(installOperationOf(node, [], {})).toBeNull();
    // Homebrew itself is never installed from here.
    expect(installOperationOf({ ...node, id: "homebrew", install: "HomebrewItself" }, operations, {})).toBeNull();
  });

  it("is over once an uninstall of the same formula or cask comes after it", () => {
    expect(installOperationOf(node, [op(2), op(3, { kind: "Uninstall" })], {})).toBeNull();
    // An uninstall before it, or of something else, changes nothing.
    expect(installOperationOf(node, [op(1, { kind: "Uninstall" }), op(2)], {})?.id).toBe(2);
    expect(installOperationOf(node, [op(2), op(3, { kind: "Uninstall", name: "node@22" })], {})?.id).toBe(2);
  });

  it("is newer than the catalog only when this window heard it end after the snapshot was made", () => {
    const done = op(4);
    const made = (refreshed_at: number | null) => ({ instances: [], refreshed_at });
    expect(endedAfterCheck(done, { 4: 1_000_005_000 }, made(1_000_000))).toBe(true);
    expect(endedAfterCheck(done, { 4: 999_999_000 }, made(1_000_000))).toBe(false);
    expect(endedAfterCheck(done, {}, made(1_000_000))).toBe(false);
    expect(endedAfterCheck(done, { 4: 5 }, made(null))).toBe(true);
    expect(endedAfterCheck(op(4, { status: "Running", outcome: null }), { 4: 1_000_005_000 }, made(1_000_000))).toBe(
      false,
    );
    // A catalog older than the snapshot (the last one's, while the new
    // one's is asked for): what the install said, while this window heard it.
    expect(endedAfterCheck(done, { 4: 5 }, null)).toBe(true);
    expect(endedAfterCheck(done, {}, null)).toBe(false);
  });

  it("goes by when the source that ran it answered, not when the snapshot was made (from the review of 44674745)", () => {
    const done = op(4);
    const brew = (answered_at: number | null) => ({
      instances: [{ id: "brew:/opt/homebrew", answered_at } as ManagerInstance],
      // Committed after the install ended, by a check that read Homebrew
      // before it.
      refreshed_at: 1_000_010,
    });
    expect(endedAfterCheck(done, { 4: 1_000_005_000 }, brew(1_000_000))).toBe(true);
    expect(endedAfterCheck(done, { 4: 1_000_005_000 }, brew(1_000_006))).toBe(false);
    // No answer of its own in full: when the snapshot was made.
    expect(endedAfterCheck(done, { 4: 1_000_005_000 }, brew(null))).toBe(false);
  });

  it("is this window's own submission until the list has it", () => {
    const reserved = op(-1, { status: "Queued", outcome: null });
    expect(installOperationOf(node, [op(2)], { "brew:/opt/homebrew/Formula/node": reserved })).toBe(reserved);
    const listed = op(7, { status: "Queued", outcome: null });
    expect(installOperationOf(node, [op(2), listed], { key: { ...reserved, id: 7 } })).toBe(listed);
  });

  it("says where it stands, and how it ended", () => {
    expect(installProgressOf(op(1, { status: "Queued", outcome: null }))).toEqual({ kind: "queued" });
    expect(installProgressOf(op(1, { status: "Verifying", outcome: null }))).toEqual({ kind: "running" });
    expect(installProgressOf(op(1, { status: "CancelRequested", outcome: null }))).toEqual({ kind: "cancelling" });
    expect(installProgressOf(op(1))).toEqual({ kind: "installed" });
    expect(installProgressOf(op(1, { outcome: "Cancelled" }))).toEqual({ kind: "cancelled" });
    expect(installProgressOf(op(1, { outcome: "Unconfirmed" }))).toEqual({ kind: "unconfirmed", opId: 1 });
    expect(installProgressOf(op(1, { outcome: { NeedsAttention: "NotInstalledAfterInstall" } }))).toEqual({
      kind: "check",
      opId: 1,
    });
    expect(
      installProgressOf(op(1, { outcome: { Failed: { exit_code: 1, summary: "", cause: null } } })),
    ).toEqual({ kind: "failed", opId: 1 });
  });
});

describe("a refusal of plan_install", () => {
  it("is said in the row's terms where the Mac cannot install the entry now", () => {
    const said = (payload: unknown) => catalogRefusal(t, JSON.stringify(payload));
    expect(said({ kind: "catalog_state", state: "Installed" })).toBe("It's already installed.");
    expect(said({ kind: "catalog_state", state: "NeedsHomebrew" })).toBe(
      "No Homebrew on this Mac can install it right now.",
    );
    expect(said({ kind: "catalog_state", state: { NeedsMacOS: { major: 14 } } })).toBe("It needs macOS 14 or later.");
    // The model, with every Ollama at another machine's address (from
    // the review of abba6f7e).
    expect(said({ kind: "catalog_state", state: "OllamaElsewhere" })).toBe(
      "The Ollama this Mac uses is set by OLLAMA_HOST to an address other than this Mac's own, so the model isn't downloaded from here.",
    );
    expect(stateText(t, "OllamaElsewhere")).toBe("Ollama uses another address");
    // Anything else is `planErrorMessage`'s.
    expect(said({ kind: "refused" })).toBeNull();
    expect(said({ kind: "catalog_state", state: { NeedsMacOS: {} } })).toBeNull();
    expect(catalogRefusal(t, "not json")).toBeNull();
  });
});

describe("the catalog's words", () => {
  it("give every entry installed from here how it is used and what to type", () => {
    for (const id of ["claude-code", "codex", "gemini-cli", "node", "uv", "ffmpeg"]) {
      const keys = ITEM_KEYS[id];
      expect(keys.use, id).toBe(`discover.items.${id}.use`);
      expect(t(keys.use!), id).toContain(`Type ${keys.command}`);
    }
  });
});
