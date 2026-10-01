import { describe, expect, it } from "vitest";
import i18n from "../i18n";
import { compareBySize, modelsTotalText, oldVersionsText, sizeOrderOf, sizeText, sizeViewOf } from "./sizes";
import { NO_FACTS, NO_SIZES, type InstalledArtifact, type Sizes } from "./types";

const ruff: InstalledArtifact = {
  key: { instance_id: "uv", kind: "Tool", name: "ruff" },
  display_name: "ruff",
  version: "0.14.3",
  reason: "Requested",
  description: null,
  homepage: null,
  size_bytes: null,
  installed_at: null,
  path: "/Users/you/.local/share/uv/tools/ruff",
  auto_updates: false,
  uninstall_blocked: null,
  facts: NO_FACTS,
};

const OLLAMA = "ollama:http://127.0.0.1:11434";

function sizes(overrides: Partial<Sizes>): Sizes {
  return { ...NO_SIZES, round: 1, ...overrides };
}

const zh = i18n.getFixedT("zh-CN");
const en = i18n.getFixedT("en");

describe("sizeViewOf", () => {
  it("has nothing to show for an artifact the sizes do not list, or before they are asked for", () => {
    expect(sizeViewOf(undefined, ruff)).toBeNull();
    expect(sizeViewOf(NO_SIZES, ruff)).toBeNull();
    const other = sizes({
      artifacts: [{ key: { ...ruff.key, name: "black" }, version: "25.1.0", measured: null, old_versions: null }],
    });
    expect(sizeViewOf(other, ruff)).toBeNull();
  });

  it("is measuring while its size is not in, and while the size is of another version", () => {
    const pending = sizes({ artifacts: [{ key: ruff.key, version: "0.14.3", measured: null, old_versions: null }] });
    expect(sizeViewOf(pending, ruff)).toEqual({ kind: "measuring" });
    const older = sizes({
      artifacts: [
        { key: ruff.key, version: "0.14.2", measured: { bytes: 1, partial: false, at_least: false }, old_versions: null },
      ],
    });
    expect(sizeViewOf(older, ruff)).toEqual({ kind: "measuring" });
  });

  it("is the measured size, with the old versions, for the version listed", () => {
    const measured = { bytes: 312_600_000, partial: false, at_least: false };
    const old = { bytes: 298_400_000, partial: false, at_least: false };
    const done = sizes({ done: true, artifacts: [{ key: ruff.key, version: "0.14.3", measured, old_versions: old }] });
    expect(sizeViewOf(done, ruff)).toEqual({ kind: "measured", measured, oldVersions: old });
  });
});

describe("the size words", () => {
  it("say every number is rough, and how it is when part of it could not be measured", () => {
    const exact = { bytes: 312_600_000, partial: false, at_least: false };
    expect(sizeText(zh, exact)).toBe("约312.6 MB");
    expect(sizeText(en, exact)).toBe("About\u00a0312.6 MB");
    expect(sizeText(zh, { ...exact, at_least: true })).toBe("至少约312.6 MB");
    expect(sizeText(en, { ...exact, at_least: true })).toBe("At least about\u00a0312.6 MB");
    expect(sizeText(zh, { ...exact, partial: true })).toBe("约312.6 MB，部分无法读取");
    expect(sizeText(en, { ...exact, partial: true })).toBe("About\u00a0312.6 MB; some of it couldn't be read");
    // Both: the budget's word wins -- it is the larger "more than this".
    expect(sizeText(zh, { ...exact, partial: true, at_least: true })).toBe("至少约312.6 MB");
  });

  it("say a formula's old versions together, as at least when not all were measured", () => {
    const old = { bytes: 1_200_000_000, partial: false, at_least: false };
    expect(oldVersionsText(zh, old)).toBe("旧版本约1.2 GB");
    expect(oldVersionsText(en, old)).toBe("Old versions: about\u00a01.2 GB");
    expect(oldVersionsText(zh, { ...old, partial: true })).toBe("旧版本至少约1.2 GB");
  });

  it("say what an Ollama's models take together, and nothing while it is measured or for another source", () => {
    const done = sizes({
      done: true,
      models: [{ instance_id: OLLAMA, measured: { bytes: 6_620_000_000, partial: false, at_least: false } }],
    });
    expect(modelsTotalText(zh, done, OLLAMA)).toBe("Ollama模型共约6.6 GB");
    expect(modelsTotalText(en, done, OLLAMA)).toBe("Ollama models: about\u00a06.6 GB in all");
    expect(modelsTotalText(zh, done, "brew:/opt/homebrew")).toBeNull();
    expect(modelsTotalText(zh, sizes({ models: [{ instance_id: OLLAMA, measured: null }] }), OLLAMA)).toBeNull();
    expect(modelsTotalText(zh, undefined, OLLAMA)).toBeNull();
    const cut = sizes({
      models: [{ instance_id: OLLAMA, measured: { bytes: 6_620_000_000, partial: false, at_least: true } }],
    });
    expect(modelsTotalText(zh, cut, OLLAMA)).toBe("Ollama模型共至少约6.6 GB");
  });

  it("never claim what could be freed", () => {
    const words = [
      ...Object.values(i18n.getResourceBundle("zh-CN", "translation").sizes as Record<string, string>),
      ...Object.values(i18n.getResourceBundle("en", "translation").sizes as Record<string, string>),
    ];
    for (const text of words) {
      expect(text).not.toMatch(/腾出|释放|清理|free up|reclaim|clean/i);
    }
  });
});

describe("By Size", () => {
  const tool = (name: string, version = "1.0", size_bytes: number | null = null): InstalledArtifact => ({
    ...ruff,
    key: { ...ruff.key, name },
    display_name: name,
    version,
    size_bytes,
  });

  it("goes by a measured size of the version listed, or the size the source reports, and nothing else", () => {
    const big = tool("big");
    const small = tool("small");
    const stale = tool("stale", "2.0");
    const pending = tool("pending");
    const model = tool("model", "abc", 2_019_393_189);
    const order = sizeOrderOf(
      sizes({
        artifacts: [
          { key: big.key, version: "1.0", measured: { bytes: 900, partial: false, at_least: false }, old_versions: null },
          { key: small.key, version: "1.0", measured: { bytes: 10, partial: true, at_least: false }, old_versions: null },
          { key: stale.key, version: "1.0", measured: { bytes: 5_000, partial: false, at_least: false }, old_versions: null },
          { key: pending.key, version: "1.0", measured: null, old_versions: null },
        ],
      }),
      [big, small, stale, pending, model, tool("unknown")],
    );
    expect([...order.entries()]).toEqual([
      ["uv|Tool|big", 900],
      ["uv|Tool|small", 10],
      ["uv|Tool|model", 2_019_393_189],
    ]);
  });

  it("puts the larger first and a row with no size after every row with one", () => {
    const order = new Map([
      ["uv|Tool|a", 10],
      ["uv|Tool|b", 900],
    ]);
    const [a, b, c] = [tool("a"), tool("b"), tool("c")];
    expect([a, c, b].sort((x, y) => compareBySize(order, x, y)).map((t) => t.key.name)).toEqual(["b", "a", "c"]);
    expect(compareBySize(order, c, tool("d"))).toBe(0);
  });
});
