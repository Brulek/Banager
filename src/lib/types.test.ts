import { describe, expect, it } from "vitest";
import type {
  Snapshot,
  Outcome,
  OperationEvent,
  UiEvent,
  Plan,
  OpSummary,
  ReadOnlyReason,
  Settings,
} from "./types";

// Every fixture below is a *typed* literal rather than a JSON string. vitest
// only strips types, so a JSON-string fixture would pass no matter what
// `types.ts` says; `pnpm build` runs `tsc` over the test files too (tsconfig
// `include` is `["src"]`), so a typed literal fails the build the moment a
// field name or variant drifts away from the Rust side. The JSON round trip
// keeps the runtime check that the wire shape (snake_case, externally tagged
// enums) survives serialisation unchanged.
function roundTrip<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

describe("types", () => {
  it("round-trips a realistic Snapshot (shape copied from canager-core's brew fixtures)", () => {
    const snapshot = {
      generation: 3,
      detect: "Found",
      instances: [
        {
          id: "brew:/opt/homebrew",
          adapter_id: "brew",
          exe_path: "/opt/homebrew/bin/brew",
          prefix: "/opt/homebrew",
          scope: "User",
          version: "7.0.3",
          healthy: true,
          unverified_version: null,
          read_only_reason: null,
        },
      ],
      artifacts: [
        {
          key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "jq" },
          display_name: "jq",
          version: "1.8.2",
          reason: "Requested",
          description: "Lightweight and flexible command-line JSON processor",
          homepage: "https://jqlang.github.io/jq/",
          size_bytes: null,
          installed_at: 1783762037,
          path: null,
          auto_updates: false,
        },
        {
          key: { instance_id: "brew:/opt/homebrew", kind: "Cask", name: "onyx" },
          display_name: "OnyX",
          version: "5.0.2",
          reason: "Requested",
          description: "Verify system files structure, run miscellaneous maintenance and more",
          homepage: "https://www.titanium-software.fr/en/onyx.html",
          size_bytes: null,
          installed_at: null,
          path: null,
          auto_updates: false,
        },
      ],
      updates: [
        {
          key: { instance_id: "brew:/opt/homebrew", kind: "Cask", name: "onyx" },
          current: "5.0.2",
          target: "5.1.0",
          channel: "Native",
          checkable: true,
          warnings: [],
        },
      ],
      refreshed_at: 1789700000,
      stale: false,
      errors: [],
    } satisfies Snapshot;

    const parsed = roundTrip<Snapshot>(snapshot);

    expect(parsed.generation).toBe(3);
    expect(parsed.detect).toBe("Found");
    expect(parsed.instances[0].scope).toBe("User");
    expect(parsed.instances[0].read_only_reason).toBeNull();
    expect(parsed.artifacts[0].key.kind).toBe("Formula");
    expect(parsed.artifacts[1].key.kind).toBe("Cask");
    expect(parsed.artifacts[1].installed_at).toBeNull();
    expect(parsed.updates[0].channel).toBe("Native");
    expect(parsed.stale).toBe(false);
    expect(parsed.errors).toEqual([]);
  });

  it("spells both ReadOnlyReason variants as bare strings, and writable as null", () => {
    // `Option<ReadOnlyReason>` on the Rust side: a unit variant serialises
    // to its bare name, `None` to `null`. Every spelling below has to match
    // `crates/canager-core/src/model.rs` exactly -- nothing checks this at
    // compile time, and a typo would silently land every npm row in the
    // wrong branch of the notice copy.
    const reasons: ReadOnlyReason[] = ["ByDesign", "PrefixNotWritable"];
    expect(roundTrip(reasons)).toEqual(["ByDesign", "PrefixNotWritable"]);
    const writable: ReadOnlyReason | null = null;
    expect(roundTrip(writable)).toBeNull();
  });

  it("keeps Outcome's externally tagged variants intact on the wire", () => {
    const succeeded: Outcome = "Succeeded";
    const needsAttention: Outcome = {
      NeedsAttention: "command succeeded but the package is not installed",
    };
    const failed: Outcome = { Failed: { exit_code: 1, summary: "boom" } };

    expect(roundTrip(succeeded)).toBe("Succeeded");
    expect(roundTrip(needsAttention)).toEqual({
      NeedsAttention: "command succeeded but the package is not installed",
    });
    expect(roundTrip(failed)).toEqual({ Failed: { exit_code: 1, summary: "boom" } });
    expect(JSON.stringify(failed)).toBe('{"Failed":{"exit_code":1,"summary":"boom"}}');
  });

  it("keeps OperationEvent and UiEvent wire shapes intact", () => {
    const log: OperationEvent = { Log: { op_id: 1, stream: "Stdout", line: "Installing jq" } };
    const uiEvent: UiEvent = { Operation: { Status: { op_id: 1, status: "Running" } } };
    const snapshotChanged: UiEvent = { SnapshotChanged: { generation: 7 } };

    expect(roundTrip(log)).toEqual({ Log: { op_id: 1, stream: "Stdout", line: "Installing jq" } });
    const parsedUiEvent = roundTrip(uiEvent);
    expect("Operation" in parsedUiEvent && parsedUiEvent.Operation).toEqual({
      Status: { op_id: 1, status: "Running" },
    });
    const parsedSnapshotChanged = roundTrip(snapshotChanged);
    expect(
      "SnapshotChanged" in parsedSnapshotChanged && parsedSnapshotChanged.SnapshotChanged,
    ).toEqual({ generation: 7 });
  });

  it("round-trips a Plan, an OpSummary and Settings", () => {
    const plan: Plan = {
      request: {
        kind: "Install",
        instance_id: "brew:/opt/homebrew",
        artifact_kind: "Formula",
        name: "jq",
      },
      program: "/opt/homebrew/bin/brew",
      args: ["install", "--formula", "jq"],
      env: [],
      needs_password: false,
      locks: ["brew:/opt/homebrew"],
      cancel_policy: "KillThenReconcile",
      warnings: [],
      affected: [],
      timeout_secs: 1800,
    };
    const opSummary: OpSummary = {
      id: 1,
      kind: "Install",
      instance_id: "brew:/opt/homebrew",
      artifact_kind: "Formula",
      name: "jq",
      status: "Running",
      outcome: null,
      argv_preview: ["/opt/homebrew/bin/brew", "install", "--formula", "jq"],
    };
    const settings: Settings = {
      language: "ZhCn",
      show_technical_details: true,
      ignored_updates: [],
      include_self_updating: false,
    };

    expect(roundTrip(plan).cancel_policy).toBe("KillThenReconcile");
    expect(roundTrip(plan).locks).toEqual(["brew:/opt/homebrew"]);
    expect(roundTrip(opSummary).status).toBe("Running");
    expect(roundTrip(opSummary).outcome).toBeNull();
    expect(roundTrip(settings).language).toBe("ZhCn");
  });
});
