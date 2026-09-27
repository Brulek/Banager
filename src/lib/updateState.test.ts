import { describe, expect, it } from "vitest";
import {
  actionableUpdatesOf,
  canSkipVersion,
  everySourceChecked,
  hidingRule,
  isUpdateActionable,
  notHidden,
  shownSkippedVersion,
  updatesSummary,
  updateStateOf,
  withSkippedVersion,
} from "./updateState";
import type { HidingSettings } from "./updateState";
import type { ArtifactKey, ManagerInstance, UpdateCandidate } from "./types";

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

function hiding(over: Partial<HidingSettings> = {}): HidingSettings {
  return { ignored_updates: [], skipped_versions: [], ...over };
}

const qwenKey: ArtifactKey = {
  instance_id: "ollama:127.0.0.1:11434",
  kind: "Model",
  name: "qwen3:8b",
};

// A Homebrew cask declared `version :latest`. `brew outdated --json=v2
// --greedy` lists one whenever it takes its download to have changed, and
// its `current_version` is the cask's version, "latest", for every release
// (`Cask#outdated_info`).
const chromiumKey: ArtifactKey = { instance_id: brew.id, kind: "Cask", name: "chromium" };

describe("canSkipVersion", () => {
  it("offers a skip where the version a row offers names one release", () => {
    expect(canSkipVersion(candidate())).toBe(true);
    expect(
      canSkipVersion(
        candidate({
          key: { instance_id: brew.id, kind: "Cask", name: "onyx" },
          current: "5.0.2",
          target: "5.1.0",
        }),
      ),
    ).toBe(true);
    // An Ollama model's newer build is offered by its registry manifest's
    // config digest, one per build.
    expect(
      canSkipVersion(
        candidate({
          key: qwenKey,
          current: "5642e97495e1",
          target: "sha256:9f1c0b6d2e4a",
          channel: "Digest",
        }),
      ),
    ).toBe(true);
  });

  it("offers none on a row Canager could not check", () => {
    // Its `target` is its installed version, not one any source offered.
    expect(
      canSkipVersion(candidate({ current: "2.88.3", target: "2.88.3", checkable: false })),
    ).toBe(false);
  });

  it("offers none on a Homebrew cask declared version :latest, every release of which is offered as latest", () => {
    expect(
      canSkipVersion(candidate({ key: chromiumKey, current: "latest", target: "latest" })),
    ).toBe(false);
    // Had it been installed while its cask still had numbered versions,
    // Homebrew would name the installed copy by that version and offer
    // "latest" all the same.
    expect(
      canSkipVersion(candidate({ key: chromiumKey, current: "120.0.6099.0", target: "latest" })),
    ).toBe(false);
  });

  it("goes by the version offered, not by whether it reads the same as the one installed", () => {
    // Homebrew also lists an unpinned formula whose installed keg is its
    // current version but is neither linked nor opt-linked
    // (`Formula#outdated_kegs`), as 2.90.0 -> 2.90.0. Its next release has
    // another number, so a skip of 2.90.0 ends there, as the button
    // promises.
    expect(canSkipVersion(candidate({ current: "2.90.0", target: "2.90.0" }))).toBe(true);
  });
});

