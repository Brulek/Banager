import { invoke, Channel, type InvokeArgs } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type {
  ArtifactKey,
  HistoryView,
  IssuedPlan,
  OpRequest,
  PlanId,
  Settings,
  Snapshot,
  Sizes,
  SystemFacts,
  OpSummary,
  UiEvent,
  UnknownScan,
  UpdatePair,
  FinishedRun,
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
 * backend drops a Channel from its broadcast registry once a send to it
 * fails (the window closed), or once newer ones fill it (`MAX_CHANNELS` in
 * src-tauri/src/events.rs). The returned function is a client-side detach:
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
 * selected -- for the Unknown page's Show in Finder, through Banager's own
 * `reveal_in_finder` (src-tauri/src/reveal.rs): only a path the newest
 * `scan_unknown` resolved (`UnknownEntry.resolved`) and that still leads,
 * with no link on its way and outside every protected place, to the file
 * the scan found there; refused otherwise as `not_revealable`. Then it
 * asks macOS for that path and nothing else (`NSWorkspace
 * activateFileViewerSelectingURLs:`): no command runs. The window is given
 * no command of the opener plugin's own, which would show any path at all.
 */
export function revealInFinder(path: string): Promise<void> {
  return call<void>("reveal_in_finder", { path });
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
 * How much disk each installed thing takes, as the newest round of
 * measuring says so far (`get_sizes` in src-tauri/src/ipc.rs): measured on
 * the Rust side after each refresh, outside the snapshot. Takes nothing;
 * `SizesChanged` says when to ask again. `useSizes` is the caller.
 */
export function getSizes(): Promise<Sizes> {
  return call<Sizes>("get_sizes");
}

/**
 * What 「拷贝诊断信息」 needs and the window cannot read itself
 * (`get_system_facts` in src-tauri/src/ipc.rs): macOS's version, the chip,
 * whether `PATH` is the login shell's and its folders, and each source's
 * program -- home folder as `~`. Takes nothing; runs no command.
 * `useSystemFacts` (src/lib/diagnostics.ts) is the caller.
 */
export function getSystemFacts(): Promise<SystemFacts> {
  return call<SystemFacts>("get_system_facts");
}

/**
 * The finished updates and uninstalls Banager kept across launches
 * (`get_history` in src-tauri/src/history.rs): this launch's id, when the
 * Updates page's Clear was last pressed, and every record, newest first.
 * Takes nothing. `useHistory` is the caller.
 */
export function getHistory(): Promise<HistoryView> {
  return call<HistoryView>("get_history");
}

/**
 * The Updates page's Clear, kept across launches (`clear_history`): the
 * page lists nothing that finished before now. Removes no record.
 */
export function clearHistory(): Promise<HistoryView> {
  return call<HistoryView>("clear_history");
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
 * the page is chosen, by what the page does for it: Settings… (⌘,); the
 * View menu's Overview (⌘1), Updates (⌘2), Installed (⌘3) and Other
 * Programs (⌘4); Check Again (⌘R); Search (⌘F); and Help's Welcome to
 * Banager, Common Questions, Keyboard Shortcuts, Check Tool Setup and Copy
 * Diagnostic Info. `PageCommand` in src-tauri/src/menu.rs sends these
 * twelve.
 */
export const MENU_EVENTS = {
  settings: "menu://settings",
  overview: "menu://overview",
  updates: "menu://updates",
  installed: "menu://installed",
  unknown: "menu://unknown",
  checkAgain: "menu://check-again",
  search: "menu://search",
  welcome: "menu://welcome",
  commonQuestions: "menu://common-questions",
  keyboardShortcuts: "menu://keyboard-shortcuts",
  checkToolSetup: "menu://check-tool-setup",
  copyDiagnostics: "menu://copy-diagnostics",
} as const;

export type MenuCommand = keyof typeof MENU_EVENTS;

/**
 * Calls `onCommand` each time one of those items is chosen, and resolves to
 * what stops that once the window listens for all of them. If one cannot be
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
 * Puts `count` on Banager's icon in the Dock, as the App Store puts there
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
 * The page's report of a run of operations that has finished, for the
 * notification when operations finish (`report_finished_run` in
 * src-tauri/src/notify_ops.rs), which decides there whether one goes out.
 * `useOperationsNotification` sends one as each run ends.
 */
export function reportFinishedRun(run: FinishedRun): Promise<void> {
  return call<void>("report_finished_run", { run });
}

/**
 * Asks for permission to post notifications, as Settings' 「有更新时通知我」
 * is turned on (`request_notification_permission` in
 * src-tauri/src/notify.rs): true when it is granted.
 */
export function requestNotificationPermission(): Promise<boolean> {
  return call<boolean>("request_notification_permission");
}

/**
 * The event Rust sends the window for the update notification, once the
 * window is back on screen: `OPEN_UPDATES_EVENT` in src-tauri/src/notify.rs,
 * sent the way the menu bar's are. On a Mac, Rust hears no click on the
 * notification itself (`post` there): it sends this when Banager comes to
 * the front -- as a click on it brings Banager -- with its window closed or
 * minimized while a notification waits on the window (`on_activate` in
 * src-tauri/src/window.rs).
 */
export const OPEN_UPDATES_EVENT = "notification://open-updates";

/**
 * Calls `onClick` each time Rust sends `OPEN_UPDATES_EVENT`, and resolves
 * to what stops that once the window listens. `useUpdateNotification` is
 * the caller.
 */
export async function onOpenUpdates(onClick: () => void): Promise<() => void> {
  try {
    return await listen(OPEN_UPDATES_EVENT, () => onClick());
  } catch (e) {
    throw asError(e);
  }
}

/**
 * The event Rust sends the window when a quit waits on the page's question:
 * `QUIT_REQUESTED_EVENT` in src-tauri/src/quit.rs, sent the way the menu
 * bar's are, once the window is back on screen. On a Mac every way of
 * quitting reaches it -- Quit Banager (⌘Q), the Dock's Quit, a logout --
 * while an operation is not done, once the page has said it listens
 * (`askBeforeQuit`). Its payload is the question's number, which the page
 * hands back once the question is on screen (`quitQuestionShown`).
 */
export const QUIT_REQUESTED_EVENT = "quit://requested";

/**
 * Calls `onQuit` with the question's number each time Rust sends
 * `QUIT_REQUESTED_EVENT`, and resolves to what stops that once the window
 * listens. `useQuitRequests` is the caller.
 */
export async function onQuitRequested(onQuit: (question: number) => void): Promise<() => void> {
  try {
    return await listen<number>(QUIT_REQUESTED_EVENT, (event) => onQuit(event.payload));
  } catch (e) {
    throw asError(e);
  }
}

/**
 * Tells Rust whether the page listens for `QUIT_REQUESTED_EVENT`
 * (`ask_before_quit` in src-tauri/src/quit.rs): `true` once it does, and
 * from then on a quit asks first while an operation is not done; `false`
 * as it stops, and a quit quits at once again, as it would with no page
 * to ask. `useQuitRequests` sends both.
 */
export function askBeforeQuit(ask: boolean): Promise<void> {
  return call<void>("ask_before_quit", { ask });
}

/**
 * Tells Rust that question `question` (`QUIT_REQUESTED_EVENT`'s payload) is
 * on screen, so that Banager waits for the user's answer
 * (`quit_question_shown` in src-tauri/src/quit.rs). Without it, Banager
 * quits 2 seconds after asking: nobody is there to answer. `QuitQuestion`
 * sends it.
 */
export function quitQuestionShown(question: number): Promise<void> {
  return call<void>("quit_question_shown", { question });
}

/**
 * The user answered question `question` 「取消」 (or Escape), or the
 * sheet went by itself, everything having finished: Banager does not quit
 * 2 seconds after asking, even when `quitQuestionShown` did not get through
 * (`quit_kept_waiting` in src-tauri/src/quit.rs). `QuitQuestion` sends it.
 */
export function quitKeptWaiting(question: number): Promise<void> {
  return call<void>("quit_kept_waiting", { question });
}

/**
 * 「退出」: Banager cancels what can be cancelled, waits for it to stop,
 * and quits (`quit_anyway` in src-tauri/src/quit.rs); the promise settles
 * only if Banager is still there to answer.
 */
export function quitAnyway(): Promise<void> {
  return call<void>("quit_anyway");
}
