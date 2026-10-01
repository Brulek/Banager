import { afterEach, describe, expect, it, vi } from "vitest";
import i18n from "../i18n";
import { diagnosticsText, type DiagnosticsInput } from "../lib/diagnostics";
import type { Snapshot } from "../lib/types";
import { buildWorld, HOME } from "./mockData";
import { mockSystemFacts } from "./mockDiagnostics";

const en = i18n.getFixedT("en");
const zh = i18n.getFixedT("zh-CN");

function input(more: Partial<DiagnosticsInput>): DiagnosticsInput {
  return {
    now: new Date(2026, 9, 1, 14, 3),
    appName: "Banager",
    appVersion: "0.1.0",
    languageName: "English",
    facts: null,
    snapshot: null,
    sizes: null,
    includeTools: false,
    ...more,
  };
}

describe("the diagnostic text of the browser preview's Mac", () => {
  afterEach(() => {
    vi.unstubAllEnvs();
  });

  it("holds no home folder and no environment variable's value, and adds the tools only when asked", () => {
    const world = buildWorld("full");
    const snapshot: Snapshot = {
      generation: 1,
      round: 1,
      detect: world.detect,
      instances: world.instances,
      artifacts: world.artifacts,
      updates: world.updates,
      refreshed_at: 1_790_000_000,
      stale: world.errors.length > 0,
      errors: world.errors,
    };
    // Values a shell might hold that must never be pasted to anyone.
    vi.stubEnv("HTTPS_PROXY", "http://someone:hunter2@proxy.example:8080");
    vi.stubEnv("GITHUB_TOKEN", "ghp_notARealTokenButLooksLikeOne123");
    const secrets = Object.entries(process.env)
      .filter(([name]) => name !== "PATH")
      .map(([, value]) => value ?? "")
      .filter((value) => value.length >= 8 && !value.startsWith("/"));
    for (const t of [en, zh]) {
      for (const includeTools of [false, true]) {
        const text = diagnosticsText(t, input({ snapshot, facts: mockSystemFacts(world.instances), includeTools }));
        expect(text).not.toContain("/Users/");
        expect(text).not.toContain(HOME);
        for (const secret of secrets) expect(text).not.toContain(secret);
        expect(text).not.toContain("hunter2");
        // Every source the Mac has is named.
        expect(text.split("\n").filter((line) => line.startsWith("  ") && !line.startsWith("    ")).length).toBeGreaterThan(
          world.instances.length * 3,
        );
      }
    }
    const plain = diagnosticsText(en, input({ snapshot, facts: mockSystemFacts(world.instances) }));
    const listed = diagnosticsText(en, input({ snapshot, facts: mockSystemFacts(world.instances), includeTools: true }));
    expect(plain.split("\n").some((line) => line.startsWith("    "))).toBe(false);
    expect(listed.split("\n").filter((line) => line.startsWith("    ")).length).toBe(world.artifacts.length);
  });
});
