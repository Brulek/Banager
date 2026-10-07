import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
  ArtifactKey,
  HistoryView,
  InventoryPreview,
  IssuedPlan,
  OperationEvent,
  OpRequest,
  OpSummary,
  Settings,
  Sizes,
  Snapshot,
  SystemFacts,
  UiEvent,
  UnknownScan,
  UpdateCandidate,
} from "../lib/types";
import { NO_FACTS } from "../lib/types";
import { toolsNotJudged } from "../lib/commandsKnown";
import { outcomeCause, failureCause } from "../lib/failureCause";
import { resolveToolIcon } from "../lib/toolIcons";
import { everySourceChecked, hidingRule, updateStateOf } from "../lib/updateState";
import { artifactKeyId } from "../store/ui";
import { createMockBackend, MOCK_COMMANDS, TIMING, type MockBackend } from "./mockBackend";
import { MODELS, buildWorld } from "./mockData";
import { withMockNeededBy } from "./mockNeededBy";
import { getCurrentWindow as previewWindow } from "./mockTauriWindow";
import { DEFAULT_SCENARIO, parseScenario, type Scenario } from "./scenario";

const __dirname = path.dirname(fileURLToPath(import.meta.url));

/** Settles a call whose answer waits on the backend's timers. */
async function answer<T>(call: Promise<unknown>): Promise<T> {
  await vi.runOnlyPendingTimersAsync();
  return (await call) as T;
}

function backendFor(overrides: Partial<Scenario> = {}): { backend: MockBackend; events: UiEvent[] } {
  const backend = createMockBackend({ ...DEFAULT_SCENARIO, ...overrides });
  const events: UiEvent[] = [];
  void backend.invoke("subscribe_events", { channel: { onmessage: (e: UiEvent) => events.push(e) } });
  return { backend, events };
}

function upgradeOf(candidate: UpdateCandidate): OpRequest {
  return {
    kind: "Upgrade",
    instance_id: candidate.key.instance_id,
    artifact_kind: candidate.key.kind,
    name: candidate.key.name,
  };
}

function operationEvents(events: UiEvent[], opId: number): OperationEvent[] {
  return events.flatMap((e) => ("Operation" in e ? [e.Operation] : [])).filter((e) => {
    const payload = Object.values(e)[0] as { op_id: number };
    return payload.op_id === opId;
  });
}

function statusesOf(events: OperationEvent[]): string[] {
  return events.flatMap((e) => ("Status" in e ? [e.Status.status] : []));
}

/** Plans an upgrade of each of `names`, as the Updates page's batch does. */
async function planUpgrades(backend: MockBackend, ...names: string[]): Promise<IssuedPlan[]> {
  const snapshot = await answer<Snapshot>(backend.invoke("get_snapshot"));
  const calls = names.map((name) => {
    const candidate = snapshot.updates.find((u) => u.key.name === name);
    if (candidate === undefined) throw new Error(`no update for ${name}`);
    return backend.invoke("plan_operation", { request: upgradeOf(candidate) });
  });
  await vi.runOnlyPendingTimersAsync();
  return (await Promise.all(calls)) as IssuedPlan[];
}

/** Plans, then submits one after the other, as the Updates page's batch does. */
async function submitUpgrades(backend: MockBackend, ...names: string[]): Promise<number[]> {
  const ids: number[] = [];
  for (const issued of await planUpgrades(backend, ...names)) {
    ids.push((await backend.invoke("submit_operation", { planId: issued.id })) as number);
  }
  return ids;
}

/** Expects `call` to reject with `reason`, letting the backend's timers run. */
async function expectRefusal(call: Promise<unknown>, reason: string): Promise<void> {
  const assertion = expect(call).rejects.toBe(reason);
  await vi.runOnlyPendingTimersAsync();
  await assertion;
}

