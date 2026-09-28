import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type {
  ArtifactKey,
  IssuedPlan,
  OperationEvent,
  OpRequest,
  OpSummary,
  Settings,
  Snapshot,
  UiEvent,
  UpdateCandidate,
} from "../lib/types";
import { hidingRule, updateStateOf } from "../lib/updateState";
import { createMockBackend, MOCK_COMMANDS, type MockBackend } from "./mockBackend";
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
    await expect(backend.invoke("set_menu_language", { language: "fr" })).rejects.toMatch(/^invalid args/);
    await expect(backend.invoke("set_menu_language")).rejects.toMatch(/^invalid args/);
  });

  it("starts empty, like a real launch, and the first refresh commits generation 1", async () => {
    const { backend, events } = backendFor();
    const before = await answer<Snapshot>(backend.invoke("get_snapshot"));
    expect(before).toMatchObject({ generation: 0, round: 0, detect: "Missing", refreshed_at: null });
    const first = await answer<Snapshot>(backend.invoke("refresh"));
    expect(first.generation).toBe(1);
    expect(first.round).toBe(1);
    expect(first.refreshed_at).not.toBeNull();
    await vi.runOnlyPendingTimersAsync();
    expect(events).toContainEqual({ SnapshotChanged: { generation: 1 } });
    // An unchanged refresh keeps its generation and announces nothing new,
    // and is a round of its own (`Snapshot::round`).
    const again = await answer<Snapshot>(backend.invoke("refresh"));
    expect(again.generation).toBe(1);
    expect(again.round).toBe(2);
    expect(await answer<Snapshot>(backend.invoke("get_snapshot"))).toEqual(again);
  });

  it("covers every Updates row state and both ways of hiding one", async () => {
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
      "blocked:Pinned",
      "blocked:SelfUpdatesOnly",
      "cannotCheck",
      "readOnly",
      "sourceUnavailable",
    ]);
    expect(new Set(snapshot.updates.map(hiddenBy))).toEqual(new Set([null, "ignored", "skipped"]));
    expect(snapshot.updates.some((u) => u.channel === "Digest")).toBe(true);
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

  it("show nothing in Finder: revealItemInDir needs no Tauri, and says in the console what it was asked for", async () => {
    // Imported here, under the spy: it shares the preview's console marker
    // with ./mockTauri.ts, whose first line goes to the console on import.
    const info = vi.spyOn(console, "info").mockImplementation(() => {});
    try {
      const { revealItemInDir } = await import("./mockTauriOpener");
      const docker = "/Applications/Docker.app/Contents/Resources/bin/docker";
      await expect(revealItemInDir(docker)).resolves.toBeUndefined();
      const said = String(info.mock.lastCall?.[0]);
      expect(said.startsWith("[canager-ui-preview-mock] ")).toBe(true);
      expect(said.endsWith(docker)).toBe(true);
    } finally {
      info.mockRestore();
    }
  });
});

describe("the preview's URL switches", () => {
  it("reads every switch", () => {
    const { scenario, problems } = parseScenario(
      "?state=offline&lang=zh-CN&tech=1&page=updates&outcome=failed&scan=stopped",
    );
    expect(problems).toEqual([]);
    expect(scenario).toEqual({
      state: "offline",
      language: "ZhCn",
      technicalDetails: true,
      page: "updates",
      outcome: "failed",
      scan: "stopped",
    });
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
