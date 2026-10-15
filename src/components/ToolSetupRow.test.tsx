import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { ToolSetupRow } from "./ToolSetupRow";
import i18n from "../i18n";
import { useToolSetupSheet } from "../lib/toolSetupCheck";
import type { ManagerInstance, Snapshot, SystemFacts } from "../lib/types";

/**
 * I4 (decisions round, 2026-10-06): the Overview's 「工具环境」 row says
 * 「N项需要查看」, with the sheet's ⚠︎, when the sheet would open on N
 * warnings -- the words its own summary says -- and is the plain row
 * otherwise: never 「0项需要查看」, and nothing while the first check runs.
 */

const mockInvoke = vi.mocked(invoke);

function instance(id: string, adapterId: string, more: Partial<ManagerInstance> = {}): ManagerInstance {
  return {
    id,
    adapter_id: adapterId,
    exe_path: `/opt/homebrew/bin/${adapterId}`,
    prefix: "/opt/homebrew",
    scope: "User",
    version: "1.0.0",
    status: { unavailable: null, notes: [] },
    answered_at: null,
    unverified_version: null,
    read_only_reason: null,
    ...more,
  };
}

const FACTS: SystemFacts = {
  macos_version: "27.0",
  chip: "Apple M2 Pro",
  arch: "aarch64",
  login_path: true,
  path_dirs: ["/opt/homebrew/bin", "/usr/bin"],
  sources: [],
  path_folders: { read: 2, unread: [] },
};

let served: Snapshot;
let facts: SystemFacts | null;

function snapshotWith(more: Partial<Snapshot> = {}): Snapshot {
  return {
    generation: 5,
    round: 5,
    detect: "Found",
    instances: [instance("brew:/opt/homebrew", "brew")],
    artifacts: [],
    updates: [],
    refreshed_at: 1790586000,
    stale: false,
    errors: [],
    ...more,
  };
}

beforeEach(() => {
  served = snapshotWith();
  facts = FACTS;
  mockInvoke.mockReset();
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "get_snapshot") return Promise.resolve(served);
    if (cmd === "get_system_facts") return Promise.resolve(facts);
    if (cmd === "get_settings") return Promise.resolve(undefined);
    return Promise.resolve(undefined);
  });
});

function rowLine(): string | null | undefined {
  return document.querySelector("[data-overview-tool-setup] [data-setup-row-line]")?.textContent;
}

describe("the Overview's 工具环境 row", () => {
  it("is the plain row while there is nothing to look at", async () => {
    renderWithProviders(<ToolSetupRow />);
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("get_system_facts"));
    await waitFor(() => expect(rowLine()).toBe("Whether Terminal finds your tools, and how each source is doing."));
    expect(document.querySelector("[data-overview-tool-setup] svg.text-warning")).toBeNull();
  });

  it("says how many items need attention, with the sheet's ⚠︎, in every language", async () => {
    served = snapshotWith({
      instances: [
        instance("brew:/opt/homebrew", "brew", { status: { unavailable: "NotResponding", notes: [] } }),
        instance("ollama:http://127.0.0.1:11434", "ollama", { status: { unavailable: "NotRunning", notes: [] } }),
      ],
    });
    renderWithProviders(<ToolSetupRow />);
    await waitFor(() => expect(rowLine()).toBe("2 items need attention"));
    expect(document.querySelector("[data-overview-tool-setup] svg.text-warning")).not.toBeNull();
    // The button says what it opens, and the count with it.
    expect(screen.getByRole("button", { name: "Check Tool Setup…" })).toHaveAccessibleDescription(
      "2 items need attention",
    );
    await i18n.changeLanguage("zh-CN");
    try {
      await waitFor(() => expect(rowLine()).toBe("2项需要查看"));
      await i18n.changeLanguage("zh-Hant");
      await waitFor(() => expect(rowLine()).toBe("2項需要查看"));
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("counts Terminal's settings that could not be read, as the sheet does", async () => {
    facts = { ...FACTS, login_path: false };
    renderWithProviders(<ToolSetupRow />);
    await waitFor(() => expect(rowLine()).toBe("1 item needs attention"));
  });

  it("says nothing of it before the first check has answered", async () => {
    served = {
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
    facts = null;
    renderWithProviders(<ToolSetupRow />);
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("get_system_facts"));
    await waitFor(() => expect(rowLine()).toBe("Whether Terminal finds your tools, and how each source is doing."));
  });

  it("opens the sheet", async () => {
    renderWithProviders(<ToolSetupRow />);
    fireEvent.click(await screen.findByRole("button", { name: "Check Tool Setup…" }));
    expect(useToolSetupSheet.getState().open).toBe(true);
    useToolSetupSheet.setState({ open: false });
  });
});
