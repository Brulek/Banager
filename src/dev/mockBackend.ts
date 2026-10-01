/**
 * The browser preview's backend (docs/ui-preview.md): every command
 * src/lib/api.ts sends, answered from the machine in ./mockData.ts the way
 * src-tauri/src/ipc.rs and crates/banager-core answer it -- the same
 * refusals, the same event sequence for an operation, the same snapshot
 * generations. Dev-only: ./mockTauri.ts is its one caller, and only
 * `vite --mode mock` puts that in a page. Deterministic apart from the
 * clock: timings are fixed, and nothing is random.
 */
import type {
  ArtifactKey,
  HistoryView,
  InstalledArtifact,
  IssuedPlan,
  OpRequest,
  OpStatus,
  OpSummary,
  Outcome,
  Plan,
  PlanId,
  Settings,
  Sizes,
  Snapshot,
  UiEvent,
} from "../lib/types";
import { NO_SIZES } from "../lib/types";
import { checkEvery } from "../lib/checkFrequency";
import { buildWorld, initialSettings, sameKey, unknownScan, unverifiedVersion, type World } from "./mockData";
import { appIcon } from "./mockIcons";
import { withFamilies } from "./mockFamilies";
import { buildPlan, playOutcome, refusal, type LogLine, type Subject } from "./mockPlans";
import { withMockKeptData } from "./mockKeptData";
import { mockSizes } from "./mockSizes";
import { mockSystemFacts } from "./mockDiagnostics";
import { mockHistory, mockRecord } from "./mockHistory";
import type { Scenario } from "./scenario";

/** Every command the backend registers (`generate_handler!` in src-tauri/src/lib.rs). */
export const MOCK_COMMANDS = [
  "get_snapshot",
  "refresh",
  "plan_operation",
  "submit_operation",
  "cancel_operation",
  "list_operations",
  "get_settings",
  "set_settings",
  "subscribe_events",
  "open_ollama_app",
  "scan_unknown",
  "artifact_icon",
  "get_sizes",
  "get_system_facts",
  "get_history",
  "clear_history",
  "set_menu_language",
  "report_update_set",
  "request_notification_permission",
  "ask_before_quit",
  "quit_question_shown",
  "quit_kept_waiting",
  "quit_anyway",
] as const;
type MockCommand = (typeof MOCK_COMMANDS)[number];

/** What `subscribe_events` is handed: a Tauri `Channel`, or the preview's stand-in. */
export interface EventChannel {
  onmessage: (event: UiEvent) => void;
}

export interface MockBackend {
  /** One IPC call; rejects with a string, as Tauri's `invoke` does. */
  invoke(cmd: string, args?: unknown): Promise<unknown>;
}

/** How long things take, in milliseconds. */
export const TIMING = {
  refresh: 900,
  /**
   * The first refresh's list (`InventoryPreview`), before that refresh
   * answers at `refresh`: every source has listed what it has, and the
   * update checks run on.
   */
  inventory: 300,
  plan: 400,
  /** A Homebrew uninstall preview runs `brew uses --installed` first. */
  brewUninstallPlan: 1200,
  scan: 700,
  openOllama: 300,
  /** Queued, then Running once its locks are free. */
  start: 250,
  /** Running to Verifying: the tool's own work, its log lines spread over it. */
  run: 4200,
  /** How long a Homebrew operation waits for a `brew update` still running. */
  brewUpdateWait: 3500,
  /** Verifying to Finished. */
  verify: 550,
  /** A cancelled command stopping. */
  cancel: 500,
  /** Drawing a cask's app icon (the real one remembers each after the first). */
  icon: 60,
  /**
   * Measuring how much each tool takes, after a refresh commits: long
   * enough to see 「正在计算…」 in the details first. A folder measured
   * before at the same version is shown at once, as the real one
   * remembers it.
   */
  sizes: 1500,
} as const;

/** How many operations may run at once (`OperationManager`'s semaphore). */
const MAX_RUNNING = 3;