describe("hidingRule", () => {
  const formula = candidate();
  const cask = candidate({ key: { instance_id: brew.id, kind: "Cask", name: "glib" } });
  const otherPrefix = candidate({
    key: { instance_id: "brew:/usr/local", kind: "Formula", name: "glib" },
  });

  it("hides every version of a package the user is never reminded about, matching instance, kind and name", () => {
    const hiddenBy = hidingRule(hiding({ ignored_updates: [formula.key] }));
    expect(hiddenBy(formula)).toBe("ignored");
    expect(hiddenBy(candidate({ target: "3.0.0" }))).toBe("ignored");
    expect(hiddenBy(cask)).toBeNull();
    expect(hiddenBy(otherPrefix)).toBeNull();
  });

  it("hides a skipped version only while the source still offers that version", () => {
    const hiddenBy = hidingRule(
      hiding({ skipped_versions: [{ key: formula.key, version: "2.90.0" }] }),
    );
    expect(hiddenBy(formula)).toBe("skipped");
    // The source has moved on: the skip no longer matches and the row is
    // listed again, with the version it offers now.
    expect(hiddenBy(candidate({ target: "2.92.0" }))).toBeNull();
    // The same version of another package is that package's own update.
    expect(hiddenBy({ ...cask, target: "2.90.0" })).toBeNull();
    expect(hiddenBy({ ...otherPrefix, target: "2.90.0" })).toBeNull();
  });

  it("never lets a skip hide a row Canager could not check", () => {
    // An uncheckable candidate's `target` is its installed version
    // (`uncheckable_candidate` in crates/canager-core/src/adapters/mod.rs),
    // not one the source offered. glib 2.90.0 was skipped, glib was later
    // brought to 2.90.0 some other way, and now its lookup fails: that row
    // says Canager could not check it, and a skip of an offered 2.90.0 must
    // not hide it.
    const hiddenBy = hidingRule(
      hiding({ skipped_versions: [{ key: formula.key, version: "2.90.0" }] }),
    );
    expect(
      hiddenBy(candidate({ current: "2.90.0", target: "2.90.0", checkable: false })),
    ).toBeNull();
  });

  it("never lets a skip hide a Homebrew cask declared version :latest", () => {
    // Every release of it is offered as "latest", so a skip of "latest"
    // would hide each later release as well and never end.
    const hiddenBy = hidingRule(
      hiding({ skipped_versions: [{ key: chromiumKey, version: "latest" }] }),
    );
    expect(hiddenBy(candidate({ key: chromiumKey, current: "latest", target: "latest" }))).toBeNull();
    expect(
      hiddenBy(candidate({ key: chromiumKey, current: "120.0.6099.0", target: "latest" })),
    ).toBeNull();
  });

  it("calls a package that is both never reminded about and skipped ignored", () => {
    // "Never remind me" holds for every version, the skip for one.
    const hiddenBy = hidingRule(
      hiding({
        ignored_updates: [formula.key],
        skipped_versions: [{ key: formula.key, version: "2.90.0" }],
      }),
    );
    expect(hiddenBy(formula)).toBe("ignored");
  });

  it("matches a skipped Ollama build by the digest the row offered", () => {
    // `target` is the registry manifest's config digest; the skip stored the
    // same kind of digest, so this compares like with like (never `current`,
    // a digest from another hash space).
    const offered = candidate({
      key: qwenKey,
      current: "5642e97495e1",
      target: "sha256:9f1c0b6d2e4a",
      channel: "Digest",
    });
    const hiddenBy = hidingRule(
      hiding({ skipped_versions: [{ key: qwenKey, version: "sha256:9f1c0b6d2e4a" }] }),
    );
    expect(hiddenBy(offered)).toBe("skipped");
    expect(hiddenBy({ ...offered, target: "sha256:0a1b2c3d4e5f" })).toBeNull();
  });
});

describe("notHidden", () => {
  it("lists exactly the candidates the rule does not hide", () => {
    const formula = candidate();
    const cask = candidate({ key: { instance_id: brew.id, kind: "Cask", name: "glib" } });
    const jq = candidate({ key: { instance_id: brew.id, kind: "Formula", name: "jq" } });
    const settings = hiding({
      ignored_updates: [formula.key],
      skipped_versions: [{ key: cask.key, version: cask.target }],
    });
    expect(notHidden([formula, cask, jq], settings)).toEqual([jq]);
    expect(notHidden([formula, cask, jq], hiding())).toEqual([formula, cask, jq]);
  });
});

describe("actionableUpdatesOf", () => {
  it("keeps the listed updates that have an Update button, in the snapshot's order", () => {
    const pip: ManagerInstance = {
      ...brew,
      id: "pip:/usr/bin/python3",
      adapter_id: "pip",
      read_only_reason: "ByDesign",
    };
    const stopped: ManagerInstance = {
      ...brew,
      id: "ollama:http://127.0.0.1:11434",
      adapter_id: "ollama",
      status: { unavailable: "NotRunning", notes: [] },
    };
    const named = (name: string, over: Partial<UpdateCandidate> = {}) =>
      candidate({ key: { instance_id: brew.id, kind: "Formula", name }, ...over });
    const glib = named("glib");
    const wget = named("wget");
    const pinned = named("jq", { blocked: "Pinned" });
    const unchecked = named("pcre2", { checkable: false });
    const ignored = named("ffmpeg");
    const skipped = named("gh");
    const readOnly = candidate({ key: { instance_id: pip.id, kind: "Package", name: "urllib3" } });
    const silent = candidate({ key: { instance_id: stopped.id, kind: "Model", name: "qwen3:8b" } });
    const orphan = candidate({ key: { instance_id: "cargo:/gone", kind: "Binary", name: "rg" } });

    const offered = actionableUpdatesOf(
      {
        instances: [brew, pip, stopped],
        updates: [glib, pinned, unchecked, ignored, skipped, readOnly, silent, orphan, wget],
      },
      hiding({
        ignored_updates: [ignored.key],
        skipped_versions: [{ key: skipped.key, version: skipped.target }],
      }),
    );

    expect(offered).toEqual([glib, wget]);
  });
});

