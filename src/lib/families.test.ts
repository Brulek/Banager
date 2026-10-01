import { describe, expect, it } from "vitest";
import { isAiTool, shownBy } from "./families";
import { NO_FACTS, type ArtifactFacts, type InstalledArtifact } from "./types";

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
    const tagged = { facts: { ...NO_FACTS, family: "ollama" } };
    const plain = { facts: NO_FACTS };
    expect([shownBy("all", tagged), shownBy("all", plain), shownBy("all", undefined)]).toEqual([true, true, true]);
    expect([shownBy("ai", tagged), shownBy("ai", plain), shownBy("ai", undefined)]).toEqual([true, false, false]);
  });
});