describe("the browser preview's mock backend", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("answers every command src/lib/api.ts sends, and nothing else", () => {
    const api = readFileSync(path.resolve(__dirname, "../lib/api.ts"), "utf-8");
    const sent = [...api.matchAll(/call<[^>]*>\("([a-z_]+)"/g)].map((m) => m[1]);
    expect(sent.length).toBeGreaterThan(0);
    expect([...sent].sort()).toEqual([...MOCK_COMMANDS].sort());
  });

  it("rejects an unknown command with a string, as Tauri does", async () => {
    const { backend } = backendFor();
    await expect(backend.invoke("no_such_command")).rejects.toBe("Command no_such_command not found");
  });

  it("takes the menu bar's language as the real command does: the window's two, and no other", async () => {
    const { backend } = backendFor();
    await expect(backend.invoke("set_menu_language", { language: "en" })).resolves.toBeUndefined();
    await expect(backend.invoke("set_menu_language", { language: "zh-CN" })).resolves.toBeUndefined();
    await expect(backend.invoke("set_menu_language", { language: "zh-Hant" })).resolves.toBeUndefined();
    await expect(backend.invoke("set_menu_language", { language: "fr" })).rejects.toMatch(/^invalid args/);
    await expect(backend.invoke("set_menu_language")).rejects.toMatch(/^invalid args/);
  });

  it("takes the update notification's report as the real command does, and grants its permission", async () => {
    // Nothing is posted in the preview: it has no daily check.
    const { backend } = backendFor();
    const updates = [{ key_id: "brew:/opt/homebrew|Formula|jq", target: "1.8.1" }];
    await expect(backend.invoke("report_update_set", { round: 1, updates })).resolves.toBeUndefined();
    await expect(backend.invoke("report_update_set", { updates })).rejects.toMatch(/^invalid args/);
    await expect(backend.invoke("report_update_set", { round: 1 })).rejects.toMatch(/^invalid args/);
    await expect(backend.invoke("request_notification_permission")).resolves.toBe(true);
  });

  it("takes the report of a finished run as the real command does, and posts nothing", async () => {
    const { backend } = backendFor();
    const run = { last_op: 3, kind: "Upgrade", succeeded: 2, failed: 1, attention: 0 };
    await expect(backend.invoke("report_finished_run", { run })).resolves.toBeUndefined();
    await expect(backend.invoke("report_finished_run", {})).rejects.toMatch(/^invalid args/);
    await expect(backend.invoke("report_finished_run", { run: { ...run, kind: "Install" } })).rejects.toMatch(
      /^invalid args/,
    );
  });

  it("shows nothing in Finder, only for a path the newest scan resolved, and says in the console what it was asked for", async () => {
    const info = vi.spyOn(console, "info").mockImplementation(() => {});
    try {
      const { backend } = backendFor();
      // Before any scan, as `reveal::Revealable` starts: nothing.
      await expect(backend.invoke("reveal_in_finder", { path: "/usr/bin/true" })).rejects.toBe(
        '{"kind":"not_revealable"}',
      );
      const scanned = backend.invoke("scan_unknown") as Promise<UnknownScan>;
      await vi.runAllTimersAsync();
      const scan = await scanned;
      const resolved = scan.entries.find((entry) => entry.resolved !== null)?.resolved;
      expect(resolved).toBeDefined();
      await expect(backend.invoke("reveal_in_finder", { path: resolved })).resolves.toBeUndefined();
      const said = String(info.mock.lastCall?.[0]);
      expect(said.startsWith("[banager-ui-preview-mock] ")).toBe(true);
      expect(said.endsWith(String(resolved))).toBe(true);
      await expect(backend.invoke("reveal_in_finder", { path: "/Users/someone/Documents" })).rejects.toBe(
        '{"kind":"not_revealable"}',
      );
    } finally {
      info.mockRestore();
    }
  });

  it("opens no browser, only for a homepage the committed snapshot lists, and says in the console what it was asked for", async () => {
    const info = vi.spyOn(console, "info").mockImplementation(() => {});
    try {
      const { backend } = backendFor();
      // Before the first refresh commits, as `homepage::listed_homepage`
      // over an empty snapshot: nothing.
      await expect(backend.invoke("open_homepage", { address: "https://iterm2.com/" })).rejects.toBe(
        '{"kind":"not_listed"}',
      );
      const snapshot = await answer<Snapshot>(backend.invoke("refresh"));
      const homepage = snapshot.artifacts.find((a) => a.key.name === "iterm2")?.homepage;
      expect(homepage).toBe("https://iterm2.com/");
      await expect(backend.invoke("open_homepage", { address: homepage })).resolves.toBeUndefined();
      const said = String(info.mock.lastCall?.[0]);
      expect(said.startsWith("[banager-ui-preview-mock] ")).toBe(true);
      expect(said.endsWith("https://iterm2.com/")).toBe(true);
      for (const address of ["https://example.com/?paths=%2FUsers", "https://iterm2.com", "file:///etc/hosts", ""]) {
        await expect(backend.invoke("open_homepage", { address })).rejects.toBe('{"kind":"not_listed"}');
      }
    } finally {
      info.mockRestore();
    }
  });

  it("takes the page's word on the question before a quit as the real commands do, and quits nothing", async () => {
    // The preview never hears the question (./mockTauriEvent.ts listens to
    // nothing), and a page cannot quit the browser. Like the real commands,
    // it takes whether the page asks, and a question's number, and Tauri
    // turns anything else away.
    const { backend } = backendFor();
    await expect(backend.invoke("ask_before_quit", { ask: true })).resolves.toBeUndefined();
    await expect(backend.invoke("ask_before_quit", { ask: false })).resolves.toBeUndefined();
    await expect(backend.invoke("ask_before_quit")).rejects.toMatch(/^invalid args/);
    await expect(backend.invoke("quit_question_shown", { question: 1 })).resolves.toBeUndefined();
    await expect(backend.invoke("quit_question_shown", { question: -1 })).rejects.toMatch(/^invalid args/);
    await expect(backend.invoke("quit_question_shown")).rejects.toMatch(/^invalid args/);
    await expect(backend.invoke("quit_kept_waiting", { question: 1 })).resolves.toBeUndefined();
    await expect(backend.invoke("quit_kept_waiting", { question: -1 })).rejects.toMatch(/^invalid args/);
    await expect(backend.invoke("quit_kept_waiting")).rejects.toMatch(/^invalid args/);
    await expect(backend.invoke("quit_anyway")).resolves.toBeUndefined();
  });

  it("starts empty, like a real launch, and the first refresh commits generation 1", async () => {
    const { backend, events } = backendFor();
    const before = await answer<Snapshot>(backend.invoke("get_snapshot"));
    expect(before).toMatchObject({ generation: 0, round: 0, detect: "Missing", refreshed_at: null });
    const first = await answer<Snapshot>(backend.invoke("refresh"));
    expect(first.generation).toBe(1);
    expect(first.round).toBe(1);
    expect(first.refreshed_at).not.toBeNull();
    // The daily check is due a day after this round, the window's.
    expect(before.next_auto_check_at).toBeNull();
    expect(first.next_auto_check_at).toBe((first.refreshed_at ?? 0) + 24 * 60 * 60);
    await vi.runOnlyPendingTimersAsync();
    expect(events).toContainEqual({ SnapshotChanged: { generation: 1 } });
    // An unchanged refresh keeps its generation and announces nothing new,
    // and is a round of its own (`Snapshot::round`).
    const again = await answer<Snapshot>(backend.invoke("refresh"));
    expect(again.generation).toBe(1);
    expect(again.round).toBe(2);
    expect(await answer<Snapshot>(backend.invoke("get_snapshot"))).toEqual(again);
  });

  it("measures sizes after each refresh: measuring first, every size a moment later, remembered after that", async () => {
    const { backend, events } = backendFor();
    expect(await answer<Sizes>(backend.invoke("get_sizes"))).toEqual({
      round: 0,
      done: false,
      artifacts: [],
      models: [],
      total: null,
      sources: [],
    });
    const snapshot = await answer<Snapshot>(backend.invoke("refresh"));
    const measuring = (await backend.invoke("get_sizes")) as Sizes;
    expect(measuring.round).toBe(snapshot.round);
    expect(measuring.done).toBe(false);
    expect(measuring.artifacts.length).toBeGreaterThan(0);
    expect(measuring.artifacts.every((size) => size.measured === null)).toBe(true);
    expect(measuring.total).toBeNull();
    await vi.advanceTimersByTimeAsync(2_000);
    const measured = (await backend.invoke("get_sizes")) as Sizes;
    expect(measured.done).toBe(true);
    expect(measured.artifacts.every((size) => size.measured !== null)).toBe(true);
    expect(events.filter((e) => "SizesChanged" in e)).toEqual([
      { SizesChanged: { round: 1 } },
      { SizesChanged: { round: 1 } },
    ]);
    // What size.rs never measures is never listed: pip's packages, a cask
    // with no app, Ollama's models (they keep their own size).
    const listed = new Set(measured.artifacts.map((size) => artifactKeyId(size.key)));
    for (const artifact of snapshot.artifacts) {
      const id = artifactKeyId(artifact.key);
      if (artifact.key.instance_id.startsWith("pip:") || artifact.key.kind === "Model") {
        expect(listed.has(id), id).toBe(false);
      }
    }
    expect(listed.has("brew:/opt/homebrew|Cask|font-jetbrains-mono")).toBe(false);
    expect(listed.has("brew:/opt/homebrew|Cask|iterm2")).toBe(true);
    // A formula with other kegs, one partial, one cut short, the models.
    const node = measured.artifacts.find((size) => size.key.name === "node@22");
    expect(node?.old_versions?.bytes).toBeGreaterThan(0);
    // The other kegs' size and the other versions Homebrew names are the
    // same kegs: a formula has both or neither (git, node@22 among them).
    const sizeOf = new Map(measured.artifacts.map((size) => [artifactKeyId(size.key), size]));
    for (const artifact of snapshot.artifacts) {
      const size = sizeOf.get(artifactKeyId(artifact.key));
      if (size === undefined) continue;
      const others = artifact.facts.homebrew?.other_versions.length ?? 0;
      expect(size.old_versions !== null, artifact.key.name).toBe(others > 0);
    }
    expect(sizeOf.get("brew:/opt/homebrew|Formula|git")?.old_versions).not.toBeNull();
    expect(measured.artifacts.some((size) => size.measured?.partial)).toBe(true);
    expect(measured.artifacts.some((size) => size.measured?.at_least)).toBe(true);
    expect(measured.models).toEqual([
      { instance_id: "ollama:http://127.0.0.1:11434", measured: expect.objectContaining({ partial: false }) },
    ]);
    const modelsOwn = snapshot.artifacts
      .filter((a) => a.key.kind === "Model")
      .reduce((sum, a) => sum + (a.size_bytes ?? 0), 0);
    expect(measured.models[0].measured!.bytes).toBeLessThanOrEqual(modelsOwn);
    // The next round shows what it measured before at once.
    const again = await answer<Snapshot>(backend.invoke("refresh"));
    const remembered = (await backend.invoke("get_sizes")) as Sizes;
    expect(remembered.round).toBe(again.round);
    expect(remembered.done).toBe(true);
  });

  it("never finishes measuring with ?sizes=pending", async () => {
    const { backend } = backendFor({ sizes: "pending" });
    await answer<Snapshot>(backend.invoke("refresh"));
    await vi.advanceTimersByTimeAsync(10_000);
    const sizes = (await backend.invoke("get_sizes")) as Sizes;
    expect(sizes.done).toBe(false);
    expect(sizes.artifacts.every((size) => size.measured === null)).toBe(true);
  });

  it("covers every Updates row state and every way of hiding one", async () => {
    const { backend } = backendFor();
    const snapshot = await answer<Snapshot>(backend.invoke("refresh"));
    const settings = await answer<Settings>(backend.invoke("get_settings"));
    const byId = new Map(snapshot.instances.map((i) => [i.id, i]));
    const hiddenBy = hidingRule(settings);
    const states = new Set(
      snapshot.updates.map((u) => {
        const state = updateStateOf(u, byId.get(u.key.instance_id));
        return state.kind === "blocked" ? `blocked:${state.reason}` : state.kind;
      }),
    );
    expect([...states].sort()).toEqual([
      "actionable",
      "blocked:Disabled",
      "blocked:LinkTaken",
      "blocked:Pinned",
      "blocked:SelfUpdatesOnly",
      "cannotCheck",
      "readOnly",
      "sourceUnavailable",
    ]);
    expect(new Set(snapshot.updates.map(hiddenBy))).toEqual(new Set([null, "ignored", "snoozed", "skipped"]));
    expect(snapshot.updates.some((u) => u.channel === "Digest")).toBe(true);
  });

  it("has two Homebrews with ?state=notices, the second an Intel Mac's in /usr/local that did not answer and lists nothing", async () => {
    const { backend } = backendFor({ state: "notices" });
    const snapshot = await answer<Snapshot>(backend.invoke("refresh"));
    const brews = snapshot.instances.filter((i) => i.adapter_id === "brew");
    expect(brews.map((i) => [i.id, i.prefix, i.status.unavailable])).toEqual([
      ["brew:/opt/homebrew", "/opt/homebrew", null],
      ["brew:/usr/local", "/usr/local", "NotResponding"],
    ]);
    expect(snapshot.artifacts.filter((a) => a.key.instance_id === "brew:/usr/local")).toEqual([]);
  });

  it("says when uv last answered with ?state=notices, and no time for the Intel Homebrew it never heard (R12)", async () => {
    const before = Math.floor(Date.now() / 1000);
    const snapshot = await answer<Snapshot>(backendFor({ state: "notices" }).backend.invoke("refresh"));
    const answeredAt = (id: string) => snapshot.instances.find((i) => i.id === id)?.answered_at;
    // Earlier this session, 47 minutes before the mock started.
    expect(before - answeredAt("uv")!).toBeGreaterThanOrEqual(47 * 60);
    expect(before - answeredAt("uv")!).toBeLessThan(48 * 60);
    expect(answeredAt("brew:/usr/local")).toBeNull();
    // Without ?state=notices, no source says a time.
    const plain = await answer<Snapshot>(backendFor({}).backend.invoke("refresh"));
    expect(plain.instances.every((i) => i.answered_at === null)).toBe(true);
  });

  it("has an Ollama at an https address and a Python with no pip with ?state=refused, neither listing anything", async () => {
    const { backend } = backendFor({ state: "refused" });
    const snapshot = await answer<Snapshot>(backend.invoke("refresh"));
    const unasked = snapshot.instances.filter((i) => i.status.unavailable !== null && i.adapter_id !== "uv");
    expect(unasked.map((i) => [i.id, i.status.unavailable])).toEqual([
      ["ollama:https://ollama.home.lan", "HttpsHostRefused"],
      ["pip:/opt/local/bin/python3.13", "NoPip"],
    ]);
    for (const { id } of unasked) {
      expect(snapshot.artifacts.filter((a) => a.key.instance_id === id)).toEqual([]);
      expect(snapshot.updates.filter((u) => u.key.instance_id === id)).toEqual([]);
    }
  });

  it("has a lookup that met a certificate rustls would not accept and one a redirect refused with ?state=refused, neither to check again", async () => {
    const { backend } = backendFor({ state: "refused" });
    const snapshot = await answer<Snapshot>(backend.invoke("refresh"));
    const tokei = snapshot.updates.find((u) => u.key.name === "tokei");
    expect(tokei?.checkable).toBe(false);
    expect(tokei?.warnings.slice(1)).toEqual([{ SecureConnectionFailed: { host: "crates.io" } }]);
    const rustup = snapshot.updates.find((u) => u.key.name === "rustup");
    expect(rustup?.checkable).toBe(false);
    expect(rustup?.warnings).toHaveLength(1);
    expect(snapshot.updates.filter((u) => u.warnings.includes("TransientLookupFailure"))).toEqual([]);
  });

  it("has npm unable to start for want of node with ?state=nonode, and a link that puts it back", async () => {
    // The author's Mac on 2026-10-07 (finding 1): npm says why, and offers
    // node@22 and node@20, newest first; node@22's link has npm's own npm
    // and npx in its way, node@20's does not.
    const { backend, events } = backendFor({ state: "nonode" });
    const before = await answer<Snapshot>(backend.invoke("refresh"));
    const npm = before.instances.find((i) => i.adapter_id === "npm");
    expect(npm?.status.unavailable).toBe("NotResponding");
    expect(npm?.status.no_answer?.kind).toBe("CouldNotStart");
    expect(npm?.status.no_answer?.missing_program).toBe("node");
    expect(npm?.status.no_answer?.link_fixes.map((fix) => fix.key.name)).toEqual(["node@22", "node@20"]);
    expect(before.artifacts.filter((a) => a.key.name.startsWith("node@")).map((a) => a.version)).toEqual([
      "22.23.3_1",
      "20.19.5",
    ]);
    const link = (name: string): OpRequest => ({
      kind: "Link",
      instance_id: "brew:/opt/homebrew",
      artifact_kind: "Formula",
      name,
    });
    const blocked = await answer<IssuedPlan>(backend.invoke("plan_operation", { request: link("node@22") }));
    expect(blocked.plan.warnings).toEqual([
      { LinkConflicts: { paths: ["/opt/homebrew/bin/npm", "/opt/homebrew/bin/npx"] } },
    ]);
    // Only a formula a source's reason offers.
    const refused = backend.invoke("plan_operation", { request: link("jq") }).catch((error: unknown) => error);
    await vi.runOnlyPendingTimersAsync();
    expect(await refused).toBe('{"kind":"not_listed"}');
    const issued = await answer<IssuedPlan>(backend.invoke("plan_operation", { request: link("node@20") }));
    expect(issued.plan.action).toEqual({
      Command: {
        program: "/opt/homebrew/bin/brew",
        args: ["link", "--force", "node@20"],
        env: expect.any(Array) as unknown as [string, string][],
      },
    });
    expect(issued.plan.warnings).toEqual([]);
    const opId = await answer<number>(backend.invoke("submit_operation", { planId: issued.id }));
    await vi.runAllTimersAsync();
    const own = operationEvents(events, opId);
    expect(own[own.length - 1]).toEqual({ Finished: { op_id: opId, outcome: "Succeeded" } });
    const after = await answer<Snapshot>(backend.invoke("refresh"));
    const answered = after.instances.find((i) => i.adapter_id === "npm");
    expect(answered?.status).toEqual({ unavailable: null, notes: [], no_answer: null });
    expect(answered?.version).toBe("12.0.2");
  });

  it("installs about 800 real tools with ?state=many, one in seven with an update, the same on every run", async () => {
    const { backend } = backendFor({ state: "many" });
    const snapshot = await answer<Snapshot>(backend.invoke("refresh"));
    const again = await answer<Snapshot>(backendFor({ state: "many" }).backend.invoke("refresh"));
    expect({ ...again, refreshed_at: null, next_auto_check_at: null }).toEqual({
      ...snapshot,
      refreshed_at: null,
      next_auto_check_at: null,
    });

    const { artifacts, updates, instances } = snapshot;
    // Two of them the rows `withHomebrewState` adds, four the AI tools `aiTools` adds, one Codex's own install
    // (`codexStandalone`), one npm's Claude Code (`addMany`), whose uninstall preview names what stays
    // (./mockKeptData.ts), and five pip packages of two Pythons (`secondPython`).
    expect(artifacts.length).toBe(805);
    const ids = artifacts.map((a) => artifactKeyId(a.key));
    expect(new Set(ids).size).toBe(ids.length);
    const count = (instanceId: string) => artifacts.filter((a) => a.key.instance_id === instanceId).length;
    expect(count("brew:/opt/homebrew")).toBeGreaterThan(600);
    expect(artifacts.filter((a) => a.reason === "Dependency" && a.key.instance_id === "brew:/opt/homebrew")).toHaveLength(54);
    for (const id of ["npm:/opt/homebrew", "pipx", "uv", `cargo:/Users/you/.cargo`, "ollama:http://127.0.0.1:11434"]) {
      expect(count(id)).toBeGreaterThan(5);
    }
    // Every update is of a tool on the list, from a source that answered;
    // the list is the Updates page's, of which Banager can install ~120.
    expect(updates.every((u) => ids.includes(artifactKeyId(u.key)))).toBe(true);
    expect(instances.every((i) => i.status.unavailable === null && i.status.notes.length === 0)).toBe(true);
    const settings = await answer<Settings>(backend.invoke("get_settings"));
    const byId = new Map(instances.map((i) => [i.id, i]));
    const hiddenBy = hidingRule(settings);
    const actionable = updates.filter(
      (u) => hiddenBy(u) === null && updateStateOf(u, byId.get(u.key.instance_id)).kind === "actionable",
    );
    expect(actionable.length).toBeGreaterThanOrEqual(100);
    expect(actionable.length).toBeLessThanOrEqual(140);
    expect(updates.every((u) => !u.checkable || u.channel === "Digest" || u.target !== u.current)).toBe(true);

    // Real names: the logo pack has a logo for nearly every one.
    const withLogo = artifacts.filter((a) => {
      const instance = byId.get(a.key.instance_id);
      return instance !== undefined && resolveToolIcon(a.key, instance.adapter_id) !== null;
    });
    expect(withLogo.length / artifacts.length).toBeGreaterThan(0.9);
  });

  it("installs about 5,000 tools with ?state=huge: many's, and more made from their names, the same on every run", async () => {
    const snapshot = await answer<Snapshot>(backendFor({ state: "huge" }).backend.invoke("refresh"));
    const again = await answer<Snapshot>(backendFor({ state: "huge" }).backend.invoke("refresh"));
    expect(again.artifacts).toEqual(snapshot.artifacts);
    expect(again.updates).toEqual(snapshot.updates);
    const many = await answer<Snapshot>(backendFor({ state: "many" }).backend.invoke("refresh"));

    const { artifacts, updates } = snapshot;
    const ids = artifacts.map((a) => artifactKeyId(a.key));
    expect(new Set(ids).size).toBe(ids.length);
    // Every row of ?state=many is here as it is there.
    const byId = new Map(artifacts.map((a) => [artifactKeyId(a.key), a]));
    for (const row of many.artifacts) expect(byId.get(artifactKeyId(row.key))).toEqual(row);
    const count = (instanceId: string) => artifacts.filter((a) => a.key.instance_id === instanceId).length;
    expect(artifacts.length).toBe(4897);
    expect(count("brew:/opt/homebrew")).toBe(3893);
    expect(artifacts.filter((a) => a.key.kind === "Cask")).toHaveLength(335);
    for (const id of ["npm:/opt/homebrew", "pipx", "uv", "cargo:/Users/you/.cargo"]) expect(count(id)).toBeGreaterThan(100);
    // About one in seven has an update, as with ?state=many.
    expect(updates.length / artifacts.length).toBeGreaterThan(0.12);
    expect(updates.length / artifacts.length).toBeLessThan(0.18);
    // The new rows put a command on the Mac; about one in forty is not found.
    const before = new Set(many.artifacts.map((m) => artifactKeyId(m.key)));
    const added = artifacts.filter((a) => !before.has(artifactKeyId(a.key)));
    const notFound = added.filter((a) =>
      a.facts.commands.some((c) => typeof c.state === "object" && c.state !== null && "NotOnPath" in c.state),
    );
    expect(notFound.length).toBeGreaterThan(50);
    expect(added.filter((a) => a.reason === "Requested").every((a) => a.facts.commands.length === 1)).toBe(true);
  });

  it("runs an upgrade the way the real backend reports one, and the next refresh shows it done", async () => {
    const { backend, events } = backendFor();
    await answer(backend.invoke("refresh"));
    const [opId] = await submitUpgrades(backend, "git");

    await vi.advanceTimersByTimeAsync(1_000);
    const running = (await backend.invoke("list_operations")) as OpSummary[];
    expect(running[0]).toMatchObject({ id: opId, name: "git", status: "Running", outcome: null });
    expect(running[0].argv_preview).toEqual(["/opt/homebrew/bin/brew", "upgrade", "--formula", "git"]);

    await vi.runAllTimersAsync();
    const own = operationEvents(events, opId);
    expect(statusesOf(own)).toEqual(["Queued", "Running", "Verifying"]);
    expect(own.filter((e) => "Log" in e).length).toBeGreaterThan(2);
    expect(own[own.length - 1]).toEqual({ Finished: { op_id: opId, outcome: "Succeeded" } });
    const done = (await backend.invoke("list_operations")) as OpSummary[];
    expect(done[0]).toMatchObject({ id: opId, status: "Done", outcome: "Succeeded" });

    const after = await answer<Snapshot>(backend.invoke("refresh"));
    expect(after.generation).toBe(2);
    expect(after.updates.some((u) => u.key.name === "git")).toBe(false);
    expect(after.artifacts.find((a) => a.key.name === "git")?.version).toBe("2.55.1");
  });

  it("under ?outcome=password, stops a cask upgrade where sudo wanted the Mac's password, the command and its variables kept", async () => {
    const { backend, events } = backendFor({ outcome: "password" });
    await answer(backend.invoke("refresh"));
    const [opId] = await submitUpgrades(backend, "android-platform-tools");
    await vi.runAllTimersAsync();
    const own = operationEvents(events, opId);
    const finished = own[own.length - 1];
    expect(finished).toMatchObject({ Finished: { op_id: opId, outcome: { Failed: { exit_code: 1 } } } });
    const [done] = (await backend.invoke("list_operations")) as OpSummary[];
    expect(outcomeCause(done.outcome)).toBe("needsPassword");
    expect(done.argv_preview).toEqual(["/opt/homebrew/bin/brew", "upgrade", "--cask", "android-platform-tools"]);
    expect(done.env_preview).toContainEqual(["HOMEBREW_NO_AUTOREMOVE", "1"]);
    // Nothing changed: the update is still there.
    const after = await answer<Snapshot>(backend.invoke("refresh"));
    expect(after.updates.some((u) => u.key.name === "android-platform-tools")).toBe(true);
  });

  it("under ?outcome=already, says how each update already at its new version was done, and keeps it (r6 y3-batch)", async () => {
    // As the core tells them apart (review of r6 y3-batch, findings 1 and
    // 7): on Homebrew the first update of the batch updates for real and
    // brings the later ones along, which are then 「已由前面的更新一并完成」;
    // on another source, where one update never updates another package,
    // one already new was so before its turn, and says no Homebrew words; a
    // model, whose digests are never compared, updates as ever.
    const { backend, events } = backendFor({ outcome: "already" });
    await answer(backend.invoke("refresh"));
    const [git, wget, typescript, coder] = await submitUpgrades(backend, "git", "wget", "typescript", MODELS.coder);
    await vi.runAllTimersAsync();
    const ops = (await backend.invoke("list_operations")) as OpSummary[];
    const of = (id: number) => ops.find((op) => op.id === id);
    const logOf = (id: number) =>
      events.flatMap((event) =>
        "Operation" in event && "Log" in event.Operation && event.Operation.Log.op_id === id ? [event.Operation.Log.line] : [],
      );
    expect(of(git)).toMatchObject({ outcome: "Succeeded" });
    expect(of(git)?.already_updated ?? null).toBeNull();
    expect(of(wget)).toMatchObject({ outcome: "Succeeded", already_updated: "ByEarlierUpdate" });
    // Homebrew's own words, then the clean-up that follows any update (U9).
    expect(logOf(wget)[0]).toBe("Warning: wget 1.26.0 already installed");
    expect(of(typescript)).toMatchObject({ outcome: "Succeeded", already_updated: "BeforeItsTurn" });
    expect(logOf(typescript).join("\n")).not.toMatch(/already installed|Warning:/);
    expect(of(coder)).toMatchObject({ outcome: "Succeeded" });
    expect(of(coder)?.already_updated ?? null).toBeNull();
    const history = await answer<HistoryView>(backend.invoke("get_history"));
    const kept = history.records.find((record) => record.op_id === wget);
    expect(kept).toMatchObject({ result: "Succeeded", already_updated: "ByEarlierUpdate", verified: false });
  });

  it("under ?outcome=already, says a Homebrew update confirmed after the last that changed something ended was new before its turn", async () => {
    // Two single updates, the second confirmed after the first ended: the
    // first moved its version, but before the second was confirmed, so it
    // did not bring the second along.
    const { backend } = backendFor({ outcome: "already" });
    await answer(backend.invoke("refresh"));
    const [git] = await submitUpgrades(backend, "git");
    await vi.runAllTimersAsync();
    await answer(backend.invoke("refresh"));
    const [wget] = await submitUpgrades(backend, "wget");
    await vi.runAllTimersAsync();
    const ops = (await backend.invoke("list_operations")) as OpSummary[];
    expect(ops.find((op) => op.id === git)?.already_updated ?? null).toBeNull();
    expect(ops.find((op) => op.id === wget)).toMatchObject({ outcome: "Succeeded", already_updated: "BeforeItsTurn" });
  });

  it("under ?outcome=failed, keeps a failure no cause names with its first error line (r6 y3-batch)", async () => {
    const { backend } = backendFor({ outcome: "failed" });
    await answer(backend.invoke("refresh"));
    const [tokei] = await submitUpgrades(backend, "tokei");
    await vi.runAllTimersAsync();
    const history = await answer<HistoryView>(backend.invoke("get_history"));
    expect(history.records.find((record) => record.op_id === tokei)?.result).toEqual({
      Failed: { cause: null, detail: "failed to compile `tokei v13.0.1`" },
    });
  });

  it("runs operations that share a source one after the other", async () => {
    const { backend, events } = backendFor();
    await answer(backend.invoke("refresh"));
    const [git, wget] = await submitUpgrades(backend, "git", "wget");
    await vi.advanceTimersByTimeAsync(1_000);
    const ops = (await backend.invoke("list_operations")) as OpSummary[];
    expect(ops.map((o) => [o.id, o.status])).toEqual([
      [wget, "Queued"],
      [git, "Running"],
    ]);
    await vi.runAllTimersAsync();
    const finished = events.flatMap((e) =>
      "Operation" in e && "Finished" in e.Operation ? [e.Operation.Finished.op_id] : [],
    );
    expect(finished).toEqual([git, wget]);
  });

  /** Plans and submits the uninstall of each Homebrew formula named, in order, as a batch does. */
  async function submitUninstalls(backend: MockBackend, ...names: string[]): Promise<number[]> {
    const snapshot = await answer<Snapshot>(backend.invoke("get_snapshot"));
    const calls = names.map((name) => {
      const artifact = snapshot.artifacts.find((a) => a.key.instance_id === "brew:/opt/homebrew" && a.key.name === name);
      if (artifact === undefined) throw new Error(`no formula ${name}`);
      return backend.invoke("plan_operation", {
        request: { kind: "Uninstall", instance_id: artifact.key.instance_id, artifact_kind: artifact.key.kind, name },
      });
    });
    await vi.runOnlyPendingTimersAsync();
    const ids: number[] = [];
    for (const issued of (await Promise.all(calls)) as IssuedPlan[]) {
      ids.push((await backend.invoke("submit_operation", { planId: issued.id })) as number);
    }
    return ids;
  }

  it("uninstalls a dependent and then what it needed, in the order submitted, and Homebrew refuses the other order", async () => {
    // Every operation would succeed, but Homebrew refuses x264 while
    // ffmpeg, which needs it, is installed. (Not pipx and python@3.13:
    // their previews name the sources that run on them, and never run.)
    const refusedFirst = backendFor();
    await answer(refusedFirst.backend.invoke("refresh"));
    const [x264, ffmpeg] = await submitUninstalls(refusedFirst.backend, "x264", "ffmpeg");
    await vi.runAllTimersAsync();
    const ops = (await refusedFirst.backend.invoke("list_operations")) as OpSummary[];
    const outcomeOf = (id: number) => ops.find((o) => o.id === id)?.outcome;
    const refusal = [
      "Error: Refusing to uninstall /opt/homebrew/Cellar/x264/r3222",
      "because it is required by ffmpeg, which is currently installed.",
      "You can override this and force removal with:",
      "  brew uninstall --ignore-dependencies x264",
    ];
    expect(outcomeOf(x264)).toEqual({ Failed: { exit_code: 1, summary: refusal.join("\n"), cause: failureCause(refusal.join("\n")) } });
    expect(outcomeOf(ffmpeg)).toBe("Succeeded");
    const logged = operationEvents(refusedFirst.events, x264).flatMap((e) => ("Log" in e ? [e.Log.line] : []));
    expect(logged).toEqual(refusal);

    // The order a batch submits them in: ffmpeg first, then x264.
    const failing = backendFor({ outcome: "failed" });
    await answer(failing.backend.invoke("refresh"));
    // Under ?outcome=failed too, while ffmpeg is installed.
    const [first] = await submitUninstalls(failing.backend, "x264");
    await vi.runAllTimersAsync();
    const failed = ((await failing.backend.invoke("list_operations")) as OpSummary[]).find((o) => o.id === first);
    expect(failed?.outcome).toEqual({ Failed: { exit_code: 1, summary: refusal.join("\n"), cause: failureCause(refusal.join("\n")) } });

    const batch = backendFor();
    await answer(batch.backend.invoke("refresh"));
    const [ffmpegOp, x264Op] = await submitUninstalls(batch.backend, "ffmpeg", "x264");
    await vi.runAllTimersAsync();
    const done = (await batch.backend.invoke("list_operations")) as OpSummary[];
    expect(done.find((o) => o.id === ffmpegOp)?.outcome).toBe("Succeeded");
    expect(done.find((o) => o.id === x264Op)?.outcome).toBe("Succeeded");
    const finished = batch.events.flatMap((e) =>
      "Operation" in e && "Finished" in e.Operation ? [e.Operation.Finished.op_id] : [],
    );
    expect(finished).toEqual([ffmpegOp, x264Op]);
  });

  it("names the sources that run on a Homebrew package in its preview, and never runs that preview", async () => {
    // `Session::issue_plan` and `submit` (crates/banager-core/src/session/
    // needed_by.rs), as ./mockNeededBy.ts stands in for them.
    const uninstall = (name: string): OpRequest => ({
      kind: "Uninstall",
      instance_id: "brew:/opt/homebrew",
      artifact_kind: "Formula",
      name,
    });
    const needed = (issued: IssuedPlan) =>
      issued.plan.warnings.flatMap((w) => (typeof w !== "string" && "NeededBySource" in w ? [w.NeededBySource] : []));
    const { backend } = backendFor();
    const snapshot = await answer<Snapshot>(backend.invoke("refresh"));
    const rowsOf = (instanceId: string) => snapshot.artifacts.filter((a) => a.key.instance_id === instanceId);

    const node = await answer<IssuedPlan>(backend.invoke("plan_operation", { request: uninstall("node@22") }));
    // npm's own `npm` and `corepack` are not counted among its tools.
    const npmTools = rowsOf("npm:/opt/homebrew").filter((a) => !["npm", "corepack"].includes(a.key.name));
    expect(npmTools.length).toBeGreaterThan(0);
    expect(needed(node)).toEqual([{ instance_id: "npm:/opt/homebrew", program: true, tools: npmTools.length }]);
    await expectRefusal(
      backend.invoke("submit_operation", { planId: node.id }),
      '{"kind":"uninstall_blocked","reason":"NeededBySource"}',
    );
    // pip runs in python@3.13 (only what the user installed counts), and
    // every pipx venv's Python is its.
    const python = await answer<IssuedPlan>(backend.invoke("plan_operation", { request: uninstall("python@3.13") }));
    const pipLeaves = rowsOf("pip:/opt/homebrew/bin/python3").filter(
      (a) => a.reason !== "Dependency" && !["pip", "setuptools", "wheel"].includes(a.key.name),
    );
    expect(needed(python)).toEqual([
      { instance_id: "pip:/opt/homebrew/bin/python3", program: true, tools: pipLeaves.length },
      { instance_id: "pipx", program: false, tools: rowsOf("pipx").filter((a) => a.path !== null).length },
    ]);
    // Nothing runs on git.
    const git = await answer<IssuedPlan>(backend.invoke("plan_operation", { request: uninstall("git") }));
    expect(needed(git)).toEqual([]);
    // npm's own row offers no Uninstall, and the gate says why.
    const npmItself = rowsOf("npm:/opt/homebrew").find((a) => a.key.name === "npm");
    expect(npmItself?.uninstall_blocked).toBe("SourceProgram");
    await expectRefusal(
      backend.invoke("plan_operation", {
        request: { kind: "Uninstall", instance_id: "npm:/opt/homebrew", artifact_kind: "Package", name: "npm" },
      }),
      '{"kind":"uninstall_blocked","reason":"SourceProgram"}',
    );

    // With the npm from nodejs.org (`?state=notices`, whose Homebrew is
    // updating its list, so nothing of it is planned there), node@22 is
    // nobody's.
    const bare = { ...node.plan, warnings: [] };
    expect(withMockNeededBy(bare, buildWorld("notices"), uninstall("node@22")).warnings).toEqual([]);
    expect(withMockNeededBy(bare, buildWorld("full"), uninstall("node@22")).warnings).toEqual(node.plan.warnings.filter(
      (w) => typeof w !== "string" && "NeededBySource" in w,
    ));
  });

  it("offers Uninstall on Codex's own install and previews the two links and the package folder it moves (U8)", async () => {
    // `recipes::CODEX`'s path list: the helper link first, the launcher
    // last, and ~/.codex itself, with the settings, login and sessions, kept.
    const { backend } = backendFor();
    const snapshot = await answer<Snapshot>(backend.invoke("refresh"));
    const codex = snapshot.artifacts.find((a) => a.key.instance_id === "standalone-codex");
    expect(codex?.uninstall_blocked).toBeNull();
    const issued = await answer<IssuedPlan>(
      backend.invoke("plan_operation", {
        request: { kind: "Uninstall", instance_id: "standalone-codex", artifact_kind: "Binary", name: "codex" },
      }),
    );
    expect(issued.plan.action).toEqual({
      TrashPaths: {
        paths: [
          "/Users/you/.local/bin/codex-code-mode-host",
          "/Users/you/.codex/packages/standalone",
          "/Users/you/.local/bin/codex",
        ],
      },
    });
    expect(issued.plan.warnings).toEqual([
      { WillTrash: { path: "~/.local/bin/codex-code-mode-host", what: "Program" } },
      { WillTrash: { path: "~/.codex/packages/standalone", what: "Program" } },
      { WillTrash: { path: "~/.local/bin/codex", what: "Launcher" } },
      { WillKeep: { path: "~/.codex", what: "SettingsAndHistory" } },
      { WillKeep: { path: "~/.zprofile", what: "ShellConfigLines" } },
    ]);
    expect(issued.plan.timeout_secs).toBe(120);
  });

  it("keeps listing what a source had while an operation holds it, as the real refresh carries its rows forward", async () => {
    const { backend } = backendFor();
    await answer(backend.invoke("refresh"));
    const [jq, ripgrep] = await submitUninstalls(backend, "jq", "ripgrep");
    // jq done, ripgrep running on the same Homebrew: jq is still listed.
    await vi.advanceTimersByTimeAsync(TIMING.start + TIMING.run + TIMING.verify + 400);
    const ops = (await backend.invoke("list_operations")) as OpSummary[];
    expect(ops.find((o) => o.id === jq)?.outcome).toBe("Succeeded");
    expect(ops.find((o) => o.id === ripgrep)?.status).not.toBe("Done");
    const during = await answer<Snapshot>(backend.invoke("refresh"));
    const listed = (snapshot: Snapshot) =>
      snapshot.artifacts.filter((a) => a.key.instance_id === "brew:/opt/homebrew").map((a) => a.key.name);
    expect(listed(during)).toContain("jq");
    expect(listed(during)).toContain("ripgrep");
    // Its last operation done: read again, and both are gone.
    await vi.runAllTimersAsync();
    const after = await answer<Snapshot>(backend.invoke("refresh"));
    expect(listed(after)).not.toContain("jq");
    expect(listed(after)).not.toContain("ripgrep");
  });

  it("under ?outcome=mixed, fails every second operation of the session and runs the rest", async () => {
    const { backend } = backendFor({ outcome: "mixed" });
    await answer(backend.invoke("refresh"));
    const ids = await submitUninstalls(backend, "jq", "ripgrep", "gh", "git");
    await vi.runAllTimersAsync();
    const ops = (await backend.invoke("list_operations")) as OpSummary[];
    const tones = ids.map((id) => {
      const outcome = ops.find((o) => o.id === id)?.outcome;
      return outcome === "Succeeded" ? "ok" : typeof outcome === "object" && outcome !== null && "Failed" in outcome ? "failed" : String(outcome);
    });
    expect(ids).toEqual([1, 2, 3, 4]);
    expect(tones).toEqual(["ok", "failed", "ok", "failed"]);
    const snapshot = await answer<Snapshot>(backend.invoke("refresh"));
    const names = snapshot.artifacts.filter((a) => a.key.instance_id === "brew:/opt/homebrew").map((a) => a.key.name);
    expect(names).not.toContain("jq");
    expect(names).toContain("ripgrep");
    expect(names).not.toContain("gh");
    expect(names).toContain("git");
  });

  it("cancels a running operation and leaves the machine as it was", async () => {
    const { backend, events } = backendFor();
    await answer(backend.invoke("refresh"));
    const [opId] = await submitUpgrades(backend, "wget");
    await vi.advanceTimersByTimeAsync(1_000);
    await backend.invoke("cancel_operation", { opId });
    await vi.runAllTimersAsync();
    const own = operationEvents(events, opId);
    expect(statusesOf(own)).toEqual(["Queued", "Running", "CancelRequested", "Cancelling", "Verifying"]);
    // A stopped upgrade proves nothing either way.
    expect(own[own.length - 1]).toEqual({ Finished: { op_id: opId, outcome: "Unconfirmed" } });
    const after = await answer<Snapshot>(backend.invoke("refresh"));
    expect(after.updates.some((u) => u.key.name === "wget")).toBe(true);
  });

  it("refuses to cancel rustup's self update once it runs", async () => {
    const { backend } = backendFor();
    await answer(backend.invoke("refresh"));
    const [opId] = await submitUpgrades(backend, "rustup");
    await vi.advanceTimersByTimeAsync(1_000);
    await expectRefusal(backend.invoke("cancel_operation", { opId }), '{"kind":"no_cancel"}');
  });

  it("refuses what the real gate refuses", async () => {
    const { backend } = backendFor();
    const snapshot = await answer<Snapshot>(backend.invoke("refresh"));
    const plan = (name: string) => {
      const candidate = snapshot.updates.find((u) => u.key.name === name);
      if (candidate === undefined) throw new Error(`no update for ${name}`);
      return backend.invoke("plan_operation", { request: upgradeOf(candidate) });
    };
    await expectRefusal(
      plan("requests"),
      '{"kind":"not_actionable","read_only":"ByDesign","unavailable":null}',
    );
    await expectRefusal(plan("ruff"), '{"kind":"not_actionable","read_only":null,"unavailable":"NotResponding"}');
    await expectRefusal(plan("postgresql@17"), '{"kind":"update_blocked","reason":"Pinned"}');
    await expectRefusal(plan("agy"), '{"kind":"update_blocked","reason":"SelfUpdatesOnly"}');
    // An install is refused before the gate, as `plan_operation_impl`
    // refuses it: `refused` even for pip, which the gate would call read-only.
    const requests = snapshot.updates.find((u) => u.key.name === "requests");
    if (requests === undefined) throw new Error("no update for requests");
    await expectRefusal(
      backend.invoke("plan_operation", { request: { ...upgradeOf(requests), kind: "Install" } }),
      '{"kind":"refused"}',
    );
    await expectRefusal(
      backend.invoke("submit_operation", { planId: "0".repeat(32) }),
      '{"kind":"unknown"}',
    );
  });

  it("keeps settings, and lists the self-updating app once asked to", async () => {
    const { backend } = backendFor();
    const settings = await answer<Settings>(backend.invoke("get_settings"));
    await backend.invoke("set_settings", { settings: { ...settings, include_self_updating: true } });
    expect(await backend.invoke("get_settings")).toEqual({ ...settings, include_self_updating: true });
    const snapshot = await answer<Snapshot>(backend.invoke("refresh"));
    expect(snapshot.updates.some((u) => u.key.name === "visual-studio-code")).toBe(true);
  });

  it("shows the states the URL switches name", async () => {
    const empty = backendFor({ state: "empty" }).backend;
    expect(await answer<Snapshot>(empty.invoke("refresh"))).toMatchObject({
      generation: 0,
      detect: "Missing",
    });

    const loading = backendFor({ state: "loading" }).backend;
    let settled = false;
    void loading.invoke("refresh").then(() => {
      settled = true;
    });
    await vi.advanceTimersByTimeAsync(60_000);
    expect(settled).toBe(false);

    const error = backendFor({ state: "error" }).backend;
    const refusal = expect(error.invoke("get_snapshot")).rejects.toEqual(expect.any(String));
    await vi.runOnlyPendingTimersAsync();
    await refusal;
  });

  it("draws an icon for a cask's app from the committed snapshot, and for nothing else", async () => {
    const { backend } = backendFor();
    const iterm: ArtifactKey = { instance_id: "brew:/opt/homebrew", kind: "Cask", name: "iterm2" };
    // Before the first refresh the snapshot has no rows to draw for.
    expect(await answer(backend.invoke("artifact_icon", { key: iterm }))).toBeNull();

    const snapshot = await answer<Snapshot>(backend.invoke("refresh"));
    const drawn: string[] = [];
    for (const row of snapshot.artifacts) {
      const icon = await answer<string | null>(backend.invoke("artifact_icon", { key: row.key }));
      if (icon !== null) drawn.push(`${row.key.kind} ${row.key.name}`);
    }
    // The two casks with an app; not the font, not the one with no app.
    expect(drawn.sort()).toEqual(["Cask iterm2", "Cask visual-studio-code"]);

    const icon = await answer<string>(backend.invoke("artifact_icon", { key: iterm }));
    expect(icon.startsWith("data:image/svg+xml;charset=utf-8,")).toBe(true);
    const svg = decodeURIComponent(icon.slice(icon.indexOf(",") + 1));
    expect(svg.startsWith('<svg xmlns="http://www.w3.org/2000/svg"')).toBe(true);
    expect(svg).toContain(">I</text>");
    // A key that names a path is a key with no row: nothing is drawn.
    for (const name of ["/Applications/iTerm.app", "iTerm.app"]) {
      expect(await answer(backend.invoke("artifact_icon", { key: { ...iterm, name } }))).toBeNull();
    }
  });
});

