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
  IssuedPlan,
  OpRequest,
  OpStatus,
  OpSummary,
  Outcome,
  Plan,
  PlanId,
  Settings,
  Snapshot,
  UiEvent,
} from "../lib/types";
import { buildWorld, initialSettings, sameKey, unknownScan, unverifiedVersion, type World } from "./mockData";
import { appIcon } from "./mockIcons";
import { buildPlan, playOutcome, refusal, type LogLine, type Subject } from "./mockPlans";
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

  /** The part of a snapshot that decides its generation. */
  function snapshotContent(from: World, current: Settings) {
    return {
      detect: from.detect,
      instances: from.instances,
      artifacts: from.artifacts,
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
    committed = clone({
      generation,
      round,
      ...content,
      refreshed_at: nowSeconds(),
      stale: content.errors.length > 0,
    });
    if (generation > lastAnnounced) {
      lastAnnounced = generation;
      emit({ SnapshotChanged: { generation } });
    }
    return clone(committed);
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
    if (outcome === "Succeeded") apply(op.plan);
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
      return clone(committed ?? EMPTY_SNAPSHOT);
    },
    async refresh() {
      if (scenario.state === "loading") return never();
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
        plan: buildPlan(world, inst, request),
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
