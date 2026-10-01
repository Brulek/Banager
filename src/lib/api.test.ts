import { describe, expect, it, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import capability from "../../src-tauri/capabilities/default.json";
import {
  getSnapshot,
  refresh,
  planOperation,
  submitOperation,
  cancelOperation,
  listOperations,
  getSettings,
  setSettings,
  subscribeEvents,
  scanUnknown,
  artifactIcon,
  getSizes,
  setMenuLanguage,
  onMenuCommand,
  type MenuCommand,
  setDockBadge,
  revealInFinder,
  reportUpdateSet,
  requestNotificationPermission,
  OPEN_UPDATES_EVENT,
  onOpenUpdates,
  QUIT_REQUESTED_EVENT,
  onQuitRequested,
  askBeforeQuit,
  quitQuestionShown,
  quitAnyway,
} from "./api";
import type { ArtifactKey, IssuedPlan, OpRequest, Settings, Sizes, UiEvent, UnknownScan } from "./types";
import { watchDock } from "../test/dock";

const mockInvoke = vi.mocked(invoke);

beforeEach(() => {
  mockInvoke.mockReset();
});

describe("api", () => {
  it("getSnapshot invokes get_snapshot with no args", async () => {
    mockInvoke.mockResolvedValueOnce({} as never);
    await getSnapshot();
    expect(mockInvoke).toHaveBeenCalledWith("get_snapshot");
  });

  it("turns a bare-string rejection from the backend into an Error carrying that string", async () => {
    // A `#[tauri::command]` returning `Result<_, String>` rejects with the
    // raw string; every error surface in the app renders `error.message`.
    mockInvoke.mockRejectedValueOnce("boom" as never);
    await expect(getSnapshot()).rejects.toThrow("boom");
  });

  it("refresh invokes refresh", async () => {
    mockInvoke.mockResolvedValueOnce({} as never);
    await refresh();
    expect(mockInvoke).toHaveBeenCalledWith("refresh");
  });

  it("planOperation invokes plan_operation with the request and returns the IssuedPlan", async () => {
    const request: OpRequest = {
      kind: "Uninstall",
      instance_id: "brew:/opt/homebrew",
      artifact_kind: "Formula",
      name: "jq",
    };
    const issued: IssuedPlan = {
      id: "a1b2c3",
      plan: {
        request,
        action: {
          Command: {
            program: "/opt/homebrew/bin/brew",
            args: ["uninstall", "--formula", "jq"],
            env: [],
          },
        },
        needs_password: false,
        locks: ["brew:/opt/homebrew"],
        cancel_policy: "KillThenReconcile",
        warnings: [],
        affected: [],
        timeout_secs: 1800,
      },
      issued_at: 1758000000,
    };
    mockInvoke.mockResolvedValueOnce(issued as never);
    const result = await planOperation(request);
    expect(mockInvoke).toHaveBeenCalledWith("plan_operation", { request });
    expect(result).toEqual(issued);
  });

  it("submitOperation invokes submit_operation with only the plan id", async () => {
    mockInvoke.mockResolvedValueOnce(7 as never);
    await submitOperation("a1b2c3");
    expect(mockInvoke).toHaveBeenCalledWith("submit_operation", { planId: "a1b2c3" });
  });

  it("cancelOperation invokes cancel_operation with opId", async () => {
    mockInvoke.mockResolvedValueOnce(undefined as never);
    await cancelOperation(7);
    expect(mockInvoke).toHaveBeenCalledWith("cancel_operation", { opId: 7 });
  });

  it("listOperations invokes list_operations", async () => {
    mockInvoke.mockResolvedValueOnce([] as never);
    await listOperations();
    expect(mockInvoke).toHaveBeenCalledWith("list_operations");
  });

  it("getSettings invokes get_settings", async () => {
    mockInvoke.mockResolvedValueOnce({} as never);
    await getSettings();
    expect(mockInvoke).toHaveBeenCalledWith("get_settings");
  });

  it("setSettings invokes set_settings with the settings", async () => {
    const settings: Settings = {
      language: "System",
      show_technical_details: false,
      ignored_updates: [],
      skipped_versions: [],
      include_self_updating: false,
      auto_check: false,
      notify_updates: false,
    };
    mockInvoke.mockResolvedValueOnce(undefined as never);
    await setSettings(settings);
    expect(mockInvoke).toHaveBeenCalledWith("set_settings", { settings });
  });

  it("subscribeEvents registers a Channel and forwards messages to the callback", async () => {
    mockInvoke.mockResolvedValueOnce(undefined as never);
    const received: UiEvent[] = [];
    await subscribeEvents((event) => {
      received.push(event);
    });

    expect(mockInvoke).toHaveBeenCalledWith(
      "subscribe_events",
      expect.objectContaining({ channel: expect.anything() }),
    );
    const channelArg = mockInvoke.mock.calls[0][1] as {
      channel: { onmessage: (e: UiEvent) => void };
    };
    channelArg.channel.onmessage({ SnapshotChanged: { generation: 3 } });

    expect(received).toEqual([{ SnapshotChanged: { generation: 3 } }]);
  });

  it("scanUnknown invokes scan_unknown with no args and returns the scan", async () => {
    const scan: UnknownScan = { scanned: [], entries: [], attributed: 0, stopped: null };
    mockInvoke.mockResolvedValueOnce(scan as never);
    const result = await scanUnknown();
    expect(mockInvoke).toHaveBeenCalledWith("scan_unknown");
    expect(result).toEqual(scan);
  });

  it("getSizes invokes get_sizes with no args and returns the sizes", async () => {
    const sizes: Sizes = { round: 2, done: true, artifacts: [], models: [], total: null, sources: [] };
    mockInvoke.mockResolvedValueOnce(sizes as never);
    expect(await getSizes()).toEqual(sizes);
    expect(mockInvoke.mock.calls).toEqual([["get_sizes"]]);
  });

  it("artifactIcon invokes artifact_icon with the key and nothing else, and returns its answer", async () => {
    // The trust boundary: a key the Rust side looks up in its own
    // snapshot, never a path -- not even the row's own `path`.
    const key: ArtifactKey = { instance_id: "brew:/opt/homebrew", kind: "Cask", name: "iterm2" };
    mockInvoke.mockResolvedValueOnce("data:image/png;base64,iVBORw0KGgo=" as never);
    expect(await artifactIcon(key)).toBe("data:image/png;base64,iVBORw0KGgo=");
    expect(mockInvoke).toHaveBeenCalledTimes(1);
    expect(mockInvoke.mock.calls[0]).toEqual(["artifact_icon", { key }]);
    expect(Object.keys(mockInvoke.mock.calls[0][1] as object)).toEqual(["key"]);

    mockInvoke.mockResolvedValueOnce(null as never);
    expect(await artifactIcon({ ...key, kind: "Formula", name: "git" })).toBeNull();
  });

  it("setMenuLanguage invokes set_menu_language with the language and nothing else", async () => {
    mockInvoke.mockResolvedValueOnce(undefined as never);
    await setMenuLanguage("zh-CN");
    expect(mockInvoke.mock.calls).toEqual([["set_menu_language", { language: "zh-CN" }]]);
  });

  it("reportUpdateSet invokes report_update_set with the round and the pairs, and nothing else", async () => {
    // `report_update_set(round, updates)` in src-tauri/src/notify.rs.
    mockInvoke.mockResolvedValueOnce(undefined as never);
    const updates = [{ key_id: "brew:/opt/homebrew|Formula|jq", target: "1.8.1" }];
    await reportUpdateSet(7, updates);
    expect(mockInvoke.mock.calls).toEqual([["report_update_set", { round: 7, updates }]]);
  });

  it("requestNotificationPermission invokes request_notification_permission and answers its yes or no", async () => {
    mockInvoke.mockResolvedValueOnce(true as never);
    expect(await requestNotificationPermission()).toBe(true);
    mockInvoke.mockResolvedValueOnce(false as never);
    expect(await requestNotificationPermission()).toBe(false);
    expect(mockInvoke.mock.calls).toEqual([["request_notification_permission"], ["request_notification_permission"]]);
  });
});

describe("the update notification's click", () => {
  const mockListen = vi.mocked(listen);

  beforeEach(() => {
    mockListen.mockReset();
  });

  it("is the event Rust sends, and calls back each time it comes", async () => {
    // `OPEN_UPDATES_EVENT` in src-tauri/src/notify.rs, whose test pins the
    // same string.
    expect(OPEN_UPDATES_EVENT).toBe("notification://open-updates");
    let heard: EventCallback<unknown> | undefined;
    let stopped = false;
    mockListen.mockImplementation(async (event, handler) => {
      expect(event).toBe(OPEN_UPDATES_EVENT);
      heard = handler as EventCallback<unknown>;
      return () => {
        stopped = true;
      };
    });
    let clicks = 0;
    const stop = await onOpenUpdates(() => {
      clicks += 1;
    });
    heard?.({ event: OPEN_UPDATES_EVENT, id: 1, payload: null });
    heard?.({ event: OPEN_UPDATES_EVENT, id: 2, payload: null });
    expect(clicks).toBe(2);
    stop();
    expect(stopped).toBe(true);
  });

  it("reports a failure to listen as an Error carrying Tauri's text", async () => {
    mockListen.mockRejectedValueOnce("event.listen not allowed");
    await expect(onOpenUpdates(() => {})).rejects.toThrow("event.listen not allowed");
  });
});

describe("the question before a quit", () => {
  const mockListen = vi.mocked(listen);

  beforeEach(() => {
    mockListen.mockReset();
  });

  it("is the event Rust sends, and calls back each time it comes, with the question's number", async () => {
    // `QUIT_REQUESTED_EVENT` in src-tauri/src/quit.rs, whose test pins the
    // same string; its payload is the number `QuitGuard::ask` gave.
    expect(QUIT_REQUESTED_EVENT).toBe("quit://requested");
    let heard: EventCallback<unknown> | undefined;
    let stopped = false;
    mockListen.mockImplementation(async (event, handler) => {
      expect(event).toBe(QUIT_REQUESTED_EVENT);
      heard = handler as EventCallback<unknown>;
      return () => {
        stopped = true;
      };
    });
    const asked: number[] = [];
    const stop = await onQuitRequested((question) => {
      asked.push(question);
    });
    heard?.({ event: QUIT_REQUESTED_EVENT, id: 1, payload: 1 });
    heard?.({ event: QUIT_REQUESTED_EVENT, id: 2, payload: 2 });
    expect(asked).toEqual([1, 2]);
    stop();
    expect(stopped).toBe(true);
  });

  it("reports a failure to listen as an Error carrying Tauri's text", async () => {
    mockListen.mockRejectedValueOnce("event.listen not allowed");
    await expect(onQuitRequested(() => {})).rejects.toThrow("event.listen not allowed");
  });

  it("askBeforeQuit invokes ask_before_quit with whether the page asks", async () => {
    // `ask_before_quit` in src-tauri/src/quit.rs, whose argument is `ask`.
    mockInvoke.mockResolvedValue(undefined as never);
    await askBeforeQuit(true);
    await askBeforeQuit(false);
    expect(mockInvoke.mock.calls).toEqual([
      ["ask_before_quit", { ask: true }],
      ["ask_before_quit", { ask: false }],
    ]);
  });

  it("quitQuestionShown invokes quit_question_shown with the question's number", async () => {
    // `quit_question_shown` in src-tauri/src/quit.rs, whose argument is
    // `question`.
    mockInvoke.mockResolvedValueOnce(undefined as never);
    await quitQuestionShown(3);
    expect(mockInvoke.mock.calls).toEqual([["quit_question_shown", { question: 3 }]]);
  });

  it("quitAnyway invokes quit_anyway with no args", async () => {
    // `quit_anyway` in src-tauri/src/quit.rs.
    mockInvoke.mockResolvedValueOnce(undefined as never);
    await quitAnyway();
    expect(mockInvoke.mock.calls).toEqual([["quit_anyway"]]);
  });
});

describe("the menu bar's events", () => {
  const mockListen = vi.mocked(listen);
  // Every event `PageCommand::event` in src-tauri/src/menu.rs sends, sorted:
  // spelled out, not read from MENU_EVENTS, so a name changed there alone fails.
  const ALL_MENU_EVENTS = [
    "menu://check-again",
    "menu://check-tool-setup",
    "menu://copy-diagnostics",
    "menu://installed",
    "menu://keyboard-shortcuts",
    "menu://overview",
    "menu://search",
    "menu://settings",
    "menu://unknown",
    "menu://updates",
    "menu://welcome",
  ];
  // What is listened for, and what stopped listening, by event name.
  let handlers: Map<string, EventCallback<unknown>>;
  let stopped: string[];

  beforeEach(() => {
    handlers = new Map();
    stopped = [];
    mockListen.mockReset();
    mockListen.mockImplementation(async (event, handler) => {
      handlers.set(event, handler as EventCallback<unknown>);
      return () => {
        stopped.push(event);
      };
    });
  });

  it("are the eleven Rust sends, one per item acting in the page, each calling back with its item", async () => {
    const chosen: MenuCommand[] = [];
    await onMenuCommand((command) => chosen.push(command));

    // `PageCommand::event` in src-tauri/src/menu.rs.
    expect([...handlers.keys()].sort()).toEqual(ALL_MENU_EVENTS);
    for (const event of [
      "menu://search",
      "menu://settings",
      "menu://overview",
      "menu://updates",
      "menu://installed",
      "menu://unknown",
      "menu://check-again",
      "menu://welcome",
      "menu://keyboard-shortcuts",
      "menu://check-tool-setup",
      "menu://copy-diagnostics",
      "menu://search",
    ]) {
      handlers.get(event)?.({ event, id: 1, payload: null });
    }
    expect(chosen).toEqual([
      "search",
      "settings",
      "overview",
      "updates",
      "installed",
      "unknown",
      "checkAgain",
      "welcome",
      "keyboardShortcuts",
      "checkToolSetup",
      "copyDiagnostics",
      "search",
    ]);
  });

  it("stop being listened for, all eleven, through what onMenuCommand resolves to", async () => {
    const stop = await onMenuCommand(() => {});
    expect(stopped).toEqual([]);

    stop();

    expect(stopped.sort()).toEqual(ALL_MENU_EVENTS);
  });

  it("are not left half listened for: when one cannot be, the others stop and the error says why", async () => {
    mockListen.mockImplementation(async (event, handler) => {
      if (event === "menu://search") throw "event.listen not allowed";
      handlers.set(event, handler as EventCallback<unknown>);
      return () => {
        stopped.push(event);
      };
    });

    await expect(onMenuCommand(() => {})).rejects.toThrow("event.listen not allowed");
    expect(stopped.sort()).toEqual(ALL_MENU_EVENTS.filter((event) => event !== "menu://search"));
  });
});

describe("the Dock's badge", () => {
  it("is the count setDockBadge is given, set on the window, not through a command of Banager's", async () => {
    const dock = watchDock();
    await setDockBadge(12);
    expect(dock.counts()).toEqual([12]);
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  it("is taken away at 0, not shown as a 0", async () => {
    // Tauri writes a count into the Dock tile's badge label as text, a 0
    // included; no count at all is what clears the label.
    const dock = watchDock();
    await setDockBadge(3);
    await setDockBadge(0);
    expect(dock.counts()).toEqual([3, undefined]);
    expect(dock.badge()).toBeUndefined();
  });

  it("reports a failure as an Error carrying Tauri's text", async () => {
    watchDock();
    vi.mocked(getCurrentWindow().setBadgeCount).mockRejectedValueOnce(
      "window.set_badge_count not allowed",
    );
    await expect(setDockBadge(2)).rejects.toThrow("window.set_badge_count not allowed");
  });
});

describe("Show in Finder", () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  it("hands Banager's own command the path and nothing else", async () => {
    mockInvoke.mockResolvedValueOnce(undefined);
    await revealInFinder("/Applications/Helper.app/Contents/Helpers/helper-cli");
    expect(mockInvoke).toHaveBeenCalledTimes(1);
    expect(mockInvoke).toHaveBeenCalledWith("reveal_in_finder", {
      path: "/Applications/Helper.app/Contents/Helpers/helper-cli",
    });
  });

  it("reports a refusal as an Error carrying the backend's text", async () => {
    mockInvoke.mockRejectedValueOnce('{"kind":"not_revealable"}');
    await expect(revealInFinder("/Users/someone/Documents")).rejects.toThrow('{"kind":"not_revealable"}');
  });

  it("gives the window no command of the opener plugin's, which would show any path", () => {
    // `reveal_item_in_dir` has no scope to narrow it to some paths, and
    // `opener:default` would also let the page open a web address or a
    // mail link: src-tauri/src/reveal.rs shows only what the scan found.
    expect(capability.permissions.filter((p) => p.startsWith("opener:"))).toEqual([]);
  });
});

describe("the notification plugin", () => {
  it("gives the page one command, the one the plugin's own script calls as the page loads", () => {
    // tauri-plugin-notification's script asks whether notifications are
    // allowed as the page loads; refused, the call would end in an
    // unhandled rejection. Asking for permission and posting are
    // Banager's own commands (src-tauri/src/notify.rs), so the page can
    // post nothing itself: not `notification:default`, which would let it.
    expect(capability.permissions.filter((p) => p.startsWith("notification:"))).toEqual([
      "notification:allow-is-permission-granted",
    ]);
  });
});