describe("everySourceChecked", () => {
  it("holds when every source answered and none says its updates went unchecked", () => {
    expect(everySourceChecked([])).toBe(true);
    expect(everySourceChecked([brew, { ...brew, read_only_reason: "ByDesign" }])).toBe(true);
    // Which copy runs when its name is typed says nothing about the check.
    expect(
      everySourceChecked([{ ...brew, status: { unavailable: null, notes: ["NotOnPath"] } }]),
    ).toBe(true);
  });

  it("fails for a source that did not answer or was not checked in full", () => {
    expect(
      everySourceChecked([brew, { ...brew, status: { unavailable: "NotRunning", notes: [] } }]),
    ).toBe(false);
    for (const note of ["IndexMayBeStale", "IndexUpdating", "LauncherOnly"] as const) {
      expect(everySourceChecked([{ ...brew, status: { unavailable: null, notes: [note] } }])).toBe(
        false,
      );
    }
  });
});

describe("updatesSummary", () => {
  it("counts what can be installed, before anything else", () => {
    const glib = candidate();
    const pinned = candidate({
      key: { instance_id: brew.id, kind: "Formula", name: "jq" },
      blocked: "Pinned",
    });
    expect(updatesSummary({ instances: [brew], updates: [pinned, glib] }, hiding())).toEqual({
      kind: "updates",
      actionable: [glib],
    });
  });

  it("is up to date only with no update at all and every source checked", () => {
    expect(updatesSummary({ instances: [brew], updates: [] }, hiding())).toEqual({
      kind: "upToDate",
    });
    const stopped: ManagerInstance = {
      ...brew,
      status: { unavailable: "NotRunning", notes: [] },
    };
    expect(updatesSummary({ instances: [stopped], updates: [] }, hiding())).toEqual({
      kind: "nothingToUpdate",
    });
    const pinned = candidate({ blocked: "Pinned" });
    expect(updatesSummary({ instances: [brew], updates: [pinned] }, hiding())).toEqual({
      kind: "nothingToUpdate",
    });
    const glib = candidate();
    expect(
      updatesSummary({ instances: [brew], updates: [glib] }, hiding({ ignored_updates: [glib.key] })),
    ).toEqual({ kind: "nothingToUpdate" });
  });
});

describe("withSkippedVersion", () => {
  it("records the version the row offers", () => {
    expect(withSkippedVersion([], candidate())).toEqual([
      { key: candidate().key, version: "2.90.0" },
    ]);
  });

  it("replaces the package's earlier skip, whose version the source no longer offers, and keeps every other", () => {
    // The row is listed, so an earlier skip of this package no longer
    // matches what it offers: the source has moved past that version.
    const onyx: ArtifactKey = { instance_id: brew.id, kind: "Cask", name: "onyx" };
    const earlier = [
      { key: candidate().key, version: "2.88.0" },
      { key: onyx, version: "5.1.0" },
    ];
    expect(withSkippedVersion(earlier, candidate())).toEqual([
      { key: onyx, version: "5.1.0" },
      { key: candidate().key, version: "2.90.0" },
    ]);
  });
});

describe("shownSkippedVersion", () => {
  it("shows a skipped version, and never an Ollama model's digest", () => {
    expect(shownSkippedVersion({ key: candidate().key, version: "2.90.0" })).toBe("2.90.0");
    expect(
      shownSkippedVersion({ key: qwenKey, version: "sha256:9f1c0b6d2e4a" }),
    ).toBeNull();
  });
});
