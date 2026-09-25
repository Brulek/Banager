import { describe, expect, it, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
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
} from "./api";
import type { IssuedPlan, OpRequest, Settings, UiEvent, UnknownScan } from "./types";

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
});
