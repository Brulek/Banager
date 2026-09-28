import { describe, expect, it, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { listen, type EventCallback } from "@tauri-apps/api/event";
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
  setMenuLanguage,
  onMenuCommand,
  type MenuCommand,
} from "./api";
import type { ArtifactKey, IssuedPlan, OpRequest, Settings, UiEvent, UnknownScan } from "./types";

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
});

describe("the menu bar's events", () => {
  const mockListen = vi.mocked(listen);
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

  it("are the three Rust sends, one per item acting in the page, each calling back with its item", async () => {
    const chosen: MenuCommand[] = [];
    await onMenuCommand((command) => chosen.push(command));

    // `PageCommand::event` in src-tauri/src/menu.rs.
    expect([...handlers.keys()].sort()).toEqual(["menu://check-again", "menu://search", "menu://settings"]);
    for (const event of ["menu://search", "menu://settings", "menu://check-again", "menu://search"]) {
      handlers.get(event)?.({ event, id: 1, payload: null });
    }
    expect(chosen).toEqual(["search", "settings", "checkAgain", "search"]);
  });

  it("stop being listened for, all three, through what onMenuCommand resolves to", async () => {
    const stop = await onMenuCommand(() => {});
    expect(stopped).toEqual([]);

    stop();

    expect(stopped.sort()).toEqual(["menu://check-again", "menu://search", "menu://settings"]);
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
    expect(stopped.sort()).toEqual(["menu://check-again", "menu://settings"]);
  });
});
