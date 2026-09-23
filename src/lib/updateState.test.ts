import { describe, expect, it } from "vitest";
import { isUpdateActionable, notIgnored, updateStateOf } from "./updateState";
import type { ManagerInstance, UpdateCandidate } from "./types";

const brew: ManagerInstance = {
  id: "brew:/opt/homebrew",
  adapter_id: "brew",
  exe_path: "/opt/homebrew/bin/brew",
  prefix: "/opt/homebrew",
  scope: "User",
  version: "7.0.6",
  unverified_version: null,
  read_only_reason: null,
  status: { unavailable: null, notes: [] },
};

function candidate(over: Partial<UpdateCandidate> = {}): UpdateCandidate {
  return {
    key: { instance_id: brew.id, kind: "Formula", name: "glib" },
    current: "2.88.3",
    target: "2.90.0",
    channel: "Native",
    checkable: true,
    warnings: [],
    blocked: null,
    ...over,
  };
}

describe("updateStateOf", () => {
  it("offers an update only when the source and the package both allow it", () => {
    expect(updateStateOf(candidate(), brew)).toEqual({ kind: "actionable" });
    expect(isUpdateActionable(candidate(), brew)).toBe(true);
  });

  it("names each reason an update is listed and not offered", () => {
    expect(updateStateOf(candidate({ blocked: "Pinned" }), brew)).toEqual({
      kind: "blocked",
      reason: "Pinned",
    });
    expect(updateStateOf(candidate({ checkable: false }), brew)).toEqual({ kind: "cannotCheck" });
    expect(updateStateOf(candidate(), { ...brew, read_only_reason: "ByDesign" })).toEqual({
      kind: "readOnly",
    });
    expect(
      updateStateOf(candidate(), { ...brew, status: { unavailable: "NotRunning", notes: [] } }),
    ).toEqual({ kind: "sourceUnavailable" });
    // `Session::issue_plan` answers SourceGone for an instance the snapshot
    // does not have, so no button for it either.
    expect(updateStateOf(candidate(), undefined)).toEqual({ kind: "sourceUnavailable" });
  });

  it("puts the source's capability first, then the check, then the package's own refusal", () => {
    // The order the Updates page's badge has always used: "Read-only" holds
    // whatever the next refresh finds, and without a check there is no
    // update to block.
    const everything = candidate({ checkable: false, blocked: "Pinned" });
    const readOnlyAndSilent: ManagerInstance = {
      ...brew,
      read_only_reason: "PrefixNotWritable",
      status: { unavailable: "NotResponding", notes: [] },
    };
    expect(updateStateOf(everything, readOnlyAndSilent).kind).toBe("readOnly");
    expect(updateStateOf(everything, { ...brew, status: readOnlyAndSilent.status }).kind).toBe(
      "cannotCheck",
    );
    expect(
      updateStateOf(candidate({ blocked: "Pinned" }), {
        ...brew,
        status: readOnlyAndSilent.status,
      }).kind,
    ).toBe("blocked");
  });
});

describe("notIgnored", () => {
  it("drops exactly the ignored keys, matching instance, kind and name", () => {
    const formula = candidate();
    const cask = candidate({ key: { instance_id: brew.id, kind: "Cask", name: "glib" } });
    const otherPrefix = candidate({
      key: { instance_id: "brew:/usr/local", kind: "Formula", name: "glib" },
    });
    expect(notIgnored([formula, cask, otherPrefix], [formula.key])).toEqual([cask, otherPrefix]);
    expect(notIgnored([formula], [])).toEqual([formula]);
  });
});