/** A preview older than this is refused (`SubmitError::Expired`). */
const PLAN_LIFETIME_MS = 10 * 60 * 1000;

/** `Snapshot::empty()`: what `get_snapshot` answers before any refresh. */
const EMPTY_SNAPSHOT: Snapshot = {
  generation: 0,
  round: 0,
  detect: "Missing",
  instances: [],
  artifacts: [],
  updates: [],
  refreshed_at: null,
  stale: false,
  errors: [],
  next_auto_check_at: null,
};

/** What the backend says when it is broken (`?state=error`, `?state=refresh-error`). */
const BROKEN = (cmd: string) => `command ${cmd} failed: the backend is not responding`;

function clone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}

function wait(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

/** A promise that never settles: a command still running when the screenshot is taken. */
function never<T>(): Promise<T> {
  return new Promise<T>(() => {});
}

/** `auto_check::DUE_AFTER_SECS`: the daily check is due a day after the last check. */
const DAY_SECONDS = 24 * 60 * 60;

function nowSeconds(): number {
  return Math.floor(Date.now() / 1000);
}

function requestKey(request: OpRequest): ArtifactKey {
  return { instance_id: request.instance_id, kind: request.artifact_kind, name: request.name };
}

interface Operation {
  summary: OpSummary;
  plan: Plan;
  timers: ReturnType<typeof setTimeout>[];
  /** Whether it holds its locks and a slot, from start until it finishes. */
  started: boolean;
}

export function createMockBackend(scenario: Scenario): MockBackend {
  const world: World = buildWorld(scenario.state);
  let settings: Settings = initialSettings(scenario);
  let committed: Snapshot | null = null;
  let generation = 0;
  let round = 0;
  let lastContent = JSON.stringify(snapshotContent(buildWorld("empty"), settings));
  let lastAnnounced = 0;
  const channels = new Set<EventChannel>();
  const plans = new Map<PlanId, { issued: IssuedPlan; issuedAtMs: number }>();
  let planCount = 0;
  const operations = new Map<number, Operation>();
  const waiting: number[] = [];
  const held = new Set<string>();
  let running = 0;
  let nextOpId = 1;
  /** What `get_sizes` answers: the newest round's sizes so far. */
  let sizes: Sizes = NO_SIZES;
  let sizesTimer: ReturnType<typeof setTimeout> | null = null;
  /** `key|version` of every size measured so far, as the real meter's cache. */
  const measuredBefore = new Set<string>();
  /** What `get_history` answers: earlier launches' records, then this one's. */
  let history: HistoryView = mockHistory(Date.now());

  /** The part of a snapshot that decides its generation. */
  function snapshotContent(from: World, current: Settings) {
    return {
      detect: from.detect,
      instances: from.instances,
      // Which AI coding tool each is, set here once, as `families::assign`
      // does where Rust puts a snapshot together.
      artifacts: withFamilies(from.instances, from.artifacts),
      updates: current.include_self_updating
        ? [...from.updates, ...from.greedyUpdates]
        : from.updates,
      errors: from.errors,
    };
  }

  /** Events reach the page asynchronously, in order, as over Tauri's IPC. */
  function emit(event: UiEvent): void {
    const payload = clone(event);
    for (const channel of channels) {
      setTimeout(() => channel.onmessage(payload), 0);
    }
  }

  /**
   * One refresh round's result, committed: numbered one above the last
   * (`Snapshot::round`), the generation moving only when the content does
   * (`Snapshot::same_content`), and a new generation announced once
   * (`announce` in src-tauri/src/ipc.rs).
   */
  function commit(): Snapshot {
    const content = snapshotContent(world, settings);
    const serialized = JSON.stringify(content);
    if (serialized !== lastContent) {
      generation += 1;
      lastContent = serialized;
    }
    round += 1;
    const refreshedAt = nowSeconds();
    committed = clone({
      generation,
      round,
      ...content,
      refreshed_at: refreshedAt,
      stale: content.errors.length > 0,
      // Every round here is the window's, and counts as a check: the daily
      // one is due a day after it (`auto_check::next_check_due`), the
      // weekly one a week, which Settings shows under its popup once it is
      // on.
      next_auto_check_at: refreshedAt + (checkEvery(settings) === "Week" ? 7 : 1) * DAY_SECONDS,
    });
    if (generation > lastAnnounced) {
      lastAnnounced = generation;
      emit({ SnapshotChanged: { generation } });
    }
    measureSizes(committed);
    return clone(committed);
  }

  /**
   * A round of measuring for `snapshot`, as `SizeMeter` runs one after
   * each commit: what it will measure at once -- remembered ones filled
   * in, the rest "measuring" -- then, `TIMING.sizes` later, everything;
   * `SizesChanged` each time. A newer commit stops the older round. With
   * `?sizes=pending` the round never finishes.
   */
  function measureSizes(snapshot: Snapshot): void {
    if (sizesTimer !== null) clearTimeout(sizesTimer);
    sizesTimer = null;
    const remembered = (a: InstalledArtifact) => measuredBefore.has(`${artifactId(a)}|${a.version}`);
    const round = snapshot.round;
    sizes = mockSizes(round, snapshot.instances, snapshot.artifacts, remembered);
    emit({ SizesChanged: { round } });
    if (sizes.done || scenario.sizes === "pending") return;
    sizesTimer = setTimeout(() => {
      sizesTimer = null;
      sizes = mockSizes(round, snapshot.instances, snapshot.artifacts, () => true);
      for (const size of sizes.artifacts) {
        measuredBefore.add(`${size.key.instance_id}|${size.key.kind}|${size.key.name}|${size.version}`);
      }
      emit({ SizesChanged: { round } });
    }, TIMING.sizes);
  }

  function artifactId(artifact: InstalledArtifact): string {
    return `${artifact.key.instance_id}|${artifact.key.kind}|${artifact.key.name}`;
  }

  /** The actionability gate `Session::issue_plan` and `Session::submit` apply. */
  function gate(request: OpRequest) {
    const inst = world.instances.find((i) => i.id === request.instance_id);
    if (inst === undefined) throw refusal({ kind: "source_gone" });
    if (inst.read_only_reason !== null || inst.status.unavailable !== null) {
      throw refusal({
        kind: "not_actionable",
        read_only: inst.read_only_reason,
        unavailable: inst.status.unavailable,
      });
    }
    return inst;
  }

  /**
   * The first round's list, before its update checks are done
   * (`refresh_recording`'s `preview` in
   * crates/banager-core/src/session/refresh.rs): sent at `TIMING.inventory`
   * while nothing has committed, with every source detected and the rows of
   * every one that answered -- not one that is not running, which is never
   * asked for its list. Nothing listed, nothing sent; a round that has
   * committed by then sends nothing either. Each AI tool's family is set,
   * as Rust sets it on the preview; which copy of a command runs is not
   * judged until the round commits, so `commands` is empty.
   */
  function previewFirstRound(): void {
    const previewRound = round + 1;
    setTimeout(() => {
      if (committed !== null) return;
      const answering = new Set(world.instances.filter((i) => i.status.unavailable === null).map((i) => i.id));
      const artifacts = withFamilies(
        world.instances,
        world.artifacts.filter((a) => answering.has(a.key.instance_id)),
      ).map((a) => ({ ...a, facts: { ...a.facts, commands: [] } }));
      if (artifacts.length === 0) return;
      emit({ InventoryPreview: { round: previewRound, instances: world.instances, artifacts } });
    }, TIMING.inventory);
  }

  /** The update rows the next refresh reports, the greedy ones included when switched on. */
  function currentUpdates() {
    return snapshotContent(world, settings).updates;
  }

  // ------------------------------------------------------------ operations

  function schedule(op: Operation, delay: number, run: () => void): void {
    op.timers.push(setTimeout(run, delay));
  }

  function stopTimers(op: Operation): void {
    for (const timer of op.timers) clearTimeout(timer);
    op.timers = [];
  }

  function setStatus(op: Operation, status: OpStatus): void {
    op.summary.status = status;
    emit({ Operation: { Status: { op_id: op.summary.id, status } } });
  }

  function writeLine(op: Operation, line: LogLine): void {
    const op_id = op.summary.id;
    emit(
      "note" in line
        ? { Operation: { Note: { op_id, note: line.note } } }
        : { Operation: { Log: { op_id, stream: line.stream, line: line.line } } },
    );
  }

  /** What a succeeded operation changed, for the next refresh to find. */
  function apply(plan: Plan): void {
    const target = requestKey(plan.request);
    const inst = world.instances.find((i) => i.id === target.instance_id);
    if (plan.request.kind === "Upgrade") {
      const candidate = currentUpdates().find((u) => sameKey(u.key, target));
      const row = world.artifacts.find((a) => sameKey(a.key, target));
      if (candidate !== undefined && row !== undefined) {
        const previous = row.version;
        // A model's version is its local manifest digest: a new one.
        row.version = target.kind === "Model" ? candidate.target.replace("sha256:", "") : candidate.target;
        if (row.path !== null && previous !== "") row.path = row.path.split(previous).join(row.version);
        // A standalone tool is its own source: its version is the tool's.
        if (inst !== undefined && inst.adapter_id.startsWith("standalone-")) {
          inst.version = row.version;
          inst.unverified_version = unverifiedVersion(inst.adapter_id, row.version);
        }
      }
      world.updates = world.updates.filter((u) => !sameKey(u.key, target));
      world.greedyUpdates = world.greedyUpdates.filter((u) => !sameKey(u.key, target));
      return;
    }
    if (plan.request.kind === "Uninstall") {
      world.artifacts = world.artifacts.filter((a) => !sameKey(a.key, target));
      world.updates = world.updates.filter((u) => !sameKey(u.key, target));
      world.greedyUpdates = world.greedyUpdates.filter((u) => !sameKey(u.key, target));
      // With its launcher gone, detect no longer finds the tool at all.
      if (inst !== undefined && inst.adapter_id.startsWith("standalone-")) {
        world.instances = world.instances.filter((i) => i.id !== inst.id);
      }
    }
  }

  function finish(op: Operation, outcome: Outcome): void {
    stopTimers(op);
    op.summary.status = "Done";
    op.summary.outcome = outcome;
    const target = requestKey(op.plan.request);
    // Read before `apply`, which changes the row in place.
    const row = world.artifacts.find((a) => sameKey(a.key, target));
    const before = row === undefined ? null : { name: row.display_name, version: row.version };
    if (outcome === "Succeeded") apply(op.plan);
    // Kept before `Finished` is sent, as `OnFinish` keeps it.
    const record = mockRecord({
      run: history.run,
      opId: op.summary.id,
      request: op.plan.request,
      outcome,
      started: op.started,
      displayName: before?.name ?? target.name,
      adapterId: world.instances.find((i) => i.id === target.instance_id)?.adapter_id ?? target.instance_id.split(":")[0],
      before: before?.version ?? null,
      after: world.artifacts.find((a) => sameKey(a.key, target))?.version ?? null,
      now: Date.now(),
    });
    if (record !== null) history = { ...history, records: [record, ...history.records] };
    if (op.started) {
      for (const lock of op.plan.locks) held.delete(lock);
      running -= 1;
      op.started = false;
    }
    emit({ Operation: { Finished: { op_id: op.summary.id, outcome } } });
    startWaiting();
  }

  /** Runs `op` from Queued to Finished: its tool's lines over `TIMING.run`. */
  function start(op: Operation): void {
    op.started = true;
    running += 1;
    for (const lock of op.plan.locks) held.add(lock);
    const target = requestKey(op.plan.request);
    const inst = world.instances.find((i) => i.id === target.instance_id);
    if (inst === undefined) {
      // Its source went while it waited (an uninstall of the same tool
      // ahead of it): the real manager reports that as its own fault.
      schedule(op, 0, () => finish(op, { BanagerFailed: "Internal" }));
      return;
    }
    const subject: Subject = {
      inst,
      artifact: world.artifacts.find((a) => sameKey(a.key, target)),
      candidate: currentUpdates().find((u) => sameKey(u.key, target)),
    };
    const { lines, outcome } = playOutcome(op.plan, subject, scenario.outcome);
    let at = TIMING.start;
    schedule(op, at, () => setStatus(op, "Running"));
    // A `brew update` a refresh left running: Homebrew operations wait
    // for it first (`BrewAdapter::wait_for_update`), and say so.
    if (subject.inst.adapter_id === "brew" && subject.inst.status.notes.includes("IndexUpdating")) {
      schedule(op, at + 150, () =>
        writeLine(op, { note: { WaitingForBrewUpdate: { minutes: 10 } } }),
      );
      at += TIMING.brewUpdateWait;
    }
    const step = TIMING.run / (lines.length + 1);
    lines.forEach((line, index) => {
      schedule(op, at + step * (index + 1), () => writeLine(op, line));
    });
    at += TIMING.run;
    schedule(op, at, () => setStatus(op, "Verifying"));
    schedule(op, at + TIMING.verify, () => finish(op, outcome));
  }

  /** Starts every waiting operation whose locks are free, oldest first. */
  function startWaiting(): void {
    for (const id of [...waiting]) {
      if (running >= MAX_RUNNING) return;
      const op = operations.get(id);
      if (op === undefined || op.plan.locks.some((lock) => held.has(lock))) continue;
      const index = waiting.indexOf(id);
      if (index === -1) continue;
      waiting.splice(index, 1);
      start(op);
    }
  }

  function submit(planId: PlanId): number {
    const stored = plans.get(planId);
    if (stored === undefined) throw refusal({ kind: "unknown" });
    plans.delete(planId);
    if (Date.now() - stored.issuedAtMs > PLAN_LIFETIME_MS) throw refusal({ kind: "expired" });
    const { plan } = stored.issued;
    gate(plan.request);
    const id = nextOpId;
    nextOpId += 1;
    const op: Operation = {
      summary: {
        id,
        kind: plan.request.kind,
        instance_id: plan.request.instance_id,
        artifact_kind: plan.request.artifact_kind,
        name: plan.request.name,
        status: "Queued",
        outcome: null,
        argv_preview:
          "Command" in plan.action ? [plan.action.Command.program, ...plan.action.Command.args] : [],
        env_preview: "Command" in plan.action ? plan.action.Command.env : [],
        cancel_policy: plan.cancel_policy,
      },
      plan,
      timers: [],
      started: false,
    };
    operations.set(id, op);
    emit({ Operation: { Status: { op_id: id, status: "Queued" } } });
    waiting.push(id);
    startWaiting();
    return id;
  }

  /** `OperationManager::cancel`, as `cancel_operation_impl` reports it. */
  function cancel(opId: number): void {
    const op = operations.get(opId);
    if (op === undefined) return;
    if (op.summary.status === "Queued") {
      // Nothing has run: the cancel is the whole story.
      stopTimers(op);
      const index = waiting.indexOf(opId);
      if (index !== -1) waiting.splice(index, 1);
      setStatus(op, "CancelRequested");
      schedule(op, 150, () => finish(op, "Cancelled"));
      return;
    }
    if (op.summary.status !== "Running") return; // Nothing pending to stop: a silent Ok.
    if (op.plan.cancel_policy === "NoCancel") throw refusal({ kind: "no_cancel" });
    stopTimers(op);
    setStatus(op, "CancelRequested");
    schedule(op, TIMING.cancel, () => setStatus(op, "Cancelling"));
    schedule(op, TIMING.cancel + 300, () => setStatus(op, "Verifying"));
    // A stopped upgrade proves nothing either way; a stopped uninstall
    // that left the package in place is the user's cancel.
    schedule(op, TIMING.cancel + 300 + TIMING.verify, () =>
      finish(op, op.plan.request.kind === "Upgrade" ? "Unconfirmed" : "Cancelled"),
    );
  }

  // --------------------------------------------------------------- commands

  type Args = Record<string, unknown>;
  const handlers: Record<MockCommand, (args: Args) => Promise<unknown>> = {
    async get_snapshot() {
      if (scenario.state === "error") throw BROKEN("get_snapshot");
      if (committed === null) return clone(EMPTY_SNAPSHOT);
      // The next automatic check, for how often it runs as the settings
      // say now (`with_next_auto_check` in src-tauri/src/ipc.rs): a day,
      // or a week, after the last round.
      const days = checkEvery(settings) === "Week" ? 7 : 1;
      return clone({
        ...committed,
        next_auto_check_at: committed.refreshed_at === null ? null : committed.refreshed_at + days * DAY_SECONDS,
      });
    },
    async refresh() {
      if (scenario.state === "loading") return never();
      const failing = scenario.state === "error" || scenario.state === "refresh-error";
      if (committed === null && !failing) previewFirstRound();
      // Its list on screen, and the update checks never done.
      if (scenario.state === "preview") return never();
      await wait(TIMING.refresh);
      if (scenario.state === "error" || scenario.state === "refresh-error") {
        throw BROKEN("refresh");
      }
      return commit();
    },
    async plan_operation(args) {
      const request = args.request as OpRequest;
      // Before the gate, as `plan_operation_impl` (src-tauri/src/ipc.rs)
      // refuses it: no page offers an install.
      if (request.kind === "Install") throw refusal({ kind: "refused" });
      const inst = gate(request);
      await wait(
        inst.adapter_id === "brew" && request.kind === "Uninstall" ? TIMING.brewUninstallPlan : TIMING.plan,
      );
      const target = requestKey(request);
      if (request.kind === "Upgrade") {
        const blocked = currentUpdates().find((u) => sameKey(u.key, target))?.blocked ?? null;
        if (blocked !== null) throw refusal({ kind: "update_blocked", reason: blocked });
      }
      if (request.kind === "Uninstall") {
        const blocked = world.artifacts.find((a) => sameKey(a.key, target))?.uninstall_blocked ?? null;
        if (blocked !== null) throw refusal({ kind: "uninstall_blocked", reason: blocked });
      }
      planCount += 1;
      const issued: IssuedPlan = {
        // 32 hex characters, like the real random token; counted, not random.
        id: planCount.toString(16).padStart(32, "0"),
        // What the uninstall leaves behind, named (`mockKeptData.ts`).
        plan: withMockKeptData(
          buildPlan(world, inst, request),
          inst.adapter_id,
          request,
          world.instances.some((instance) => instance.adapter_id === "standalone-codex"),
        ),
        issued_at: nowSeconds(),
      };
      plans.set(issued.id, { issued, issuedAtMs: Date.now() });
      return clone(issued);
    },
    async submit_operation(args) {
      return submit(args.planId as PlanId);
    },
    async cancel_operation(args) {
      cancel(args.opId as number);
    },
    async list_operations() {
      return [...operations.values()]
        .map((op) => clone(op.summary))
        .sort((a, b) => b.id - a.id);
    },
    async get_settings() {
      return clone(settings);
    },
    async set_settings(args) {
      settings = clone(args.settings as Settings);
    },
    async subscribe_events(args) {
      channels.add(args.channel as EventChannel);
    },
    async open_ollama_app() {
      await wait(TIMING.openOllama);
      // The app starts; the next refresh finds the daemon answering.
      for (const inst of world.instances) {
        if (inst.adapter_id === "ollama" && inst.status.unavailable === "NotRunning") {
          inst.status.unavailable = null;
        }
      }
    },
    async scan_unknown() {
      if (scenario.state === "loading") return never();
      await wait(TIMING.scan);
      if (scenario.scan === "error") throw 'task 17 panicked with message "failed to read /usr/local/bin"';
      return unknownScan(scenario.scan);
    },
    async artifact_icon(args) {
      await wait(TIMING.icon);
      // `Session::artifact_icon`: the key is only matched against the
      // committed snapshot's rows, never read as a path; before the first
      // refresh there are none.
      const key = args.key as ArtifactKey;
      const row = committed?.artifacts.find((a) => sameKey(a.key, key));
      return row === undefined ? null : appIcon(row);
    },
    async get_sizes() {
      // `Session::sizes`: what the newest round has said so far.
      return clone(sizes);
    },
    async get_system_facts() {
      // `diagnostics::current`: the committed snapshot's sources, none
      // before the first refresh.
      return mockSystemFacts(committed?.instances ?? []);
    },
    async get_history() {
      // `Session::history`: every record, newest first.
      return clone(history);
    },
    async clear_history() {
      // `HistoryStore::clear`: the time, and every record kept.
      history = { ...history, cleared_before: Date.now() };
      return clone(history);
    },
    async report_update_set(args) {
      // No notification to post: the preview has no daily check, and so no
      // round of one (src-tauri/src/notify.rs posts only after one). It
      // takes what the real command takes, a round and a list of pairs;
      // Tauri turns anything else away.
      if (typeof args.round !== "number" || !Array.isArray(args.updates)) {
        throw `invalid args for command \`report_update_set\`: ${JSON.stringify(args)}`;
      }
    },
    async request_notification_permission() {
      // Asks nobody: the preview posts no notification, so the switch
      // turns on as it does where permission is granted.
      return true;
    },
    async ask_before_quit(args) {
      // Nothing to ask before: the browser has no Quit of Banager's, and
      // the one `pnpm tauri:mock` shows is Rust's, which this page never
      // reaches, so it never hears the question (./mockTauriEvent.ts).
      // Like the real command, it takes whether the page asks; Tauri turns
      // anything else away.
      if (typeof args.ask !== "boolean") {
        throw `invalid args \`ask\` for command \`ask_before_quit\`: ${JSON.stringify(args.ask)}`;
      }
    },
    async quit_question_shown(args) {
      // No question is ever asked here (above), so none is on screen; like
      // the real command, it takes a question's number, a `u64`.
      const { question } = args;
      if (typeof question !== "number" || !Number.isSafeInteger(question) || question < 0) {
        throw `invalid args \`question\` for command \`quit_question_shown\`: ${JSON.stringify(question)}`;
      }
    },
    async quit_kept_waiting(args) {
      // No question is ever asked here (above), so none is answered; like
      // the real command, it takes a question's number, a `u64`.
      const { question } = args;
      if (typeof question !== "number" || !Number.isSafeInteger(question) || question < 0) {
        throw `invalid args \`question\` for command \`quit_kept_waiting\`: ${JSON.stringify(question)}`;
      }
    },
    async quit_anyway() {
      // Nothing to quit: a page cannot quit the browser, nor reach the Rust
      // of `pnpm tauri:mock`, and the preview never asks (above).
    },
    async set_menu_language(args) {
      // No menu bar to build: the browser has none of Banager's, and the
      // one `pnpm tauri:mock` shows is Rust's, which this page never
      // reaches (./mockTauriEvent.ts). Like the real command, it takes
      // only the window's two languages; Tauri turns any other away.
      if (args.language !== "en" && args.language !== "zh-CN") {
        throw `invalid args \`language\` for command \`set_menu_language\`: ${JSON.stringify(args.language)}`;
      }
    },
  };

  return {
    async invoke(cmd, args) {
      if (!(MOCK_COMMANDS as readonly string[]).includes(cmd)) {
        throw `Command ${cmd} not found`;
      }
      const record = typeof args === "object" && args !== null ? (args as Args) : {};
      return handlers[cmd as MockCommand](record);
    },
  };
}
