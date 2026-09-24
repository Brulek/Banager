import { invoke, Channel, type InvokeArgs } from "@tauri-apps/api/core";
import type {
  IssuedPlan,
  OpRequest,
  PlanId,
  Settings,
  Snapshot,
  OpSummary,
  UiEvent,
  UnknownScan,
} from "./types";

/**
 * The single choke point for every IPC call. A `#[tauri::command]` that
 * returns `Result<_, String>` rejects with the *bare string*, not an `Error`.
 * TanStack Query types `error` as `Error`, every error surface in this plan
 * renders `error.message`, and `"boom".message` is `undefined` — so without
 * this wrapper those surfaces would render blank. Wrapping here keeps the
 * backend's text verbatim (IPC errors are shown as-is) while making
 * `.message` real. Nothing outside this file calls `invoke`.
 */
function call<T>(cmd: string, args?: InvokeArgs): Promise<T> {
  // A no-argument command must reach `invoke` as `invoke(cmd)`, not
  // `invoke(cmd, undefined)`: the tests assert `toHaveBeenCalledWith("get_snapshot")`
  // with one argument, and vitest counts arguments, so a trailing `undefined`
  // would fail every zero-argument assertion in this task.
  const result = args === undefined ? invoke<T>(cmd) : invoke<T>(cmd, args);
  return result.catch((e: unknown) => {
    throw e instanceof Error ? e : new Error(typeof e === "string" ? e : JSON.stringify(e));
  });
}

export function getSnapshot(): Promise<Snapshot> {
  return call<Snapshot>("get_snapshot");
}

export function refresh(): Promise<Snapshot> {
  return call<Snapshot>("refresh");
}

export function planOperation(request: OpRequest): Promise<IssuedPlan> {
  return call<IssuedPlan>("plan_operation", { request });
}

export function submitOperation(planId: PlanId): Promise<number> {
  return call<number>("submit_operation", { planId });
}

export function cancelOperation(opId: number): Promise<void> {
  return call<void>("cancel_operation", { opId });
}

export function listOperations(): Promise<OpSummary[]> {
  return call<OpSummary[]>("list_operations");
}

export function getSettings(): Promise<Settings> {
  return call<Settings>("get_settings");
}

export function setSettings(settings: Settings): Promise<void> {
  return call<void>("set_settings", { settings });
}

/**
 * Registers a fresh Channel with the backend and forwards every UiEvent it
 * receives to `onEvent`. There is no `unsubscribe_events` command — the
 * backend only drops a Channel from its broadcast registry once a send to it
 * fails (the window closed). The returned function is a client-side detach:
 * it stops this callback from firing, it does not tell the backend anything.
 */
export function subscribeEvents(onEvent: (e: UiEvent) => void): Promise<() => void> {
  const channel = new Channel<UiEvent>();
  channel.onmessage = onEvent;
  return call<void>("subscribe_events", { channel }).then(() => {
    return () => {
      channel.onmessage = () => {};
    };
  });
}

export function openOllamaApp(): Promise<void> {
  return call<void>("open_ollama_app");
}

/**
 * The unknown-source scan over the current snapshot: a directory walk of
 * the usual bin folders, up to ten seconds, on the Rust side. Nothing is
 * cached here; `useUnknownScan` decides when it runs.
 */
export function scanUnknown(): Promise<UnknownScan> {
  return call<UnknownScan>("scan_unknown");
}
