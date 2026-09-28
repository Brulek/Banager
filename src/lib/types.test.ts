import { describe, expect, it } from "vitest";
import type {
  Snapshot,
  Outcome,
  OperationEvent,
  UiEvent,
  Plan,
  PlanAction,
  OpSummary,
  ReadOnlyReason,
  InstanceStatus,
  Settings,
  SkippedVersion,
  UninstallBlocked,
  UpdateBlocked,
  Warning,
  RemoveCheck,
  RemovedWhat,
  KeptWhat,
  EntryKind,
  ScanStop,
  UnknownScan,
} from "./types";

// Every fixture below is a *typed* literal rather than a JSON string. vitest
// only strips types, so a JSON-string fixture would pass no matter what
// `types.ts` says; `pnpm build` runs `tsc` over the test files too (through
// `tsconfig.test.json`), so a typed literal fails the build the moment a
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
          status: { unavailable: null, notes: [] },
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
          uninstall_blocked: null,
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
          uninstall_blocked: null,
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
          blocked: null,
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

  it("spells UpdateBlocked as a bare string, and an updatable candidate as null", () => {
    // `Option<UpdateBlocked>` on `UpdateCandidate.blocked` in
    // crates/canager-core/src/model.rs, whose
    // `test_update_blocked_is_a_bare_string_on_the_wire_and_null_when_absent`
    // asserts these exact spellings from the Rust side.
    const reasons: UpdateBlocked[] = ["Pinned", "SelfUpdatesOnly"];
    expect(JSON.stringify(reasons)).toBe('["Pinned","SelfUpdatesOnly"]');
    const updatable: UpdateBlocked | null = null;
    expect(roundTrip(updatable)).toBeNull();
  });

  it("spells UninstallBlocked as a bare string, and a removable artifact as null", () => {
    // `Option<UninstallBlocked>` on `InstalledArtifact.uninstall_blocked`
    // in crates/canager-core/src/model.rs, whose
    // `test_uninstall_blocked_is_a_bare_string_on_the_wire_and_null_when_absent`
    // asserts these exact spellings from the Rust side.
    const reasons: UninstallBlocked[] = ["Pinned", "NoSafeMethod", "UvToolDirSet"];
    expect(JSON.stringify(reasons)).toBe('["Pinned","NoSafeMethod","UvToolDirSet"]');
    const removable: UninstallBlocked | null = null;
    expect(roundTrip(removable)).toBeNull();
  });

  it("spells InstanceStatus as an always-present object with bare-string variants", () => {
    // `InstanceStatus` derives `Default` on the Rust side and is a plain
    // struct field, so it is never absent and never null: an available
    // source with nothing to report is `{unavailable: null, notes: []}`.
    // Both spellings below have to match
    // `crates/canager-core/src/model.rs` exactly -- nothing checks this at
    // compile time, and `status.unavailable` reading `undefined` because
    // the key moved would make every source look available, including the
    // Ollama whose notice is the only way to start it.
    const available: InstanceStatus = { unavailable: null, notes: [] };
    expect(roundTrip(available)).toEqual({ unavailable: null, notes: [] });
    expect(JSON.stringify(available)).toBe('{"unavailable":null,"notes":[]}');

    const notRunning: InstanceStatus = { unavailable: "NotRunning", notes: [] };
    const notResponding: InstanceStatus = {
      unavailable: "NotResponding",
      notes: ["IndexMayBeStale"],
    };
    const refusesAsRoot: InstanceStatus = { unavailable: "RefusesAsRoot", notes: [] };
    expect(JSON.stringify(notRunning)).toBe('{"unavailable":"NotRunning","notes":[]}');
    expect(JSON.stringify(refusesAsRoot)).toBe('{"unavailable":"RefusesAsRoot","notes":[]}');
    expect(JSON.stringify(notResponding)).toBe(
      '{"unavailable":"NotResponding","notes":["IndexMayBeStale"]}',
    );
    expect(roundTrip(notResponding)).toEqual(notResponding);

    // The five notes a standalone tool's detect can add (phase 4): which
    // copy runs when its name is typed, or that its launcher is left
    // without its program.
    const standalone: InstanceStatus = {
      unavailable: null,
      notes: ["NotOnPath", "ShadowedByHomebrew", "ShadowedByNpm", "ShadowedByOther", "LauncherOnly"],
    };
    expect(JSON.stringify(standalone)).toBe(
      '{"unavailable":null,"notes":["NotOnPath","ShadowedByHomebrew","ShadowedByNpm","ShadowedByOther","LauncherOnly"]}',
    );
    expect(roundTrip(standalone)).toEqual(standalone);
  });

  it("keeps Outcome's externally tagged variants intact on the wire", () => {
    const succeeded: Outcome = "Succeeded";
    const needsAttention: Outcome = { NeedsAttention: "GoneAfterUpgrade" };
    const failed: Outcome = { Failed: { exit_code: 1, summary: "boom" } };

    expect(roundTrip(succeeded)).toBe("Succeeded");
    // What `model.rs`'s `test_needs_attention_is_a_bare_variant_name_on_the_wire`
    // asserts serde emits.
    expect(JSON.stringify(needsAttention)).toBe('{"NeedsAttention":"GoneAfterUpgrade"}');
    // The same test's line for a path-list uninstall's own last look.
    const backAfter: Outcome = { NeedsAttention: "BackAfterUninstall" };
    expect(JSON.stringify(backAfter)).toBe('{"NeedsAttention":"BackAfterUninstall"}');
    expect(roundTrip(backAfter)).toEqual(backAfter);
    expect(roundTrip(failed)).toEqual({ Failed: { exit_code: 1, summary: "boom" } });
    expect(JSON.stringify(failed)).toBe('{"Failed":{"exit_code":1,"summary":"boom"}}');

    // What `model.rs`'s `test_canager_failed_is_externally_tagged_on_the_wire`
    // asserts serde emits: a unit `Fault` is a bare string, a data one a
    // single-key object.
    const panicked: Outcome = { CanagerFailed: "Panicked" };
    const missing: Outcome = {
      CanagerFailed: { ProgramMissing: { program: "/opt/homebrew/bin/brew" } },
    };
    const spawn: Outcome = {
      CanagerFailed: { SpawnFailed: { detail: "Permission denied (os error 13)" } },
    };
    expect(JSON.stringify(panicked)).toBe('{"CanagerFailed":"Panicked"}');
    expect(JSON.stringify(missing)).toBe(
      '{"CanagerFailed":{"ProgramMissing":{"program":"/opt/homebrew/bin/brew"}}}',
    );
    expect(JSON.stringify(spawn)).toBe(
      '{"CanagerFailed":{"SpawnFailed":{"detail":"Permission denied (os error 13)"}}}',
    );
    expect(roundTrip(missing)).toEqual(missing);
    // Phase 4 step C: a path changed between the preview and the run.
    const changed: Outcome = { CanagerFailed: { PathChanged: { path: "~/.local/bin/claude" } } };
    expect(JSON.stringify(changed)).toBe(
      '{"CanagerFailed":{"PathChanged":{"path":"~/.local/bin/claude"}}}',
    );
    expect(roundTrip(changed)).toEqual(changed);
  });

  it("spells Warning's bare-string variants as bare strings and WouldBreak/Message as externally tagged", () => {
    // Mirrors `Warning` in crates/canager-core/src/model.rs -- every
    // spelling below has to match it exactly. `warningKey` is exhaustive
    // over this union, so a variant it lacks fails `tsc`; but a spelling
    // here that differs from Rust's compiles fine and lands the real wire
    // value in `warningKey`'s `never` default at runtime, where it is
    // returned as a raw key. This test is what pins the spellings.
    const dependentsUnknown: Warning = "DependentsUnknown";
    const compilesLocally: Warning = "CompilesLocally";
    const nonRegistrySource: Warning = "NonRegistrySource";
    const wouldBreak: Warning = { WouldBreak: { names: ["python@3.13"] } };
    const thirdPartyRegistry: Warning = { ThirdPartyRegistry: { host: "modelscope.cn" } };
    const message: Warning = { Message: "boom" };

    expect(roundTrip(dependentsUnknown)).toBe("DependentsUnknown");
    expect(roundTrip(compilesLocally)).toBe("CompilesLocally");
    expect(roundTrip(nonRegistrySource)).toBe("NonRegistrySource");
    expect(JSON.stringify(wouldBreak)).toBe('{"WouldBreak":{"names":["python@3.13"]}}');
    expect(roundTrip(wouldBreak)).toEqual({ WouldBreak: { names: ["python@3.13"] } });
    expect(JSON.stringify(thirdPartyRegistry)).toBe(
      '{"ThirdPartyRegistry":{"host":"modelscope.cn"}}',
    );
    expect(roundTrip(thirdPartyRegistry)).toEqual({
      ThirdPartyRegistry: { host: "modelscope.cn" },
    });
    expect(JSON.stringify(message)).toBe('{"Message":"boom"}');
    expect(roundTrip(message)).toEqual({ Message: "boom" });

    // Phase 4 step C: the three struct variants a path-list uninstall
    // carries, and the two nested unit enums, spelled as
    // `test_warning_wire_shapes_match_the_hand_written_ts_mirror` in
    // crates/canager-core/src/model.rs asserts serde emits them.
    const willTrash: Warning = { WillTrash: { path: "~/.local/bin/claude", what: "Launcher" } };
    const willKeep: Warning = { WillKeep: { path: "~/.claude", what: "SettingsAndHistory" } };
    const alreadyGone: Warning = { AlreadyGone: { path: "~/.local/share/claude" } };
    expect(JSON.stringify(willTrash)).toBe(
      '{"WillTrash":{"path":"~/.local/bin/claude","what":"Launcher"}}',
    );
    expect(JSON.stringify(willKeep)).toBe(
      '{"WillKeep":{"path":"~/.claude","what":"SettingsAndHistory"}}',
    );
    expect(JSON.stringify(alreadyGone)).toBe('{"AlreadyGone":{"path":"~/.local/share/claude"}}');
    expect(roundTrip(willTrash)).toEqual(willTrash);
    const removed: RemovedWhat[] = ["Launcher", "Program", "Cache", "Backups"];
    const kept: KeptWhat[] = [
      "Settings",
      "SettingsAndHistory",
      "ToolState",
      "ShellConfigLines",
      "OutsideHome",
      "NotOurs",
      "InstallerCache",
    ];
    expect(JSON.stringify(removed)).toBe('["Launcher","Program","Cache","Backups"]');
    expect(JSON.stringify(kept)).toBe(
      '["Settings","SettingsAndHistory","ToolState","ShellConfigLines","OutsideHome","NotOurs","InstallerCache"]',
    );

    // Phase 4 step E: what rustup's own uninstall does. Pinned against
    // `test_warning_wire_shapes_match_the_hand_written_ts_mirror` in
    // crates/canager-core/src/model.rs.
    const removesToolchains: Warning = {
      RemovesToolchains: { path: "~/.rustup", names: ["stable-aarch64-apple-darwin"] },
    };
    const deletesCargoHome: Warning = { DeletesCargoHome: { path: "~/.cargo" } };
    const removesCargoInstalled: Warning = { RemovesCargoInstalled: { names: ["hexyl", "rg"] } };
    const homebrew: Warning = "HomebrewRustupLosesToolchains";
    const editsShellConfig: Warning = "EditsShellConfig";
    const leavesShellConfigLine: Warning = {
      LeavesShellConfigLine: { path: "~/.zshrc", certain: true },
    };
    expect(JSON.stringify(removesToolchains)).toBe(
      '{"RemovesToolchains":{"path":"~/.rustup","names":["stable-aarch64-apple-darwin"]}}',
    );
    expect(JSON.stringify(deletesCargoHome)).toBe('{"DeletesCargoHome":{"path":"~/.cargo"}}');
    expect(JSON.stringify(removesCargoInstalled)).toBe(
      '{"RemovesCargoInstalled":{"names":["hexyl","rg"]}}',
    );
    expect(roundTrip(homebrew)).toBe("HomebrewRustupLosesToolchains");
    expect(roundTrip(editsShellConfig)).toBe("EditsShellConfig");
    expect(JSON.stringify(leavesShellConfigLine)).toBe(
      '{"LeavesShellConfigLine":{"path":"~/.zshrc","certain":true}}',
    );
    expect(roundTrip(leavesShellConfigLine)).toEqual(leavesShellConfigLine);

    // Round 2: Homebrew's autoremove, back on through a brew.env file.
    // Pinned against the same Rust test.
    const autoremoves: Warning = "HomebrewAutoremoves";
    const periodicCleanup: Warning = "HomebrewPeriodicCleanup";
    const cleanupAutoremoves: Warning = "HomebrewCleanupAutoremoves";
    expect(JSON.stringify([autoremoves, periodicCleanup, cleanupAutoremoves])).toBe(
      '["HomebrewAutoremoves","HomebrewPeriodicCleanup","HomebrewCleanupAutoremoves"]',
    );

    // Round 2: an uninstall's sentence about what goes and what stays, and
    // a cask's extra steps. Pinned against the same Rust test.
    const scope: Warning = { UninstallScope: { what: "HomebrewCaskPlain" } };
    expect(JSON.stringify(scope)).toBe('{"UninstallScope":{"what":"HomebrewCaskPlain"}}');
    expect(roundTrip(scope)).toEqual(scope);
    const step: Warning = {
      CaskUninstallStep: { step: "RemovesPackages", items: ["com.microsoft.pkg.licensing"] },
    };
    expect(JSON.stringify(step)).toBe(
      '{"CaskUninstallStep":{"step":"RemovesPackages","items":["com.microsoft.pkg.licensing"]}}',
    );
    expect(roundTrip(step)).toEqual(step);

    // A `remove` step's check, as `only_if`: pinned against the same Rust
    // test, one shape per `RemoveCheck` variant.
    const checks: [RemoveCheck, string][] = [
      [{ LinkTargetContains: "playdate" }, '{"LinkTargetContains":"playdate"}'],
      [{ ContentContains: "SocketLock" }, '{"ContentContains":"SocketLock"}'],
      [
        { LinkTargetAndContentContain: { link_target: "MacGPG2", content: "gpg" } },
        '{"LinkTargetAndContentContain":{"link_target":"MacGPG2","content":"gpg"}}',
      ],
    ];
    for (const [check, json] of checks) {
      const checked: Warning = {
        CaskUninstallStep: { step: "Deletes", items: ["/usr/local/bin/arm-*"], only_if: check },
      };
      expect(JSON.stringify(checked)).toBe(
        `{"CaskUninstallStep":{"step":"Deletes","items":["/usr/local/bin/arm-*"],"only_if":${json}}}`,
      );
      expect(roundTrip(checked)).toEqual(checked);
    }
  });

  it("keeps OperationEvent and UiEvent wire shapes intact", () => {
    const log: OperationEvent = { Log: { op_id: 1, stream: "Stdout", line: "Installing jq" } };
    const uiEvent: UiEvent = { Operation: { Status: { op_id: 1, status: "Running" } } };
    const snapshotChanged: UiEvent = { SnapshotChanged: { generation: 7 } };

    expect(roundTrip(log)).toEqual({ Log: { op_id: 1, stream: "Stdout", line: "Installing jq" } });
    // Byte-for-byte what `events.rs`'s
    // `test_note_wire_shape_is_what_the_typescript_mirror_expects` asserts
    // serde emits: every `LogNote` variant is a struct variant, so a
    // one-key object carrying its data (serde's external tagging).
    const waiting: OperationEvent = {
      Note: { op_id: 7, note: { WaitingForBrewUpdate: { minutes: 10 } } },
    };
    expect(JSON.stringify(waiting)).toBe(
      '{"Note":{"op_id":7,"note":{"WaitingForBrewUpdate":{"minutes":10}}}}',
    );
    const readFailed: OperationEvent = {
      Note: {
        op_id: 7,
        note: { ReadFailed: { stream: "Stderr", error: "Input/output error (os error 5)" } },
      },
    };
    expect(JSON.stringify(readFailed)).toBe(
      '{"Note":{"op_id":7,"note":{"ReadFailed":{"stream":"Stderr","error":"Input/output error (os error 5)"}}}}',
    );
    // Phase 4 step C: what `events.rs`'s shape test asserts for the two
    // notes a path-list uninstall writes.
    const moved: OperationEvent = {
      Note: {
        op_id: 7,
        note: { MovedToTrash: { path: "~/.local/share/claude", trashed_to: "~/.Trash/claude" } },
      },
    };
    expect(JSON.stringify(moved)).toBe(
      '{"Note":{"op_id":7,"note":{"MovedToTrash":{"path":"~/.local/share/claude","trashed_to":"~/.Trash/claude"}}}}',
    );
    const trashFailed: OperationEvent = {
      Note: { op_id: 7, note: { TrashFailed: { path: "~/.local/bin/claude", error: "Operation not permitted" } } },
    };
    expect(JSON.stringify(trashFailed)).toBe(
      '{"Note":{"op_id":7,"note":{"TrashFailed":{"path":"~/.local/bin/claude","error":"Operation not permitted"}}}}',
    );
    // The third line of that uninstall's: the item it stopped before when
    // its budget ran out, and the budget in seconds (`Plan.timeout_secs`).
    const outOfTime: OperationEvent = {
      Note: { op_id: 7, note: { OutOfTime: { path: "~/.local/bin/claude", seconds: 120 } } },
    };
    expect(JSON.stringify(outOfTime)).toBe(
      '{"Note":{"op_id":7,"note":{"OutOfTime":{"path":"~/.local/bin/claude","seconds":120}}}}',
    );
    // The fourth: a path on the list that was there when the run
    // looked once more, after the pause that follows its last move.
    const backAfter: OperationEvent = {
      Note: { op_id: 7, note: { BackAfterUninstall: { path: "~/.local/share/claude" } } },
    };
    expect(JSON.stringify(backAfter)).toBe(
      '{"Note":{"op_id":7,"note":{"BackAfterUninstall":{"path":"~/.local/share/claude"}}}}',
    );
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
      action: {
        Command: { program: "/opt/homebrew/bin/brew", args: ["install", "--formula", "jq"], env: [] },
      },
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
      cancel_policy: "KillThenReconcile",
    };
    const settings: Settings = {
      language: "ZhCn",
      show_technical_details: true,
      ignored_updates: [],
      skipped_versions: [],
      include_self_updating: false,
      auto_check: false,
      notify_updates: false,
    };

    expect(roundTrip(plan).cancel_policy).toBe("KillThenReconcile");
    expect(roundTrip(plan).locks).toEqual(["brew:/opt/homebrew"]);
    expect(roundTrip(opSummary).status).toBe("Running");
    expect(roundTrip(opSummary).outcome).toBeNull();
    expect(roundTrip(settings).language).toBe("ZhCn");
  });

  it("spells Settings.skipped_versions as settings.rs's shape test does", () => {
    // `test_skipped_versions_wire_shape_matches_the_hand_written_ts_mirror`
    // in crates/canager-core/src/settings.rs asserts this exact string from
    // the Rust side: the key is the same object `ignored_updates` holds, and
    // the skipped version is a bare string.
    const skipped: SkippedVersion[] = [
      { key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "glib" }, version: "2.90.0" },
    ];
    expect(JSON.stringify(skipped)).toBe(
      '[{"key":{"instance_id":"brew:/opt/homebrew","kind":"Formula","name":"glib"},"version":"2.90.0"}]',
    );
    const settings: Settings = {
      language: "System",
      show_technical_details: false,
      ignored_updates: [],
      skipped_versions: skipped,
      include_self_updating: false,
      auto_check: false,
      notify_updates: false,
    };
    expect(roundTrip(settings).skipped_versions).toEqual(skipped);
  });

  it("spells Settings as settings.rs's shape test does, the daily check's two fields last", () => {
    // `test_default_settings_wire_shape_matches_the_hand_written_ts_mirror`
    // in crates/canager-core/src/settings.rs asserts this exact string from
    // the Rust side: `Settings::default()`, every field snake_case.
    const defaults: Settings = {
      language: "System",
      show_technical_details: false,
      ignored_updates: [],
      skipped_versions: [],
      include_self_updating: false,
      auto_check: false,
      notify_updates: false,
    };
    expect(JSON.stringify(defaults)).toBe(
      '{"language":"System","show_technical_details":false,"ignored_updates":[],"skipped_versions":[],"include_self_updating":false,"auto_check":false,"notify_updates":false}',
    );
  });

  it("spells PlanAction as two externally tagged arms, as model.rs's shape test does", () => {
    // `test_plan_action_is_externally_tagged_on_the_wire` in
    // crates/canager-core/src/model.rs asserts these exact strings from the
    // Rust side. `CommandPreview.tsx` branches on `"Command" in action`.
    const command: PlanAction = {
      Command: { program: "/opt/homebrew/bin/brew", args: ["install"], env: [["A", "1"]] },
    };
    const trash: PlanAction = { TrashPaths: { paths: ["/Users/someone/.local/bin/claude"] } };
    expect(JSON.stringify(command)).toBe(
      '{"Command":{"program":"/opt/homebrew/bin/brew","args":["install"],"env":[["A","1"]]}}',
    );
    expect(JSON.stringify(trash)).toBe('{"TrashPaths":{"paths":["/Users/someone/.local/bin/claude"]}}');
    expect(roundTrip(command)).toEqual(command);
    expect(roundTrip(trash)).toEqual(trash);
  });

  it("spells the unknown-source scan's shapes as Rust sends them", () => {
    // Mirrors `crates/canager-core/src/scan/mod.rs`, whose
    // `test_scan_wire_shapes_match_the_hand_written_ts_mirror` asserts
    // these exact spellings from the Rust side: `EntryKind` bare strings,
    // `ScanStop` externally tagged with the limit the scan enforced, and
    // an explicit `null` for a complete scan.
    const kinds: EntryKind[] = ["File", "Symlink", "BrokenSymlink"];
    expect(JSON.stringify(kinds)).toBe('["File","Symlink","BrokenSymlink"]');
    const fileLimit: ScanStop = { FileLimit: { max_entries: 2000 } };
    const timeLimit: ScanStop = { TimeLimit: { max_secs: 10 } };
    expect(JSON.stringify(fileLimit)).toBe('{"FileLimit":{"max_entries":2000}}');
    expect(JSON.stringify(timeLimit)).toBe('{"TimeLimit":{"max_secs":10}}');

    const scan: UnknownScan = {
      scanned: [{ path: "~/.local/bin", entries: 5 }],
      entries: [
        {
          path: "~/.local/bin/old-script",
          kind: "BrokenSymlink",
          resolved: null,
          link_target: "/Applications/Removed.app/Contents/Resources/index.js",
          size_bytes: null,
          modified_at: null,
          owned_by_me: true,
          app_bundle: "Removed",
        },
      ],
      attributed: 4,
      stopped: null,
    };
    expect(JSON.stringify(scan)).toBe(
      '{"scanned":[{"path":"~/.local/bin","entries":5}],"entries":[{"path":"~/.local/bin/old-script","kind":"BrokenSymlink","resolved":null,"link_target":"/Applications/Removed.app/Contents/Resources/index.js","size_bytes":null,"modified_at":null,"owned_by_me":true,"app_bundle":"Removed"}],"attributed":4,"stopped":null}',
    );
    expect(roundTrip(scan)).toEqual(scan);
    const stopped: UnknownScan = { ...scan, stopped: timeLimit };
    expect(roundTrip(stopped).stopped).toEqual({ TimeLimit: { max_secs: 10 } });
  });
});
