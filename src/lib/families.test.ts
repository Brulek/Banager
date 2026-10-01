import { describe, expect, it } from "vitest";
import {
  DISCOVER_SHOWS,
  discoverCounts,
  discoverCovered,
  discoverNotices,
  hasCommandNotOnPath,
  isAiTool,
  isBrewRetired,
  isDiscoverShow,
  shownBy,
} from "./families";
import { NO_FACTS, type ArtifactFacts, type InstalledArtifact, type ManagerInstance } from "./types";

describe("families", () => {
  it("reads facts.family off the wire as Rust writes it, null and a family alike", () => {
    // `ArtifactFacts` in crates/banager-core/src/model.rs serialises to
    // exactly these (its test
    // test_facts_is_an_object_with_explicit_nulls_on_the_wire_and_optional_when_read).
    const none: ArtifactFacts = JSON.parse('{"family":null,"homebrew":null,"commands":[]}');
    const codex: ArtifactFacts = JSON.parse('{"family":"codex","homebrew":null,"commands":[]}');
    expect(none).toEqual(NO_FACTS);
    expect(codex).toEqual({ ...NO_FACTS, family: "codex" });
    expect(JSON.stringify(codex)).toBe('{"family":"codex","homebrew":null,"commands":[]}');
    expect(JSON.parse(JSON.stringify(NO_FACTS))).toEqual(NO_FACTS);
  });

  it("calls an artifact an AI tool only when Rust gave it a family", () => {
    const tagged: Pick<InstalledArtifact, "facts"> = { facts: { ...NO_FACTS, family: "claude-code" } };
    const plain: Pick<InstalledArtifact, "facts"> = { facts: NO_FACTS };
    expect(isAiTool(tagged)).toBe(true);
    expect(isAiTool(plain)).toBe(false);
    // A row whose artifact the snapshot no longer has is not one.
    expect(isAiTool(undefined)).toBe(false);
  });

  it("shows everything under All Tools, and only the AI tools under AI Tools", () => {
    const key = { instance_id: "brew:/opt/homebrew", kind: "Formula" as const, name: "ollama" };
    const tagged = { key, facts: { ...NO_FACTS, family: "ollama" } };
    const plain = { key: { ...key, name: "jq" }, facts: NO_FACTS };
    expect([shownBy("all", tagged), shownBy("all", plain), shownBy("all", undefined)]).toEqual([true, true, true]);
    expect([shownBy("ai", tagged), shownBy("ai", plain), shownBy("ai", undefined)]).toEqual([true, false, false]);
  });
});

describe("the discovery choices", () => {
  it("tells the discovery choices from the others", () => {
    expect(DISCOVER_SHOWS.every(isDiscoverShow)).toBe(true);
    expect(["all", "ai", "twins"].some((show) => isDiscoverShow(show as "all"))).toBe(false);
  });

  const key = { instance_id: "brew:/opt/homebrew", kind: "Formula" as const, name: "wget" };
  const lifecycle = { date: null, reason: null, replacement: null };
  const homebrew = { deprecated: null, disabled: null, caveats: null, other_versions: [] };
  const offPath = {
    key: { ...key, name: "grok" },
    facts: {
      ...NO_FACTS,
      commands: [
        { name: "agent", state: "Runs" as const },
        { name: "grok", state: { NotOnPath: { dir: "~/.grok/bin" } } },
      ],
    },
  };
  const shadowed = {
    key: { ...key, name: "claude" },
    facts: { ...NO_FACTS, commands: [{ name: "claude", state: { ShadowedBy: { by: null } } }] },
  };
  const unjudged = {
    key: { ...key, name: "node@22" },
    facts: { ...NO_FACTS, commands: [{ name: "node", state: null }] },
  };
  const deprecated = {
    key: { ...key, name: "youtube-dl" },
    facts: { ...NO_FACTS, homebrew: { ...homebrew, deprecated: lifecycle } },
  };
  const disabled = {
    key: { ...key, name: "quickjot" },
    facts: { ...NO_FACTS, homebrew: { ...homebrew, disabled: lifecycle } },
  };
  const caveatsOnly = {
    key: { ...key, name: "git" },
    facts: { ...NO_FACTS, homebrew: { ...homebrew, caveats: "note" } },
  };
  const plain = { key, facts: NO_FACTS };
  const all = [offPath, shadowed, unjudged, deprecated, disabled, caveatsOnly, plain];

  it("finds a tool with one command Terminal does not find, and nothing else", () => {
    expect(all.filter((a) => hasCommandNotOnPath(a)).map((a) => a.key.name)).toEqual(["grok"]);
    expect(all.filter((a) => shownBy("notOnPath", a)).map((a) => a.key.name)).toEqual(["grok"]);
    expect(hasCommandNotOnPath(undefined)).toBe(false);
  });

  it("finds what Homebrew disabled or deprecated, not what it merely has notes on", () => {
    expect(all.filter((a) => isBrewRetired(a)).map((a) => a.key.name)).toEqual(["youtube-dl", "quickjot"]);
    expect(all.filter((a) => shownBy("brewRetired", a)).map((a) => a.key.name)).toEqual(["youtube-dl", "quickjot"]);
    expect(isBrewRetired(undefined)).toBe(false);
  });

  it("counts what each choice shows", () => {
    expect(discoverCounts(all)).toEqual({ notOnPath: 1, brewRetired: 2 });
    expect(discoverCounts([])).toEqual({ notOnPath: 0, brewRetired: 0 });
    expect(DISCOVER_SHOWS).toEqual(["notOnPath", "brewRetired"]);
  });
});

