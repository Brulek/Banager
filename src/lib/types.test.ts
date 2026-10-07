import { describe, expect, it } from "vitest";
import type {
  ManagerInstance,
  ArtifactFacts,
  CommandState,
  Snapshot,
  Outcome,
  OperationEvent,
  LogNote,
  UiEvent,
  InventoryPreview,
  Plan,
  PlanAction,
  OpSummary,
  ReadOnlyReason,
  InstanceStatus,
  Settings,
  SkippedVersion,
  UninstallBlocked,
  UpdateBlocked,
  UpdateCandidate,
  Warning,
  RemoveCheck,
  RemovedWhat,
  KeptWhat,
  EntryKind,
  ScanStop,
  UnknownScan,
  UpdatePair,
  FinishedRun,
  SnoozedUpdate,
  Sizes,
  SystemFacts,
} from "./types";
import { NO_FACTS, NO_SIZES } from "./types";

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
  it("round-trips when a source last answered as Rust's seconds-or-null answered_at", () => {
    for (const stamp of [null, 1_791_000_000]) {
      // As serde_json writes `ManagerInstance` (model.rs), field for field.
      const wire = JSON.stringify({
        id: "uv",
        adapter_id: "uv",
        exe_path: "/Users/you/.local/bin/uv",
        prefix: "/Users/you/.local/share/uv/tools",
        scope: "User",
        version: "0.9.2",
        answered_at: stamp,
        unverified_version: null,
        read_only_reason: null,
        status: { unavailable: "NotResponding", notes: [] },
      });
      const instance = JSON.parse(wire) as ManagerInstance;
      expect(instance.answered_at).toBe(stamp);
      expect(JSON.stringify(roundTrip<ManagerInstance>(instance))).toBe(wire);
    }
  });

  it("round-trips a realistic Snapshot (shape copied from banager-core's brew fixtures)", () => {
    const snapshot = {
      generation: 3,
      round: 5,
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
          answered_at: null,
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
          facts: NO_FACTS,
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
          facts: NO_FACTS,
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
    expect(parsed.round).toBe(5);
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

  it("spells HomebrewFacts as the Rust side writes it, every empty field an explicit null", () => {
    // The literal `test_homebrew_facts_spell_every_field_on_the_wire_and_read_back`
    // in crates/banager-core/src/model.rs asserts, byte for byte.
    const wire =
      '{"family":null,"homebrew":{"deprecated":null,"disabled":{"date":"2026-09-01","reason":"fails_gatekeeper_check","replacement":"onyx"},"caveats":"Turn on \\"Launch at login\\".\\n","other_versions":["3.6.3"]},"commands":[],"commands_unavailable":false}';
    const facts = {
      family: null,
      homebrew: {
        deprecated: null,
        disabled: { date: "2026-09-01", reason: "fails_gatekeeper_check", replacement: "onyx" },
        caveats: 'Turn on "Launch at login".\n',
        other_versions: ["3.6.3"],
      },
      commands: [],
      commands_unavailable: false,
    } satisfies ArtifactFacts;
    expect(JSON.stringify(facts)).toBe(wire);
    expect(roundTrip<ArtifactFacts>(JSON.parse(wire) as ArtifactFacts)).toEqual(facts);
    expect(JSON.stringify(NO_FACTS)).toBe('{"family":null,"homebrew":null,"commands":[],"commands_unavailable":false}');
  });

  it("round-trips unavailable command coverage even when all claims were dropped", () => {
    // Same literal as Rust's test_unavailable_commands_round_trip_even_when_every_claim_was_dropped.
    const wire = '{"family":null,"homebrew":null,"commands":[],"commands_unavailable":true}';
    const facts: ArtifactFacts = { ...NO_FACTS, commands_unavailable: true };
    expect(JSON.stringify(facts)).toBe(wire);
    expect(roundTrip<ArtifactFacts>(JSON.parse(wire))).toEqual(facts);
  });

  it("reads Snapshot.next_auto_check_at as ipc.rs's wire test sends it: Unix seconds, or null before any check", () => {
    // test_every_snapshot_the_window_is_handed_says_when_the_daily_check_is_next_due
    // in src-tauri/src/ipc.rs: a window round at 1790586000, due a day on.
    const wire = {
      generation: 1,
      round: 1,
      detect: "Found",
      instances: [],
      artifacts: [],
      updates: [],
      refreshed_at: 1790586000,
      stale: false,
      errors: [],
      next_auto_check_at: 1790672400,
    } satisfies Snapshot;
    expect(roundTrip<Snapshot>(wire).next_auto_check_at).toBe(1790672400);
    expect(roundTrip<Snapshot>({ ...wire, next_auto_check_at: null }).next_auto_check_at).toBeNull();
    expect(Object.keys(roundTrip<Snapshot>(wire))).toEqual([
      "generation",
      "round",
      "detect",
      "instances",
      "artifacts",
      "updates",
      "refreshed_at",
      "stale",
      "errors",
      "next_auto_check_at",
    ]);
  });

  it("spells every ReadOnlyReason variant as a bare string, and writable as null", () => {
    // `Option<ReadOnlyReason>` on the Rust side: a unit variant serialises
    // to its bare name, `None` to `null`. Every spelling below has to match
    // `crates/banager-core/src/model.rs` exactly -- nothing checks this at
    // compile time, and a typo would silently land every npm row in the
    // wrong branch of the notice copy.
    const reasons: ReadOnlyReason[] = ["ByDesign", "PrefixNotWritable", "PrefixProtected"];
    expect(roundTrip(reasons)).toEqual(["ByDesign", "PrefixNotWritable", "PrefixProtected"]);
    const writable: ReadOnlyReason | null = null;
    expect(roundTrip(writable)).toBeNull();
  });

  it("spells UpdateBlocked as a bare string, and an updatable candidate as null", () => {
    // `Option<UpdateBlocked>` on `UpdateCandidate.blocked` in
    // crates/banager-core/src/model.rs, whose
    // `test_update_blocked_is_a_bare_string_on_the_wire_and_null_when_absent`
    // asserts these exact spellings from the Rust side.
    const reasons: UpdateBlocked[] = ["Pinned", "SelfUpdatesOnly", "Disabled"];
    expect(JSON.stringify(reasons)).toBe('["Pinned","SelfUpdatesOnly","Disabled"]');
    expect(JSON.parse('{"blocked":"Disabled"}')).toEqual({ blocked: "Disabled" satisfies UpdateBlocked });
    const updatable: UpdateBlocked | null = null;
    expect(roundTrip(updatable)).toBeNull();
  });

  it("reads an update's download_bytes as a number, null, or left out", () => {
    // `Option<u64>` on `UpdateCandidate.download_bytes` in
    // crates/banager-core/src/model.rs, whose
    // `test_download_bytes_is_a_number_or_null_on_the_wire_and_optional_when_read`
    // asserts the same spellings from the Rust side.
    const wire =
      '{"key":{"instance_id":"ollama:http://127.0.0.1:11434","kind":"Model","name":"llama3.2:3b"},' +
      '"current":"8e4c","target":"sha256:25a9","channel":"Digest","checkable":true,"warnings":[],' +
      '"blocked":null,"download_bytes":4683087520}';
    const model = JSON.parse(wire) as UpdateCandidate;
    expect(model.download_bytes).toBe(4_683_087_520);
    expect(roundTrip(model)).toEqual(model);
    expect(JSON.stringify(model)).toBe(wire);
    const unknown: UpdateCandidate = { ...model, download_bytes: null };
    expect(roundTrip(unknown).download_bytes).toBeNull();
    // Written before the field existed: reads as not known.
    const older: UpdateCandidate = { ...model };
    delete older.download_bytes;
    expect(roundTrip(older).download_bytes).toBeUndefined();
  });

  it("spells UninstallBlocked as a bare string, and a removable artifact as null", () => {
    // `Option<UninstallBlocked>` on `InstalledArtifact.uninstall_blocked`
    // in crates/banager-core/src/model.rs, whose
    // `test_uninstall_blocked_is_a_bare_string_on_the_wire_and_null_when_absent`
    // asserts these exact spellings from the Rust side.
    const reasons: UninstallBlocked[] = ["Pinned", "NoSafeMethod", "UvToolDirSet", "SourceProgram", "NeededBySource"];
    expect(JSON.stringify(reasons)).toBe('["Pinned","NoSafeMethod","UvToolDirSet","SourceProgram","NeededBySource"]');
    expect(roundTrip(reasons)).toEqual(reasons);
    const removable: UninstallBlocked | null = null;
    expect(roundTrip(removable)).toBeNull();
  });

  it("spells Warning.NeededBySource as model.rs's wire test does", () => {
    // `test_needed_by_source_is_the_json_the_typescript_mirror_reads` in
    // crates/banager-core/src/model.rs writes exactly this string.
    const wire = '{"NeededBySource":{"instance_id":"npm:/opt/homebrew","program":true,"tools":4}}';
    const warning: Warning = { NeededBySource: { instance_id: "npm:/opt/homebrew", program: true, tools: 4 } };
    expect(JSON.stringify(warning)).toBe(wire);
    expect(JSON.parse(wire) as Warning).toEqual(warning);
  });

  it("spells ArtifactFacts' commands as model.rs's wire test does", () => {
    // `test_command_facts_are_externally_tagged_and_their_inputs_never_reach_the_wire`
    // in crates/banager-core/src/model.rs serialises these exact facts;
    // the string below is what it asserts.
    const facts: ArtifactFacts = {
      family: null,
      homebrew: null,
      commands: [
        {
          name: "agent",
          state: {
            ShadowedBy: {
              by: { instance_id: "npm:/opt/homebrew", kind: "Package", name: "@anthropic-ai/claude-code" },
            },
          },
        },
        { name: "claude", state: "Runs" },
        { name: "grok", state: { NotOnPath: { dir: "~/.grok/bin" } } },
        { name: "rg", state: { ShadowedBy: { by: null } } },
        { name: "curl", state: null },
      ],
      commands_unavailable: false,
    };
    const wire =
      '{"family":null,"homebrew":null,"commands":[' +
      '{"name":"agent","state":{"ShadowedBy":{"by":{"instance_id":"npm:/opt/homebrew","kind":"Package","name":"@anthropic-ai/claude-code"}}}},' +
      '{"name":"claude","state":"Runs"},' +
      '{"name":"grok","state":{"NotOnPath":{"dir":"~/.grok/bin"}}},' +
      '{"name":"rg","state":{"ShadowedBy":{"by":null}}},' +
      '{"name":"curl","state":null}' +
      '],"commands_unavailable":false}';
    expect(JSON.stringify(facts)).toBe(wire);
    expect(roundTrip(facts)).toEqual(facts);
    // `ArtifactFacts::default()`.
    expect(JSON.stringify(NO_FACTS)).toBe('{"family":null,"homebrew":null,"commands":[],"commands_unavailable":false}');
    const states: CommandState[] = facts.commands.flatMap((command) => (command.state === null ? [] : [command.state]));
    expect(states).toHaveLength(4);
  });

  it("spells InstanceStatus as an always-present object with bare-string variants", () => {
    // `InstanceStatus` derives `Default` on the Rust side and is a plain
    // struct field, so it is never absent and never null: an available
    // source with nothing to report is `{unavailable: null, notes: []}`.
    // Both spellings below have to match
    // `crates/banager-core/src/model.rs` exactly -- nothing checks this at
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
    // The two added in round 5: spelled as `Unavailable` in model.rs.
    const httpsHostRefused: InstanceStatus = { unavailable: "HttpsHostRefused", notes: [] };
    const noPip: InstanceStatus = { unavailable: "NoPip", notes: [] };
    expect(JSON.stringify(httpsHostRefused)).toBe('{"unavailable":"HttpsHostRefused","notes":[]}');
    expect(JSON.stringify(noPip)).toBe('{"unavailable":"NoPip","notes":[]}');
    expect(roundTrip(noPip)).toEqual(noPip);

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
    const failed: Outcome = { Failed: { exit_code: 1, summary: "boom", cause: null } };

    expect(roundTrip(succeeded)).toBe("Succeeded");
    // What `model.rs`'s `test_needs_attention_is_a_bare_variant_name_on_the_wire`
    // asserts serde emits.
    expect(JSON.stringify(needsAttention)).toBe('{"NeedsAttention":"GoneAfterUpgrade"}');
    // The same test's line for a path-list uninstall's own last look.
    const backAfter: Outcome = { NeedsAttention: "BackAfterUninstall" };
    expect(JSON.stringify(backAfter)).toBe('{"NeedsAttention":"BackAfterUninstall"}');
    expect(roundTrip(backAfter)).toEqual(backAfter);
    expect(roundTrip(failed)).toEqual({ Failed: { exit_code: 1, summary: "boom", cause: null } });
    expect(JSON.stringify(failed)).toBe('{"Failed":{"exit_code":1,"summary":"boom","cause":null}}');
    // What `model.rs`'s `test_outcome_failed_carries_its_cause_on_the_wire`
    // asserts serde emits: the cause read before a login was masked out of
    // the summary (re-check 2's N1).
    const stopped: Outcome = {
      Failed: { exit_code: 1, summary: "sudo: a ****word is required", cause: "needsPassword" },
    };
    expect(JSON.stringify(stopped)).toBe(
      '{"Failed":{"exit_code":1,"summary":"sudo: a ****word is required","cause":"needsPassword"}}',
    );
    expect(roundTrip(stopped)).toEqual(stopped);

    // What `model.rs`'s `test_banager_failed_is_externally_tagged_on_the_wire`
    // asserts serde emits: a unit `Fault` is a bare string, a data one a
    // single-key object.
    const panicked: Outcome = { BanagerFailed: "Panicked" };
    const missing: Outcome = {
      BanagerFailed: { ProgramMissing: { program: "/opt/homebrew/bin/brew" } },
    };
    const spawn: Outcome = {
      BanagerFailed: { SpawnFailed: { detail: "Permission denied (os error 13)" } },
    };
    expect(JSON.stringify(panicked)).toBe('{"BanagerFailed":"Panicked"}');
    expect(JSON.stringify(missing)).toBe(
      '{"BanagerFailed":{"ProgramMissing":{"program":"/opt/homebrew/bin/brew"}}}',
    );
    expect(JSON.stringify(spawn)).toBe(
      '{"BanagerFailed":{"SpawnFailed":{"detail":"Permission denied (os error 13)"}}}',
    );
    expect(roundTrip(missing)).toEqual(missing);
    // Phase 4 step C: a path changed between the preview and the run.
    const changed: Outcome = { BanagerFailed: { PathChanged: { path: "~/.local/bin/claude" } } };
    expect(JSON.stringify(changed)).toBe(
      '{"BanagerFailed":{"PathChanged":{"path":"~/.local/bin/claude"}}}',
    );
    expect(roundTrip(changed)).toEqual(changed);
    // Review F3 (r6): an uninstall of every version of a formula found its
    // versions or its pin changed since the preview (model.rs builds the
    // same string).
    const formula: Outcome = { BanagerFailed: { FormulaChanged: { name: "wget" } } };
    expect(JSON.stringify(formula)).toBe('{"BanagerFailed":{"FormulaChanged":{"name":"wget"}}}');
    expect(roundTrip(formula)).toEqual(formula);
    // Review of v1-brew's fixes (r6): an install or update found Homebrew
    // would now delete more than its preview said (model.rs builds the same
    // string).
    const settings: Outcome = { BanagerFailed: "HomebrewSettingsChanged" };
    expect(JSON.stringify(settings)).toBe('{"BanagerFailed":"HomebrewSettingsChanged"}');
    expect(roundTrip(settings)).toEqual(settings);
  });

  it("spells Warning's bare-string variants as bare strings and WouldBreak/Message as externally tagged", () => {
    // Mirrors `Warning` in crates/banager-core/src/model.rs -- every
    // spelling below has to match it exactly. `warningKey` is exhaustive
    // over this union, so a variant it lacks fails `tsc`; but a spelling
    // here that differs from Rust's compiles fine and lands the real wire
    // value in `warningKey`'s `never` default at runtime, where it is
    // returned as a raw key. This test is what pins the spellings.
    const dependentsUnknown: Warning = "DependentsUnknown";
    const compilesLocally: Warning = "CompilesLocally";
    const downloadsModelChanges: Warning = "DownloadsModelChanges";
    const nonRegistrySource: Warning = "NonRegistrySource";
    const transientLookupFailure: Warning = "TransientLookupFailure";
    const wouldBreak: Warning = { WouldBreak: { names: ["python@3.13"] } };
    const thirdPartyRegistry: Warning = { ThirdPartyRegistry: { host: "modelscope.cn" } };
    const message: Warning = { Message: "boom" };

    expect(roundTrip(dependentsUnknown)).toBe("DependentsUnknown");
    expect(roundTrip(compilesLocally)).toBe("CompilesLocally");
    expect(roundTrip(downloadsModelChanges)).toBe("DownloadsModelChanges");
    expect(roundTrip(nonRegistrySource)).toBe("NonRegistrySource");
    expect(roundTrip(transientLookupFailure)).toBe("TransientLookupFailure");
    // A failed lookup a later check can get past, as Rust sends it: the
    // reason, then the mark (`uncheckable_candidate`).
    expect(roundTrip<Warning[]>([{ Message: "npm error code ENOTFOUND" }, "TransientLookupFailure"])).toEqual([
      { Message: "npm error code ENOTFOUND" },
      "TransientLookupFailure",
    ]);
    // A tool Banager does not look up on this Mac, by design, as Rust
    // sends it: the reason, then the mark (`uncheckable_candidate`).
    const notLookedUpHere: Warning = "NotLookedUpHere";
    expect(JSON.stringify(notLookedUpHere)).toBe('"NotLookedUpHere"');
    expect(
      roundTrip<Warning[]>([{ Message: "remote daemon manifests cannot be checked from this Mac" }, notLookedUpHere]),
    ).toEqual([{ Message: "remote daemon manifests cannot be checked from this Mac" }, "NotLookedUpHere"]);
    // A secure connection rustls would not set up: the reason, then the
    // host to name, and never the mark (`uncheckable_candidate`).
    const secureConnectionFailed: Warning = { SecureConnectionFailed: { host: "crates.io" } };
    expect(JSON.stringify(secureConnectionFailed)).toBe('{"SecureConnectionFailed":{"host":"crates.io"}}');
    expect(
      roundTrip<Warning[]>([
        { Message: "crates.io request failed: secure connection to crates.io failed: invalid peer certificate: UnknownIssuer" },
        secureConnectionFailed,
      ]),
    ).toEqual([
      { Message: "crates.io request failed: secure connection to crates.io failed: invalid peer certificate: UnknownIssuer" },
      { SecureConnectionFailed: { host: "crates.io" } },
    ]);
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
    // crates/banager-core/src/model.rs asserts serde emits them.
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
    // crates/banager-core/src/model.rs.
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
    // z1's review: the same when a brew.env in a protected place wasn't
    // read, and a startup file rustup's preview could not read. Pinned
    // against the same Rust test.
    const may: Warning[] = [
      "HomebrewMayAutoremove",
      "HomebrewMayCleanUp",
      "HomebrewCleanupMayAutoremove",
      "HomebrewMayAutoUpdate",
    ];
    expect(JSON.stringify(may)).toBe(
      '["HomebrewMayAutoremove","HomebrewMayCleanUp","HomebrewCleanupMayAutoremove","HomebrewMayAutoUpdate"]',
    );
    expect(roundTrip("HomebrewMayAutoUpdate" as Warning)).toBe("HomebrewMayAutoUpdate");
    const shellConfigUnread: Warning = { ShellConfigUnread: { path: "~/.zshrc" } };
    expect(JSON.stringify(shellConfigUnread)).toBe('{"ShellConfigUnread":{"path":"~/.zshrc"}}');
    expect(roundTrip(shellConfigUnread)).toEqual(shellConfigUnread);
    // Round 5: what HOMEBREW_NO_CLEANUP_FORMULAE leaves out of them.
    const noCleanup: Warning = {
      HomebrewNoCleanupFormulae: { names: ["python@3.13"], old_versions: true, autoremove: false },
    };
    expect(JSON.stringify(noCleanup)).toBe(
      '{"HomebrewNoCleanupFormulae":{"names":["python@3.13"],"old_versions":true,"autoremove":false}}',
    );
    expect(roundTrip(noCleanup)).toEqual(noCleanup);
    const forgetsTrust: Warning = { HomebrewForgetsTrust: { name: "gautham-v/tap/claudebar" } };
    expect(JSON.stringify(forgetsTrust)).toBe('{"HomebrewForgetsTrust":{"name":"gautham-v/tap/claudebar"}}');
    expect(roundTrip(forgetsTrust)).toEqual(forgetsTrust);

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
      env_preview: [["HOMEBREW_NO_AUTOREMOVE", "1"]],
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
    // r6 y3-batch: how an update already at its target was done, as Rust's
    // test_op_summary_wire_shape_carries_already_updated_and_reads_without_it
    // writes it; a summary without it reads as none.
    const already: OpSummary = JSON.parse(
      '{"id":10,"kind":"Upgrade","instance_id":"brew:/opt/homebrew","artifact_kind":"Formula","name":"libpng","status":"Done","outcome":"Succeeded","argv_preview":[],"env_preview":[],"cancel_policy":"KillThenReconcile","already_updated":"ByEarlierUpdate"}',
    );
    expect(roundTrip(already).already_updated).toBe("ByEarlierUpdate");
    expect(roundTrip(opSummary).already_updated ?? null).toBeNull();
    // A pair each, as Rust's `(String, String)` serializes.
    expect(roundTrip(opSummary).env_preview).toEqual([["HOMEBREW_NO_AUTOREMOVE", "1"]]);
    expect(roundTrip(settings).language).toBe("ZhCn");
  });

  it("spells Settings.skipped_versions as settings.rs's shape test does", () => {
    // `test_skipped_versions_wire_shape_matches_the_hand_written_ts_mirror`
    // in crates/banager-core/src/settings.rs asserts this exact string from
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

  it("spells Settings as settings.rs's shape test does, the fields added since in the order they came", () => {
    // `test_default_settings_wire_shape_matches_the_hand_written_ts_mirror`
    // in crates/banager-core/src/settings.rs asserts this exact string from
    // the Rust side: `Settings::default()`, every field snake_case.
    const defaults: Settings = {
      language: "System",
      show_technical_details: false,
      ignored_updates: [],
      skipped_versions: [],
      include_self_updating: false,
      auto_check: false,
      notify_updates: false,
      auto_check_every: "Day",
      notify_operations: false,
      snoozed_updates: [],
      welcome_seen: false,
    };
    expect(JSON.stringify(defaults)).toBe(
      '{"language":"System","show_technical_details":false,"ignored_updates":[],"skipped_versions":[],"include_self_updating":false,"auto_check":false,"notify_updates":false,"auto_check_every":"Day","notify_operations":false,"snoozed_updates":[],"welcome_seen":false}',
    );
  });

  it("round-trips welcome_seen, and reads its absence as not stated", () => {
    // `test_welcome_seen_saved_true_loads_true` in
    // crates/banager-core/src/settings.rs: Rust always sends it. Settings a
    // page or test builds by hand may leave it out, and the welcome sheet
    // shows only for an explicit false (`welcomeDue`).
    const seen: Settings = {
      language: "ZhCn",
      show_technical_details: false,
      ignored_updates: [],
      skipped_versions: [],
      include_self_updating: false,
      auto_check: false,
      notify_updates: false,
      welcome_seen: true,
    };
    expect(roundTrip(seen).welcome_seen).toBe(true);
    const without: Settings = { ...seen };
    delete without.welcome_seen;
    expect(roundTrip(without).welcome_seen).toBeUndefined();
  });

  it("spells SnoozedUpdate as settings.rs's shape test does", () => {
    // `test_snoozed_updates_wire_shape_matches_the_hand_written_ts_mirror`
    // in crates/banager-core/src/settings.rs asserts this exact string.
    const snoozed: SnoozedUpdate[] = [
      { key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "wget" }, until: 1793178000 },
    ];
    expect(JSON.stringify(snoozed)).toBe(
      '[{"key":{"instance_id":"brew:/opt/homebrew","kind":"Formula","name":"wget"},"until":1793178000}]',
    );
  });

  it("spells FinishedRun as notify_operations.rs's shape test reads it", () => {
    // `test_finished_run_is_the_json_the_page_sends` in
    // crates/banager-core/src/notify_operations.rs parses this exact string.
    const run: FinishedRun = { last_op: 7, kind: "Upgrade", succeeded: 2, failed: 1, attention: 0 };
    expect(JSON.stringify(run)).toBe('{"last_op":7,"kind":"Upgrade","succeeded":2,"failed":1,"attention":0}');
  });

  it("spells UpdatePair as notify_updates.rs's shape test reads it", () => {
    // `test_update_pair_is_the_json_the_page_sends` in
    // crates/banager-core/src/notify_updates.rs parses this exact string.
    const pair: UpdatePair = { key_id: "brew:/opt/homebrew|Formula|jq", target: "1.8.1" };
    expect(JSON.stringify(pair)).toBe('{"key_id":"brew:/opt/homebrew|Formula|jq","target":"1.8.1"}');
  });

  it("spells PlanAction as two externally tagged arms, as model.rs's shape test does", () => {
    // `test_plan_action_is_externally_tagged_on_the_wire` in
    // crates/banager-core/src/model.rs asserts these exact strings from the
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

  it("spells U9's Homebrew cleanup as model.rs and events.rs do", () => {
    // `test_the_homebrew_version_cleanup_is_on_the_wire_as_the_mirror_spells_it`
    // in model.rs and `test_the_cleanup_notes_wire_shape_is_what_the_typescript_mirror_expects`
    // in events.rs assert these exact strings from the Rust side.
    const then: PlanAction = {
      CommandThen: {
        program: "/opt/homebrew/bin/brew",
        args: ["upgrade", "--formula", "wget"],
        env: [["HOMEBREW_NO_AUTOREMOVE", "1"]],
        then: [["cleanup", "wget"]],
      },
    };
    expect(JSON.stringify(then)).toBe(
      '{"CommandThen":{"program":"/opt/homebrew/bin/brew","args":["upgrade","--formula","wget"],"env":[["HOMEBREW_NO_AUTOREMOVE","1"]],"then":[["cleanup","wget"]]}}',
    );
    expect(roundTrip(then)).toEqual(then);
    const cleans: Warning = { HomebrewCleansUpOldVersions: { versions: ["1.24.0", "1.25.0"] } };
    expect(JSON.stringify(cleans)).toBe('{"HomebrewCleansUpOldVersions":{"versions":["1.24.0","1.25.0"]}}');
    const every: Warning = { HomebrewRemovesEveryVersion: { versions: ["1.25.0", "1.26.0"] } };
    expect(JSON.stringify(every)).toBe('{"HomebrewRemovesEveryVersion":{"versions":["1.25.0","1.26.0"]}}');
    expect(roundTrip(cleans)).toEqual(cleans);
    expect(roundTrip(every)).toEqual(every);
    const starting: OperationEvent = { Note: { op_id: 7, note: { CleaningUpOldVersions: { name: "wget" } } } };
    expect(JSON.stringify(starting)).toBe('{"Note":{"op_id":7,"note":{"CleaningUpOldVersions":{"name":"wget"}}}}');
    const failed: OperationEvent = {
      Note: { op_id: 7, note: { OldVersionsNotCleanedUp: { name: "wget", exit_code: 1 } } },
    };
    expect(JSON.stringify(failed)).toBe(
      '{"Note":{"op_id":7,"note":{"OldVersionsNotCleanedUp":{"name":"wget","exit_code":1}}}}',
    );
    const stopped: OperationEvent = {
      Note: { op_id: 7, note: { OldVersionsNotCleanedUp: { name: "wget", exit_code: null } } },
    };
    expect(JSON.stringify(stopped)).toBe(
      '{"Note":{"op_id":7,"note":{"OldVersionsNotCleanedUp":{"name":"wget","exit_code":null}}}}',
    );
    expect(roundTrip(stopped)).toEqual(stopped);
    const kept: OperationEvent = {
      Note: { op_id: 7, note: { OldVersionsKept: { name: "wget", versions: ["1.24.0", "1.25.0"] } } },
    };
    expect(JSON.stringify(kept)).toBe(
      '{"Note":{"op_id":7,"note":{"OldVersionsKept":{"name":"wget","versions":["1.24.0","1.25.0"]}}}}',
    );
    expect(roundTrip(kept)).toEqual(kept);
    // Review F4 (r6): asked again at its turn, the settings no longer let
    // the cleanup run (events.rs builds the same string).
    const skipped: OperationEvent = { Note: { op_id: 7, note: { OldVersionsCleanupSkipped: { name: "wget" } } } };
    expect(JSON.stringify(skipped)).toBe('{"Note":{"op_id":7,"note":{"OldVersionsCleanupSkipped":{"name":"wget"}}}}');
    expect(roundTrip(skipped)).toEqual(skipped);
  });

  it("spells y1-keg's relink after an update as model.rs and events.rs do", () => {
    // model.rs's `test_the_homebrew_version_cleanup_is_on_the_wire_as_the_mirror_spells_it`
    // and `test_banager_failed_is_externally_tagged_on_the_wire`, events.rs's
    // `test_the_keg_only_relink_notes_are_on_the_wire_as_the_mirror_spells_them`
    // assert these exact strings from the Rust side.
    const both: PlanAction = {
      CommandThen: {
        program: "/opt/homebrew/bin/brew",
        args: ["upgrade", "--formula", "node@22"],
        env: [],
        then: [
          ["link", "--force", "node@22"],
          ["cleanup", "node@22"],
        ],
      },
    };
    expect(JSON.stringify(both)).toBe(
      '{"CommandThen":{"program":"/opt/homebrew/bin/brew","args":["upgrade","--formula","node@22"],"env":[],"then":[["link","--force","node@22"],["cleanup","node@22"]]}}',
    );
    expect(roundTrip(both)).toEqual(both);
    const relinks: Warning = { HomebrewRelinksAfterUpdate: { name: "node@22", commands: ["node", "npm"] } };
    expect(JSON.stringify(relinks)).toBe(
      '{"HomebrewRelinksAfterUpdate":{"name":"node@22","commands":["node","npm"]}}',
    );
    expect(roundTrip(relinks)).toEqual(relinks);
    const taken: Outcome = { BanagerFailed: { LinkTaken: { name: "node@22", paths: ["/opt/homebrew/bin/npm"] } } };
    expect(JSON.stringify(taken)).toBe(
      '{"BanagerFailed":{"LinkTaken":{"name":"node@22","paths":["/opt/homebrew/bin/npm"]}}}',
    );
    expect(roundTrip(taken)).toEqual(taken);
    const notes: [LogNote, string][] = [
      [{ RelinkingAfterUpdate: { name: "node@22" } }, '{"RelinkingAfterUpdate":{"name":"node@22"}}'],
      [{ StillLinkedAfterUpdate: { name: "node@22" } }, '{"StillLinkedAfterUpdate":{"name":"node@22"}}'],
      [
        { NoLongerLinked: { name: "node@22", commands: ["node", "npm"] } },
        '{"NoLongerLinked":{"name":"node@22","commands":["node","npm"]}}',
      ],
    ];
    for (const [note, json] of notes) {
      const event: OperationEvent = { Note: { op_id: 7, note } };
      expect(JSON.stringify(event)).toBe(`{"Note":{"op_id":7,"note":${json}}}`);
      expect(roundTrip(event)).toEqual(event);
    }
    const blocked: UpdateBlocked = "LinkTaken";
    expect(JSON.stringify(blocked)).toBe('"LinkTaken"');
  });

  it("spells the unknown-source scan's shapes as Rust sends them", () => {
    // Mirrors `crates/banager-core/src/scan/mod.rs`, whose
    // `test_scan_wire_shapes_match_the_hand_written_ts_mirror` asserts
    // these exact spellings from the Rust side: `EntryKind` bare strings,
    // `ScanStop` externally tagged with the limit the scan enforced, and
    // an explicit `null` for a complete scan.
    const kinds: EntryKind[] = ["File", "Symlink", "BrokenSymlink", "ProtectedSymlink"];
    expect(JSON.stringify(kinds)).toBe('["File","Symlink","BrokenSymlink","ProtectedSymlink"]');
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

    // The folders left unread, in the place Rust puts them (after
    // `scanned`), and a link into a protected place with nothing of where
    // it leads.
    const guarded: UnknownScan = {
      scanned: [],
      protected_dirs: ["~/Documents/scripts"],
      entries: [
        {
          path: "~/.local/bin/notes-cli",
          kind: "ProtectedSymlink",
          resolved: null,
          link_target: "/Users/someone/Documents/notes-cli/bin/notes-cli",
          size_bytes: null,
          modified_at: null,
          owned_by_me: true,
          app_bundle: null,
        },
      ],
      attributed: 0,
      stopped: null,
    };
    expect(JSON.stringify(guarded)).toBe(
      '{"scanned":[],"protected_dirs":["~/Documents/scripts"],"entries":[{"path":"~/.local/bin/notes-cli","kind":"ProtectedSymlink","resolved":null,"link_target":"/Users/someone/Documents/notes-cli/bin/notes-cli","size_bytes":null,"modified_at":null,"owned_by_me":true,"app_bundle":null}],"attributed":0,"stopped":null}',
    );
    expect(roundTrip(guarded)).toEqual(guarded);
  });

  it("keeps the InventoryPreview event's wire shape intact", () => {
    // Byte for byte what `events.rs`'s
    // `test_inventory_preview_wire_shape_is_what_the_typescript_mirror_expects`
    // asserts serde emits: a newtype variant, a one-key object carrying
    // the preview, which has nothing about updates, errors or staleness.
    const empty: UiEvent = { InventoryPreview: { round: 1, instances: [], artifacts: [] } };
    expect(JSON.stringify(empty)).toBe('{"InventoryPreview":{"round":1,"instances":[],"artifacts":[]}}');

    const preview: InventoryPreview = {
      round: 3,
      instances: [
        {
          id: "brew:1",
          adapter_id: "brew",
          exe_path: "/opt/homebrew/bin/brew",
          prefix: "/opt/homebrew",
          scope: "User",
          version: "7.0.3",
          status: { unavailable: null, notes: [] },
          answered_at: null,
          unverified_version: null,
          read_only_reason: null,
        },
      ],
      artifacts: [
        {
          key: { instance_id: "brew:1", kind: "Formula", name: "jq" },
          display_name: "jq",
          version: "1.8.2",
          reason: "Requested",
          description: null,
          homepage: null,
          size_bytes: null,
          installed_at: null,
          path: null,
          auto_updates: false,
          uninstall_blocked: null,
          facts: NO_FACTS,
        },
      ],
    };
    const event: UiEvent = { InventoryPreview: preview };
    const parsed = roundTrip(event);
    expect("InventoryPreview" in parsed && parsed.InventoryPreview).toEqual(preview);
    expect(Object.keys(preview).sort()).toEqual(["artifacts", "instances", "round"]);
  });

  it("spells Sizes and SizesChanged as size.rs's and events.rs's shape tests do", () => {
    // `test_sizes_are_the_json_the_typescript_mirror_reads` in
    // crates/banager-core/src/size.rs asserts this exact string from the
    // Rust side.
    const sizes: Sizes = {
      round: 3,
      done: true,
      artifacts: [
        {
          key: { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "node@22" },
          version: "22.23.3",
          measured: { bytes: 312000000, partial: false, at_least: false },
          old_versions: { bytes: 1200000000, partial: true, at_least: false },
        },
      ],
      models: [{ instance_id: "ollama:http://127.0.0.1:11434", measured: null }],
      total: null,
      sources: [
        { instance_id: "brew:/opt/homebrew", measured: { bytes: 1512000000, partial: true, at_least: false } },
      ],
    };
    expect(JSON.stringify(sizes)).toBe(
      '{"round":3,"done":true,"artifacts":[{"key":{"instance_id":"brew:/opt/homebrew","kind":"Formula","name":"node@22"},"version":"22.23.3","measured":{"bytes":312000000,"partial":false,"at_least":false},"old_versions":{"bytes":1200000000,"partial":true,"at_least":false}}],"models":[{"instance_id":"ollama:http://127.0.0.1:11434","measured":null}],"total":null,"sources":[{"instance_id":"brew:/opt/homebrew","measured":{"bytes":1512000000,"partial":true,"at_least":false}}]}',
    );
    expect(roundTrip(sizes)).toEqual(sizes);
    expect(JSON.stringify(NO_SIZES)).toBe(
      '{"round":0,"done":false,"artifacts":[],"models":[],"total":null,"sources":[]}',
    );
    // `test_sizes_changed_reaches_the_window_as_an_object_with_its_round`
    // in src-tauri/src/events.rs: an object, so `in` can tell it apart.
    const changed: UiEvent = { SizesChanged: { round: 12 } };
    expect(JSON.stringify(changed)).toBe('{"SizesChanged":{"round":12}}');
  });

  it("spells SystemFacts' path_folders as diagnostics.rs's wire test does: null before a round, else read and unread", () => {
    // `test_the_wire_format_is_the_one_src_lib_types_ts_mirrors` in
    // crates/banager-core/src/diagnostics.rs asserts both from the Rust side.
    const none: SystemFacts = {
      macos_version: null,
      chip: null,
      arch: "",
      login_path: false,
      path_dirs: [],
      sources: [],
      path_folders: null,
    };
    expect(JSON.stringify(none)).toBe(
      '{"macos_version":null,"chip":null,"arch":"","login_path":false,"path_dirs":[],"sources":[],"path_folders":null}',
    );
    const read: SystemFacts = { ...none, path_folders: { read: 9, unread: ["~/Documents/bin"] } };
    expect(JSON.stringify(read).endsWith('"path_folders":{"read":9,"unread":["~/Documents/bin"]}}')).toBe(true);
    expect(roundTrip(read)).toEqual(read);
  });
});


it("round-trips the Traditional Chinese settings wire value", () => {
  const settings: Settings = {
    language: "ZhHant",
    show_technical_details: false,
    ignored_updates: [],
    skipped_versions: [],
    include_self_updating: false,
    auto_check: false,
    notify_updates: false,
  };
  expect(JSON.parse(JSON.stringify(settings)).language).toBe("ZhHant");
  expect(roundTrip(settings)).toEqual(settings);
});