describe("the preview stays out of the app", () => {
  /** Every relative module specifier in `source`: static imports, re-exports, dynamic imports. */
  function relativeImports(source: string): string[] {
    const found = source.matchAll(/(?:\bfrom\s+|\bimport\s*\(\s*|\bimport\s+)["'](\.[^"']*)["']/g);
    return [...found].map((match) => match[1]);
  }

  it("is imported by nothing outside src/dev", () => {
    // The mock backend, its pretend Mac and its generated icons reach a
    // page only through vite.config.ts's `--mode mock` alias; a module
    // outside src/dev that imported any of them would put them in the app.
    const src = path.resolve(__dirname, "..");
    const dev = path.resolve(__dirname);
    const files = readdirSync(src, { recursive: true, encoding: "utf-8" })
      .filter((file) => /\.(ts|tsx)$/.test(file))
      .map((file) => path.resolve(src, file))
      .filter((file) => !file.startsWith(dev + path.sep));
    expect(files.length).toBeGreaterThan(0);
    const offenders = files.flatMap((file) =>
      relativeImports(readFileSync(file, "utf-8"))
        .map((specifier) => path.resolve(path.dirname(file), specifier))
        .filter((target) => target === dev || target.startsWith(dev + path.sep))
        .map((target) => `${path.relative(src, file)} imports ${path.relative(src, target)}`),
    );
    expect(offenders).toEqual([]);
  });
});

describe("the preview's stand-ins for Tauri", () => {
  it("replace every module of Tauri's the app imports, and no other", () => {
    // One the page imports that the preview did not replace would run for
    // real there: in a browser, with no Tauri to reach, it throws; in
    // `pnpm tauri:mock`'s window, it reaches the app's own backend.
    const config = readFileSync(path.resolve(__dirname, "../../vite.config.ts"), "utf-8");
    const replaced = [...config.matchAll(/find: \/\^(@tauri-apps[^$]*)\$\//g)].map((match) =>
      match[1].replace(/\\\//g, "/"),
    );
    // What goes into the app: src, less the tests, their helpers and the
    // preview itself. A type-only import is gone from the build.
    const src = path.resolve(__dirname, "..");
    const outside = [path.resolve(__dirname), path.join(src, "test")];
    const tauriImport = /^import\s+(?!type\b)[^;]*?\bfrom\s+["'](@tauri-apps\/[^"']+)["']/gm;
    const imported = readdirSync(src, { recursive: true, encoding: "utf-8" })
      .filter((file) => /\.(ts|tsx)$/.test(file) && !/\.test\.tsx?$/.test(file))
      .map((file) => path.resolve(src, file))
      .filter((file) => !outside.some((dir) => file.startsWith(dir + path.sep)))
      .flatMap((file) => [...readFileSync(file, "utf-8").matchAll(tauriImport)].map((match) => match[1]));
    expect(imported.length).toBeGreaterThan(0);
    expect([...replaced].sort()).toEqual([...new Set(imported)].sort());
  });

  it("badge no Dock: the window's setBadgeCount does nothing and needs no Tauri", async () => {
    // jsdom has no Tauri: the real `setBadgeCount` could not be called here.
    await expect(previewWindow().setBadgeCount(3)).resolves.toBeUndefined();
    await expect(previewWindow().setBadgeCount(undefined)).resolves.toBeUndefined();
  });

});

describe("the preview's URL switches", () => {
  it("reads every switch", () => {
    expect(parseScenario("?outcome=mixed").scenario.outcome).toBe("mixed");
    const { scenario, problems } = parseScenario(
      "?state=offline&lang=zh-CN&tech=1&page=updates&outcome=failed&scan=stopped&sizes=pending&path=unread&welcome=1",
    );
    expect(problems).toEqual([]);
    expect(scenario).toEqual({
      state: "offline",
      language: "ZhCn",
      technicalDetails: true,
      page: "updates",
      outcome: "failed",
      scan: "stopped",
      sizes: "pending",
      path: "unread",
      welcome: true,
    });
  });

  it("shows the welcome sheet only with ?welcome=1", async () => {
    const seen = async (overrides: Partial<Scenario>) =>
      ((await backendFor(overrides).backend.invoke("get_settings")) as Settings).welcome_seen;
    expect(parseScenario("").scenario.welcome).toBe(false);
    expect(await seen({})).toBe(true);
    expect(await seen({ welcome: parseScenario("?welcome=1").scenario.welcome })).toBe(false);
  });

  it("opens on any page, the Overview included", () => {
    for (const page of ["overview", "updates", "installed", "unknown", "settings"] as const) {
      const { scenario, problems } = parseScenario(`?page=${page}`);
      expect(problems).toEqual([]);
      expect(scenario.page).toBe(page);
    }
  });

  it("falls back to the default for a value it does not know, and says so", () => {
    const { scenario, problems } = parseScenario("?state=bogus&lang=fr");
    expect(scenario).toEqual(DEFAULT_SCENARIO);
    expect(problems).toHaveLength(2);
  });
});

describe("the preview's commands, and which copy runs", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  function factsOf(snapshot: Snapshot, instanceId: string, name: string) {
    return snapshot.artifacts.find((a) => a.key.instance_id === instanceId && a.key.name === name)?.facts;
  }

  it("says what each source's notice says: Grok Build not found off the search path, npm's Claude Code first", async () => {
    const full = await answer<Snapshot>(backendFor().backend.invoke("refresh"));
    // `~/.grok/bin` is off the search path in the default state (its
    // `NotOnPath` note): both of its commands, and the folder to add.
    const notFound = { NotOnPath: { dir: "~/.grok/bin" } };
    expect(factsOf(full, "standalone-grok", "grok")?.commands).toEqual([
      { name: "agent", state: notFound },
      { name: "grok", state: notFound },
    ]);
    expect(factsOf(full, "standalone-claude", "claude")?.commands).toEqual([{ name: "claude", state: "Runs" }]);
    // Keg-only, linked by hand: judged as any formula's.
    expect(factsOf(full, "brew:/opt/homebrew", "node@22")?.commands).toEqual([{ name: "node", state: "Runs" }]);

    const notices = await answer<Snapshot>(backendFor({ state: "notices" }).backend.invoke("refresh"));
    const npmClaude = notices.artifacts.find((a) => a.key.name === "@anthropic-ai/claude-code");
    expect(npmClaude?.facts).toEqual({
      ...NO_FACTS,
      family: "claude-code",
      commands: [{ name: "claude", state: "Runs" }],
    });
    expect(factsOf(notices, "standalone-claude", "claude")).toEqual({
      ...NO_FACTS,
      family: "claude-code",
      commands: [{ name: "claude", state: { ShadowedBy: { by: npmClaude?.key } } }],
    });
    // The launcher with no program: nothing to name.
    expect(factsOf(notices, "standalone-grok", "grok")?.commands).toEqual([]);
    // npm's rows keep theirs in the npm in /usr/local.
    expect(factsOf(notices, "npm:/usr/local", "typescript")?.commands.map((c) => c.name)).toEqual(["tsc", "tsserver"]);

    // Every source answering: Grok Build's folder is on the search path.
    const upToDate = await answer<Snapshot>(backendFor({ state: "uptodate" }).backend.invoke("refresh"));
    expect(factsOf(upToDate, "standalone-grok", "grok")?.commands).toEqual([
      { name: "agent", state: "Runs" },
      { name: "grok", state: "Runs" },
    ]);
    // Without Codex's own install, which Banager never checks, so the
    // pages can say "Everything is up to date".
    expect(upToDate.instances.some((i) => i.adapter_id === "standalone-codex")).toBe(false);
    expect(everySourceChecked(upToDate.instances, upToDate.errors)).toBe(true);
  });

  it("gives each row facts of its own, and leaves the shared empty ones alone", async () => {
    await answer<Snapshot>(backendFor({ state: "notices" }).backend.invoke("refresh"));
    expect(NO_FACTS).toEqual({ family: null, homebrew: null, commands: [], commands_unavailable: false });
  });
});

describe("the mock backend's first-round list (InventoryPreview)", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  function previewsIn(events: UiEvent[]): InventoryPreview[] {
    return events.flatMap((e) => ("InventoryPreview" in e ? [e.InventoryPreview] : []));
  }

  it("sends the first round's list before that round commits, and no later round's", async () => {
    const { backend, events } = backendFor();
    const first = backend.invoke("refresh");
    // Sent at `TIMING.inventory`, and reaching the page a task later.
    await vi.advanceTimersByTimeAsync(TIMING.inventory + 1);
    const [preview, ...more] = previewsIn(events);
    expect(more).toEqual([]);
    expect(preview.round).toBe(1);
    expect(((await backend.invoke("get_snapshot")) as Snapshot).round).toBe(0);

    const snapshot = await answer<Snapshot>(first);
    expect(snapshot.round).toBe(preview.round);
    expect(preview.instances).toEqual(snapshot.instances);
    // What every answering source listed, and nothing of a source that is
    // not running, which is never asked for its list. Each AI tool's family
    // is set already; which copy of a command runs is judged only when the
    // round commits.
    const answering = new Set(snapshot.instances.filter((i) => i.status.unavailable === null).map((i) => i.id));
    expect(preview.artifacts).toEqual(
      snapshot.artifacts
        .filter((a) => answering.has(a.key.instance_id))
        .map((a) => ({ ...a, facts: { ...a.facts, commands: [] } })),
    );
    expect(preview.artifacts.some((a) => a.facts.family !== null)).toBe(true);
    expect(preview.artifacts.length).toBeGreaterThan(0);

    await answer(backend.invoke("refresh"));
    await vi.runOnlyPendingTimersAsync();
    expect(previewsIn(events)).toHaveLength(1);
  });

  it("?state=preview sends the list and never finishes checking", async () => {
    expect(parseScenario("?state=preview")).toEqual({
      scenario: { ...DEFAULT_SCENARIO, state: "preview" },
      problems: [],
    });
    const { backend, events } = backendFor({ state: "preview" });
    let settled = false;
    void backend.invoke("refresh").then(() => {
      settled = true;
    });
    await vi.advanceTimersByTimeAsync(60_000);
    expect(settled).toBe(false);
    expect(previewsIn(events)).toHaveLength(1);
    expect(((await backend.invoke("get_snapshot")) as Snapshot).round).toBe(0);
  });

  it("sends no list where nothing is installed, nor where loading fails", async () => {
    for (const state of ["empty", "nothing", "refresh-error"] as const) {
      const { backend, events } = backendFor({ state });
      await answer(backend.invoke("refresh").catch(() => undefined));
      await vi.runOnlyPendingTimersAsync();
      expect(previewsIn(events), state).toEqual([]);
    }
  });

  it("serves a few weeks of history, two of them today, and keeps each operation of its own as Rust does", async () => {
    // Today's two are 25 and 70 minutes old, younger just after midnight:
    // today's at any hour, the last minutes of the day and the first too.
    for (const [hour, minute] of [[0, 0], [0, 5], [12, 0], [23, 55]] as const) {
      vi.setSystemTime(new Date(2026, 9, 1, hour, minute));
      const { records } = await answer<HistoryView>(backendFor().backend.invoke("get_history"));
      const today = records.filter((r) => new Date(r.finished_at).toDateString() === new Date().toDateString());
      expect(today.map((r) => r.key.name), `at ${hour}:${minute}`).toEqual(["htop", "ripgrep"]);
      expect(today.every((r) => r.finished_at <= Date.now()), `at ${hour}:${minute}`).toBe(true);
    }
    const { backend } = backendFor();
    const start = await answer<HistoryView>(backend.invoke("get_history"));
    expect(start.records.some((r) => r.result !== "Succeeded")).toBe(true);
    expect(start.records.some((r) => r.kind === "Uninstall")).toBe(true);
    // An update that failed and one to check, for 「最近的更新记录」 to list among the rest.
    expect(start.records.filter((r) => r.kind === "Update").map((r) => r.result)).toEqual(
      expect.arrayContaining([{ Failed: { cause: "network" } }, { NeedsAttention: "UnchangedAfterUpgrade" }]),
    );
    // Newest first, and none of this launch's.
    expect(start.records.map((r) => r.finished_at)).toEqual([...start.records.map((r) => r.finished_at)].sort((a, b) => b - a));
    expect(start.records.every((r) => r.run !== start.run)).toBe(true);

    await answer(backend.invoke("refresh"));
    const [opId] = await submitUpgrades(backend, "git");
    await vi.runAllTimersAsync();
    const after = await answer<HistoryView>(backend.invoke("get_history"));
    const mine = after.records[0];
    expect(mine).toMatchObject({ run: after.run, op_id: opId, kind: "Update", from_version: "2.55.0", to_version: "2.55.1", verified: true });
    expect(after.records).toHaveLength(start.records.length + 1);

    const cleared = await answer<HistoryView>(backend.invoke("clear_history"));
    expect(cleared.cleared_before).not.toBeNull();
    expect(cleared.records).toHaveLength(after.records.length);
  });
});

describe("the mock backend's facts for the diagnostic text (get_system_facts)", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it("names each source the last refresh found, every path under the home folder as ~", async () => {
    const { backend } = backendFor();
    const before = await answer<SystemFacts>(backend.invoke("get_system_facts"));
    expect(before.sources).toEqual([]);
    const snapshot = await answer<Snapshot>(backend.invoke("refresh"));
    const facts = await answer<SystemFacts>(backend.invoke("get_system_facts"));
    expect(facts.sources.map((source) => source.instance_id)).toEqual(snapshot.instances.map((i) => i.id));
    // The ids name where a source is, as Rust's do; the text never
    // prints one, and the paths it does print are written with ~.
    expect(JSON.stringify([facts.path_dirs, facts.sources.map((source) => source.exe_path)])).not.toContain(
      "/Users/",
    );
    expect(facts.path_dirs).toContain("~/.local/bin");
    expect(facts.sources.find((source) => source.instance_id.startsWith("standalone-claude"))?.exe_path).toBe(
      "~/.local/bin/claude",
    );
  });

  it("keeps npm's folder in ~/Documents with ?path=unread, so its rows are view only for that reason (decision I23)", async () => {
    const plain = await answer<Snapshot>(backendFor().backend.invoke("refresh"));
    const NPM = "npm:/opt/homebrew";
    expect(plain.instances.find((instance) => instance.id === NPM)?.read_only_reason).toBeNull();

    const { backend } = backendFor({ path: "unread" });
    const snapshot = await answer<Snapshot>(backend.invoke("refresh"));
    const npm = snapshot.instances.find((instance) => instance.id === NPM);
    expect(npm).toMatchObject({ prefix: "/Users/you/Documents/npm-global", read_only_reason: "PrefixProtected" });
    // And a click on one of its rows is refused for it.
    const tool = snapshot.artifacts.find((artifact) => artifact.key.instance_id === NPM && artifact.key.name !== "npm");
    expect(tool).toBeDefined();
    const refused = expect(
      backend.invoke("plan_operation", {
        request: { kind: "Uninstall", instance_id: NPM, artifact_kind: tool!.key.kind, name: tool!.key.name },
      }),
    ).rejects.toMatch(/"read_only":"PrefixProtected"/);
    await vi.runOnlyPendingTimersAsync();
    await refused;
  });

  it("says what the last refresh made of the PATH folders, as ?path= asks (Check Tool Setup)", async () => {
    const read = backendFor().backend;
    expect((await answer<SystemFacts>(read.invoke("get_system_facts"))).path_folders).toBeNull();
    await answer<Snapshot>(read.invoke("refresh"));
    const facts = await answer<SystemFacts>(read.invoke("get_system_facts"));
    expect(facts.path_folders).toEqual({ read: facts.path_dirs.length, unread: [] });

    const unread = backendFor({ path: "unread" }).backend;
    await answer<Snapshot>(unread.invoke("refresh"));
    const some = await answer<SystemFacts>(unread.invoke("get_system_facts"));
    expect(some.path_folders).toEqual({ read: some.path_dirs.length - 1, unread: ["~/Documents/bin"] });

    // Never restored: the system's few folders, and no command judged.
    const unrestored = backendFor({ path: "default" }).backend;
    const snapshot = await answer<Snapshot>(unrestored.invoke("refresh"));
    const none = await answer<SystemFacts>(unrestored.invoke("get_system_facts"));
    expect(none).toMatchObject({ login_path: false, path_folders: null, path_dirs: ["/usr/bin", "/bin", "/usr/sbin", "/sbin"] });
    const commands = snapshot.artifacts.flatMap((artifact) => artifact.facts.commands);
    expect(commands.length).toBeGreaterThan(0);
    expect(commands.every((command) => command.state === null)).toBe(true);
  });

  it("has no verdict, with a folder at the end of PATH unread, for a command no folder read leads to", async () => {
    const notFound = (snapshot: Snapshot) =>
      snapshot.artifacts.filter((artifact) =>
        artifact.facts.commands.some(({ state }) => typeof state === "object" && state !== null && "NotOnPath" in state),
      );
    const read = await answer<Snapshot>(backendFor().backend.invoke("refresh"));
    const unread = await answer<Snapshot>(backendFor({ path: "unread" }).backend.invoke("refresh"));
    expect(notFound(read).length).toBeGreaterThan(0);
    // The unread folder might hold a link to it (`commands::judge`): not
    // "not found" but no verdict, for those tools only.
    expect(notFound(unread)).toEqual([]);
    const withoutVerdict = unread.artifacts.filter((artifact) =>
      artifact.facts.commands.some(({ state }) => state === null),
    );
    for (const artifact of notFound(read)) {
      expect(withoutVerdict.map((a) => a.key)).toContainEqual(artifact.key);
    }
    expect(unread.artifacts.some((artifact) => artifact.facts.commands.some(({ state }) => state === "Runs"))).toBe(true);
    const dropped = unread.artifacts.find((artifact) => artifact.key.instance_id === "pipx" && artifact.key.name === "poetry")!;
    expect(dropped.path).toBe("/Users/you/Documents/venvs/poetry");
    expect(dropped.facts.commands).toEqual([]);
    expect(dropped.facts.commands_unavailable).toBe(true);
    expect(toolsNotJudged([dropped])).toBe(1);
  });
});

it("starts the browser mock in Traditional Chinese", async () => {
  const { scenario, problems } = parseScenario("?lang=zh-Hant");
  expect(problems).toEqual([]);
  expect(scenario.language).toBe("ZhHant");
});
