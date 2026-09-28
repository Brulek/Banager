import { invoke, Channel, type InvokeArgs } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import type {
  ArtifactKey,
  IssuedPlan,
  OpRequest,
  PlanId,
  Settings,
  Snapshot,
  OpSummary,
  UiEvent,
  UnknownScan,
  UpdatePair,
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
    throw asError(e);
  });
}

/** A rejection from Tauri as an `Error` whose `.message` is the backend's text (`call`). */
function asError(e: unknown): Error {
  return e instanceof Error ? e : new Error(typeof e === "string" ? e : JSON.stringify(e));
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

/**
 * Has Finder show `path` -- a Finder window on its folder, with it
 * selected -- for the Unknown page's Show in Finder. Through the opener
 * plugin's `revealItemInDir`, the one command of that plugin the window may
 * call (`opener:allow-reveal-item-in-dir` in
 * src-tauri/capabilities/default.json), which resolves `path` first, every
 * link followed, and then asks macOS for that and nothing else
 * (`NSWorkspace activateFileViewerSelectingURLs:`): no command runs.
 * Rejects, with the plugin's reason, when there is nothing at `path`.
 */
export async function revealInFinder(path: string): Promise<void> {
  try {
    await revealItemInDir(path);
  } catch (e) {
    throw asError(e);
  }
}

/**
 * The icon Finder shows for the app a Homebrew cask installed, as a
 * `data:image/png;base64,...` URL an `<img>` can show (the window's CSP
 * allows `data:` images), or null: not a cask, no app, or no icon. Only
 * the key is sent -- the Rust side finds the row in its own snapshot and
 * draws the `.app` Homebrew reported for it, never a path from here.
 * `useArtifactIcon` is the caller.
 */
export function artifactIcon(key: ArtifactKey): Promise<string | null> {
  return call<string | null>("artifact_icon", { key });
}

/**
 * The languages the menu bar is written in: the window's two, by the names
 * its i18n gives them (`MenuLanguage` in src-tauri/src/menu.rs).
 */
export type MenuLanguage = "en" | "zh-CN";

/**
 * Tells Rust the language the window uses, which the menu bar follows: it
 * is built again in that language, or left as it is when it is in it
 * already. `useLanguageSync` sends it.
 */
export function setMenuLanguage(language: MenuLanguage): Promise<void> {
  return call<void>("set_menu_language", { language });
}

/**
 * The event Rust sends the window when an item of the menu bar that acts in
 * the page is chosen, by what the page does for it: Settings… (⌘,), Check
 * Again (⌘R), Search (⌘F). `PageCommand` in src-tauri/src/menu.rs sends
 * these three.
 */
export const MENU_EVENTS = {
  settings: "menu://settings",
  checkAgain: "menu://check-again",
  search: "menu://search",
} as const;

export type MenuCommand = keyof typeof MENU_EVENTS;

/**
 * Calls `onCommand` each time one of those items is chosen, and resolves to
 * what stops that once the window listens for all three. If one cannot be
 * listened for, those that could are stopped again and this rejects.
 * `useMenuCommands` is the caller.
 */
export async function onMenuCommand(onCommand: (command: MenuCommand) => void): Promise<() => void> {
  const commands = Object.keys(MENU_EVENTS) as MenuCommand[];
  const settled = await Promise.allSettled(
    commands.map((command) => listen(MENU_EVENTS[command], () => onCommand(command))),
  );
  const unlisteners = settled.flatMap((result) => (result.status === "fulfilled" ? [result.value] : []));
  const stop = () => {
    for (const unlisten of unlisteners) unlisten();
  };
  const failed = settled.find((result): result is PromiseRejectedResult => result.status === "rejected");
  if (failed !== undefined) {
    stop();
    throw asError(failed.reason);
  }
  return stop;
}

/**
 * Puts `count` on Canager's icon in the Dock, as the App Store puts there
 * the number of updates it has, or takes the badge away at 0. Through
 * Tauri's `setBadgeCount` (`core:window:allow-set-badge-count` in
 * src-tauri/capabilities/default.json), which on macOS badges the app, not
 * the window that asks, by writing the number into the Dock tile's badge
 * label as text -- so it would show a 0, and 0 goes as no count at all,
 * which clears the label. `useDockBadge` is the caller.
 */
export async function setDockBadge(count: number): Promise<void> {
  try {
    await getCurrentWindow().setBadgeCount(count > 0 ? count : undefined);
  } catch (e) {
    throw asError(e);
  }
}

/**
 * Reports the updates Update all would take, as (row, version) pairs, with
 * the round of the snapshot they came from (`Snapshot.round`): the
 * update notification's report (`report_update_set` in
 * src-tauri/src/notify.rs), which decides there whether a notification
 * goes out. `useUpdateNotification` sends one after each snapshot.
 */
export function reportUpdateSet(round: number, updates: UpdatePair[]): Promise<void> {
  return call<void>("report_update_set", { round, updates });
}

/**
 * Asks for permission to post notifications, as Settings' 「有可更新时通知我」
 * is turned on (`request_notification_permission` in
 * src-tauri/src/notify.rs): true when it is granted.
 */
export function requestNotificationPermission(): Promise<boolean> {
  return call<boolean>("request_notification_permission");
}

/**
 * The event Rust sends the window when the update notification is
 * clicked, once the window is back on screen: `OPEN_UPDATES_EVENT` in
 * src-tauri/src/notify.rs, sent the way the menu bar's are.
 */
export const OPEN_UPDATES_EVENT = "notification://open-updates";

/**
 * Calls `onClick` each time the update notification is clicked, and
 * resolves to what stops that once the window listens.
 * `useUpdateNotification` is the caller.
 */
export async function onOpenUpdates(onClick: () => void): Promise<() => void> {
  try {
    return await listen(OPEN_UPDATES_EVENT, () => onClick());
  } catch (e) {
    throw asError(e);
  }
}
