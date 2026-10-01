import { describe, expect, it } from "vitest";
import i18n from "../i18n";
import { sizeTotalsOf, sourceTotalText, viewTotalText } from "./sizeTotals";
import { NO_FACTS, NO_SIZES, type InstalledArtifact, type Measured, type Sizes } from "./types";

const BREW = "brew:/opt/homebrew";
const PIP = "pip:/usr/bin/python3";
const OLLAMA = "ollama:http://127.0.0.1:11434";

function tool(instanceId: string, kind: InstalledArtifact["key"]["kind"], name: string): InstalledArtifact {
  return {
    key: { instance_id: instanceId, kind, name },
    display_name: name,
    version: "1.0.0",
    reason: "Requested",
    description: null,
    homepage: null,
    size_bytes: kind === "Model" ? 2_000_000_000 : null,
    installed_at: null,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
    facts: NO_FACTS,
  };
}

const jq = tool(BREW, "Formula", "jq");
const node = tool(BREW, "Formula", "node@22");
const requests = tool(PIP, "Package", "requests");
const llama = tool(OLLAMA, "Model", "llama3.2:3b");

const about = (bytes: number, more: Partial<Measured> = {}): Measured => ({
  bytes,
  partial: false,
  at_least: false,
  ...more,
});

/** A finished round for round 7, measuring jq and node@22 and Ollama's models. */
function done(overrides: Partial<Sizes> = {}): Sizes {
  return {
    ...NO_SIZES,
    round: 7,
    done: true,
    artifacts: [
      { key: jq.key, version: "1.0.0", measured: about(1_200_000), old_versions: null },
      { key: node.key, version: "1.0.0", measured: about(312_600_000), old_versions: about(298_400_000) },
    ],
    models: [{ instance_id: OLLAMA, measured: about(6_620_000_000) }],
    total: about(7_232_200_000),
    sources: [
      { instance_id: BREW, measured: about(612_200_000) },
      { instance_id: OLLAMA, measured: about(6_620_000_000) },
    ],
    ...overrides,
  };
}

const zh = i18n.getFixedT("zh-CN");
const en = i18n.getFixedT("en");

describe("sizeTotalsOf", () => {
  it("has nothing before the round is done, or for a round that did not measure the snapshot shown", () => {
    const snapshot = { round: 7, artifacts: [jq, node] };
    expect(sizeTotalsOf(undefined, snapshot)).toEqual({ all: null, bySource: new Map() });
    expect(sizeTotalsOf(done({ done: false, total: null, sources: [] }), snapshot).all).toBeNull();
    expect(sizeTotalsOf(done(), { ...snapshot, round: 8 })).toEqual({ all: null, bySource: new Map() });
    expect(sizeTotalsOf(done(), undefined).all).toBeNull();
  });

  it("is Rust's totals, about, when every tool in them has its size", () => {
    const totals = sizeTotalsOf(done(), { round: 7, artifacts: [jq, node, llama] });
    expect(totals.all).toEqual({ bytes: 7_232_200_000, atLeast: false });
    expect([...totals.bySource]).toEqual([
      [BREW, { bytes: 612_200_000, atLeast: false }],
      // A model counts through its Ollama's folder, never its own size.
      [OLLAMA, { bytes: 6_620_000_000, atLeast: false }],
    ]);
  });

  it("says at least where a tool has no size: the source's and the whole view's, and no other", () => {
    const totals = sizeTotalsOf(done(), { round: 7, artifacts: [jq, node, llama, requests] });
    expect(totals.all).toEqual({ bytes: 7_232_200_000, atLeast: true });
    expect(totals.bySource.get(BREW)).toEqual({ bytes: 612_200_000, atLeast: false });
    // pip is never measured: no number of its own at all.
    expect(totals.bySource.has(PIP)).toBe(false);

    const wget = tool(BREW, "Formula", "wget");
    const withWget = sizeTotalsOf(done(), { round: 7, artifacts: [jq, node, wget] });
    expect(withWget.bySource.get(BREW)).toEqual({ bytes: 612_200_000, atLeast: true });
  });

  it("says at least for a tool measured at another version, and for models whose folder has no line", () => {
    const newer = { ...jq, version: "1.1.0" };
    expect(sizeTotalsOf(done(), { round: 7, artifacts: [newer, node] }).bySource.get(BREW)?.atLeast).toBe(true);
    const unmeasured = sizeTotalsOf(done({ models: [] }), { round: 7, artifacts: [jq, node, llama] });
    expect(unmeasured.bySource.get(OLLAMA)?.atLeast).toBe(true);
    expect(unmeasured.all?.atLeast).toBe(true);
  });

  it("says at least where Rust's total is partial or cut short, and nothing for a total of nothing", () => {
    const short = done({
      total: about(7_000_000_000, { partial: true }),
      sources: [
        { instance_id: BREW, measured: about(612_200_000, { at_least: true }) },
        { instance_id: OLLAMA, measured: about(0, { at_least: true }) },
      ],
    });
    const totals = sizeTotalsOf(short, { round: 7, artifacts: [jq, node, llama] });
    expect(totals.all).toEqual({ bytes: 7_000_000_000, atLeast: true });
    expect(totals.bySource.get(BREW)).toEqual({ bytes: 612_200_000, atLeast: true });
    // Never 「至少约0 KB」.
    expect(totals.bySource.has(OLLAMA)).toBe(false);
  });
});

describe("the total words", () => {
  it("say about, or at least about, never what could be freed", () => {
    expect(sourceTotalText(zh, { bytes: 4_100_000_000, atLeast: false })).toBe("约4.1 GB");
    expect(sourceTotalText(zh, { bytes: 4_100_000_000, atLeast: true })).toBe("至少约4.1 GB");
    expect(viewTotalText(zh, { bytes: 9_800_000_000, atLeast: false })).toBe("共约9.8 GB");
    expect(viewTotalText(zh, { bytes: 9_800_000_000, atLeast: true })).toBe("共至少约9.8 GB");
    expect(sourceTotalText(en, { bytes: 4_100_000_000, atLeast: false })).toBe("about 4.1 GB");
    expect(sourceTotalText(en, { bytes: 4_100_000_000, atLeast: true })).toBe("at least about 4.1 GB");
    expect(viewTotalText(en, { bytes: 9_800_000_000, atLeast: false })).toBe("about 9.8 GB in all");
    expect(viewTotalText(en, { bytes: 9_800_000_000, atLeast: true })).toBe("at least about 9.8 GB in all");
    for (const text of [sourceTotalText(zh, { bytes: 1, atLeast: true }), viewTotalText(zh, { bytes: 1, atLeast: true })]) {
      expect(text).not.toMatch(/腾出|释放|清理/);
    }
  });
});
