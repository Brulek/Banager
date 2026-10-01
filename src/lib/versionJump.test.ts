import { describe, expect, it } from "vitest";
import type { ArtifactKind, UpdateCandidate } from "./types";
import { majorJump, majorOf } from "./versionJump";

function update(
  current: string,
  target: string,
  kind: ArtifactKind = "Formula",
  channel: UpdateCandidate["channel"] = "Native",
) {
  return { key: { kind }, current, target, channel };
}

describe("majorJump", () => {
  // Appendix A3's table (research synthesis §4.2 item 3), and the cases
  // around it.
  const TABLE: Array<[string, string, ArtifactKind, { from: number; to: number } | null]> = [
    ["3.31.6", "4.0.0", "Formula", { from: 3, to: 4 }],
    ["1.9", "10.0", "Formula", { from: 1, to: 10 }],
    ["v2.1.0", "v3.0.0", "Package", { from: 2, to: 3 }],
    ["V2.1.0", "3.0.0", "Package", { from: 2, to: 3 }],
    ["22.23.2_2", "22.23.3", "Formula", null],
    ["22.23.2_2", "23.0.0_1", "Formula", { from: 22, to: 23 }],
    ["0.155.1", "0.159.3", "Package", null],
    // 0.x to 1.0 is not marked either: the rule leaves every 0.x out.
    ["0.9.4", "1.0.0", "Tool", null],
    ["2026-08-13", "2026-09-30", "Cask", null],
    ["2025.12.01", "2026.09.28-64d2043", "Cask", null],
    ["20250101", "20260101", "Formula", null],
    ["r3222", "r3223", "Formula", null],
    ["latest", "latest", "Cask", null],
    ["4.0.0", "4.1.0", "Formula", null],
    ["4.1.0", "4.0.0", "Formula", null],
    ["", "2.0.0", "Formula", null],
    ["1.0.0", "", "Formula", null],
    ["HEAD-1a2b3c", "2.0.0", "Formula", null],
    ["3rc1", "4.0.0", "Formula", null],
    ["1.2.3", "2.0.0-beta.1", "Package", { from: 1, to: 2 }],
  ];

  it.each(TABLE)("%s → %s (%s)", (current, target, kind, expected) => {
    expect(majorJump(update(current, target, kind))).toEqual(expected);
  });

  it("never marks an Ollama model, even when its digests start with digits", () => {
    // Read as versions, these would be 1 → 9: a model's are digests, and
    // excluded by its kind, not by how they look.
    expect(majorJump(update("1a2b3c4d5e6f", "9f8e7d6c5b4a", "Model", "Digest"))).toBeNull();
    expect(majorJump(update("1.0", "2.0", "Model", "Digest"))).toBeNull();
    expect(majorJump(update("1.0", "2.0", "Model"))).toBeNull();
  });

  it("never marks an update whose versions are digests, whatever its kind", () => {
    expect(majorJump(update("1.0", "2.0", "Formula", "Digest"))).toBeNull();
  });
});

describe("majorOf", () => {
  it("reads the leading number, a v and a Homebrew revision aside", () => {
    expect(majorOf("v12.3")).toBe(12);
    expect(majorOf("7_1")).toBe(7);
    expect(majorOf("5")).toBe(5);
    expect(majorOf(" 3.1 ")).toBe(3);
    // A cask's version, a comma before its build number.
    expect(majorOf("5,1234")).toBe(5);
    expect(majorOf("4.2.1,20260901")).toBe(4);
  });

  it("reads nothing from what is not a version", () => {
    for (const value of ["latest", "r3222", "1a2b", "abc", "v", "", "99999999999999999999.1"]) {
      expect(majorOf(value)).toBeNull();
    }
  });
});