describe("discoverNotices", () => {
  it("points at each choice with tools to show, in the popup's order, only over every tool", () => {
    expect(discoverNotices("all", { notOnPath: 2, brewRetired: 1 })).toEqual([
      {
        id: "discover:notOnPath",
        variant: "info",
        titleKey: "families.notOnPathNotice",
        descriptionKey: "families.notOnPathNoticeDetail",
        values: { count: 2 },
        action: { id: "showList", labelKey: "families.view", show: "notOnPath" },
      },
      {
        id: "discover:brewRetired",
        variant: "info",
        titleKey: "families.brewRetiredNotice",
        descriptionKey: "families.brewRetiredNoticeDetail",
        values: { count: 1 },
        action: { id: "showList", labelKey: "families.view", show: "brewRetired" },
      },
    ]);
    expect(discoverNotices("all", { notOnPath: 0, brewRetired: 3 }).map((n) => n.id)).toEqual([
      "discover:brewRetired",
    ]);
    expect(discoverNotices("all", { notOnPath: 0, brewRetired: 0 })).toEqual([]);
    for (const show of ["ai", "twins", "notOnPath", "brewRetired"] as const) {
      expect(discoverNotices(show, { notOnPath: 2, brewRetired: 1 })).toEqual([]);
    }
  });

  it("leaves out a line whose every tool a source's notice names, and keeps the whole count otherwise", () => {
    const counts = { notOnPath: 2, brewRetired: 1 };
    expect(discoverNotices("all", counts, { notOnPath: 2, brewRetired: 0 }).map((n) => n.id)).toEqual([
      "discover:brewRetired",
    ]);
    expect(discoverNotices("all", counts, { notOnPath: 1, brewRetired: 0 })[0]?.values).toEqual({ count: 2 });
  });
});

describe("discoverCovered", () => {
  const offPath = (instanceId: string, name: string) => ({
    key: { instance_id: instanceId, kind: "Binary" as const, name },
    facts: { ...NO_FACTS, commands: [{ name, state: { NotOnPath: { dir: "~/bin" } } }] },
  });
  const source = (id: string, notes: ManagerInstance["status"]["notes"]) => ({
    id,
    status: { unavailable: null, notes },
  });

  it("counts the tools Terminal can't find whose source says so itself", () => {
    const artifacts = [offPath("standalone-grok", "grok"), offPath("pipx:/opt/homebrew/bin", "http")];
    expect(
      discoverCovered(artifacts, [source("standalone-grok", ["NotOnPath"]), source("pipx:/opt/homebrew/bin", [])]),
    ).toEqual({ notOnPath: 1, brewRetired: 0 });
    expect(discoverCovered(artifacts, [source("standalone-grok", [])])).toEqual({ notOnPath: 0, brewRetired: 0 });
  });
});
