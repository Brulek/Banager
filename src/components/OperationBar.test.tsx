import { describe, expect, it, vi, beforeEach } from "vitest";
import { act, fireEvent, waitFor, within } from "@testing-library/react";
import type { QueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { OperationBar } from "./OperationBar";
import { BatchUninstallResult } from "./BatchUninstallResult";
import { BUTTON } from "./ui/controls";
import { useUiStore } from "../store/ui";
import { queryKeys } from "../lib/queryKeys";
import i18n from "../i18n";
import type { OpStatus, OpSummary, Outcome, Snapshot } from "../lib/types";
import { NO_FACTS } from "../lib/types";
import { failureCause } from "../lib/failureCause";

const mockInvoke = vi.mocked(invoke);

// What `list_operations` answers, newest first as the backend lists them,
// and what `get_snapshot` answers -- nothing, until a test gives it one:
// the bar only reads it for the names the rows show.
let operations: OpSummary[];
let snapshot: Snapshot | null;
// What `get_settings` answers: Show technical details off, unless a test turns it on.
let technical: boolean;

function op(
  id: number,
  name: string,
  status: OpStatus,
  outcome: Outcome | null = null,
  extra: Partial<OpSummary> = {},
): OpSummary {
  return {
    id,
    kind: "Upgrade",
    instance_id: "brew:/opt/homebrew",
    artifact_kind: "Formula",
    name,
    status,
    outcome,
    argv_preview: ["/opt/homebrew/bin/brew", "upgrade", "--formula", name],
    cancel_policy: "KillThenReconcile",
    ...extra,
  };
}

beforeEach(() => {
  operations = [];
  snapshot = null;
  technical = false;
  mockInvoke.mockReset();
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "list_operations") return Promise.resolve(operations);
    if (cmd === "get_settings") {
      return Promise.resolve({
        language: "System",
        show_technical_details: technical,
        ignored_updates: [],
        skipped_versions: [],
        include_self_updating: false,
        auto_check: false,
        notify_updates: false,
      });
    }
    // Never answered without a snapshot: an answer of nothing would be an
    // error to the query, which is not what these tests are about.
    if (cmd === "get_snapshot") return snapshot === null ? new Promise(() => {}) : Promise.resolve(snapshot);
    return Promise.resolve(undefined);
  });
  useUiStore.setState({ drawerOpen: false, focusedOpId: null, logs: [], logRun: [] });
});

/** The ids `cancel_operation` was called with, in order. */
function calledToCancel(): number[] {
  return mockInvoke.mock.calls
    .filter(([cmd]) => cmd === "cancel_operation")
    .map(([, args]) => (args as { opId: number }).opId);
}

/** The backend's list has moved on: what a `Status` or `Finished` event makes the bar ask again. */
async function listNow(queryClient: QueryClient, next: OpSummary[]) {
  operations = next;
  await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.operations }));
}

describe("OperationBar", () => {
  it("is not there at all while nothing has run, so the window has its height back", async () => {
    const { container, queryClient, queryByText } = renderWithProviders(<OperationBar />);

    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));
    expect(container).toBeEmptyDOMElement();
    // Not even the sentence it used to say instead.
    expect(queryByText("No operation running")).toBeNull();
  });

  it("appears when an operation starts", async () => {
    const { container, queryClient, findByText, getByRole } = renderWithProviders(<OperationBar />);
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));
    expect(container).toBeEmptyDOMElement();

    await listNow(queryClient, [op(1, "wget", "Running")]);

    await findByText("wget: Updating…");
    const bar = getByRole("contentinfo", { name: "Operation status" });
    expect(within(bar).getByRole("button", { name: "Cancel" })).toBeEnabled();
    expect(within(bar).getByRole("button", { name: "View Log" })).toBeInTheDocument();
    // Nothing to close while something runs.
    expect(within(bar).queryByRole("button", { name: "Close" })).toBeNull();
  });

  it("shows the running operation and cancels it on click", async () => {
    operations = [op(5, "onyx", "Running", null, { artifact_kind: "Cask" })];

    const { findByRole, findByText } = renderWithProviders(<OperationBar />);
    await findByText("onyx: Updating…");
    fireEvent.click(await findByRole("button", { name: "Cancel" }));

    // This is the only proof in the plan that Cancel really reaches
    // cancel_operation; the mutation calls invoke after awaiting onMutate.
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith("cancel_operation", { opId: 5 }),
    );
  });

  it("opens the log of what it is running", async () => {
    operations = [op(5, "onyx", "Running")];

    const { findByRole } = renderWithProviders(<OperationBar />);
    fireEvent.click(await findByRole("button", { name: "View Log" }));

    expect(useUiStore.getState().focusedOpId).toBe(5);
    expect(useUiStore.getState().drawerOpen).toBe(true);
  });

  it("shows the wait for a brew update, not just \"running\", while one is in progress", async () => {
    // Item (2) of the loose-ends pass: `LogNote::WaitingForBrewUpdate` used
    // to reach only the log drawer, so this bar -- the only thing on
    // screen before the drawer is opened -- still said "running" while an
    // install waited on Homebrew for up to ten minutes.
    operations = [op(5, "onyx", "Running", null, { artifact_kind: "Cask" })];
    useUiStore.getState().appendLog({ opId: 5, note: { WaitingForBrewUpdate: { minutes: 10 } } });

    const { findByRole, findByText } = renderWithProviders(<OperationBar />);
    await findByText("onyx: Update · Waiting for Homebrew to check for new versions…");
    // Cancel must still work: the wait is still part of an active op.
    expect(await findByRole("button", { name: "Cancel" })).toBeEnabled();
  });

  it("says it in Chinese as the copy table has it: what it does, to what, then where it stands", async () => {
    operations = [op(5, "ffmpeg", "Running")];
    useUiStore.getState().appendLog({ opId: 5, note: { WaitingForBrewUpdate: { minutes: 10 } } });
    await i18n.changeLanguage("zh-CN");
    try {
      const { findByText } = renderWithProviders(<OperationBar />);
      await findByText("ffmpeg：更新 · 正在等待Homebrew查找新版本…");
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("says what it acts on, then what it does, never a bare verb in front: htop: Uninstalling…, then Uninstalled (walk-3 W3-3)", async () => {
    const failed: Outcome = { Failed: { exit_code: 1, summary: "Error: something went wrong", cause: failureCause("Error: something went wrong") } };
    const cases: [OpSummary, string, string][] = [
      [op(1, "htop", "Running", null, { kind: "Uninstall" }), "htop: Uninstalling…", "htop：正在卸载…"],
      [op(1, "htop", "Done", "Succeeded", { kind: "Uninstall" }), "htop: Uninstalled", "htop：已卸载"],
      [op(1, "htop", "Done", failed, { kind: "Uninstall" }), "htop: Couldn't uninstall", "htop：未能卸载"],
      [op(1, "jq", "Running", null, { kind: "Install" }), "jq: Installing…", "jq：正在安装…"],
      [op(1, "jq", "Done", "Succeeded", { kind: "Install" }), "jq: Installed", "jq：已安装"],
      [op(1, "jq", "Done", failed, { kind: "Install" }), "jq: Couldn't install", "jq：未能安装"],
      [op(1, "git", "Running"), "git: Updating…", "git：正在更新…"],
      [op(1, "git", "Done", "Succeeded"), "git: Updated", "git：已更新"],
      [op(1, "git", "Done", failed), "git: Couldn't update", "git：未能更新"],
    ];
    for (const [operation, english, chinese] of cases) {
      operations = [operation];
      const en = renderWithProviders(<OperationBar />);
      expect(await en.findByText(english)).toBeInTheDocument();
      en.unmount();
      await i18n.changeLanguage("zh-CN");
      try {
        const zh = renderWithProviders(<OperationBar />);
        expect(await zh.findByText(chinese)).toBeInTheDocument();
        zh.unmount();
      } finally {
        await i18n.changeLanguage("en");
      }
    }
  });

  it("says what it does in front wherever the words don't, in every other state (walk-3 review 1.1)", async () => {
    const network: Outcome = { Failed: { exit_code: 1, summary: "curl: (6) Could not resolve host: ghcr.io", cause: failureCause("curl: (6) Could not resolve host: ghcr.io") } };
    const plain: Outcome = { Failed: { exit_code: 1, summary: "Error: something went wrong", cause: failureCause("Error: something went wrong") } };
    const unchanged: Outcome = { NeedsAttention: "UnchangedAfterUpgrade" };
    const cases: [OpSummary, boolean, string, string][] = [
      [op(1, "htop", "Queued", null, { kind: "Uninstall" }), false, "htop: Uninstall · Queued", "htop：卸载 · 排队中"],
      [op(1, "git", "Cancelling"), false, "git: Update · Cancelling…", "git：更新 · 正在取消…"],
      [op(1, "git", "Verifying"), false, "git: Update · Checking the result…", "git：更新 · 正在核对结果…"],
      [op(1, "git", "Done", network), false, "git: Update · Connection failed", "git：更新 · 网络连接失败"],
      [op(1, "htop", "Done", "Cancelled", { kind: "Uninstall" }), false, "htop: Uninstall · Cancelled", "htop：卸载 · 已取消"],
      [op(1, "jq", "Done", "Unconfirmed", { kind: "Install" }), false, "jq: Install · Result unconfirmed", "jq：安装 · 结果未确认"],
      [
        op(1, "git", "Done", plain),
        true,
        "git: Update · Couldn't finish: Error: something went wrong",
        "git：更新 · 未能完成：Error: something went wrong",
      ],
      [
        op(1, "htop", "Done", { BanagerFailed: "Panicked" }, { kind: "Uninstall" }),
        false,
        "htop: Uninstall · Couldn't finish because of an internal error",
        "htop：卸载 · 未能完成：发生内部错误",
      ],
      // Words that say it already stand alone.
      [op(1, "git", "Done", unchanged), false, "git: Update reported success, but the version didn't change", "git：显示已更新，但版本没有变化"],
      [op(1, "git", "Done", plain), false, "git: Couldn't update", "git：未能更新"],
    ];
    try {
      for (const [operation, tech, english, chinese] of cases) {
        operations = [operation];
        technical = tech;
        const en = renderWithProviders(<OperationBar />);
        expect(await en.findByText(english)).toBeInTheDocument();
        en.unmount();
        await i18n.changeLanguage("zh-CN");
        try {
          const zh = renderWithProviders(<OperationBar />);
          expect(await zh.findByText(chinese)).toBeInTheDocument();
          zh.unmount();
        } finally {
          await i18n.changeLanguage("en");
        }
      }
    } finally {
      technical = false;
    }
  });

  it("shows cancelling, not the brew-update wait, once Cancel is pressed during the wait", async () => {
    // F5 fix: `cancel()` (ops/mod.rs) moves the record straight to
    // CancelRequested when the user presses Cancel, without waiting for
    // `wait_for_update` to notice -- so the last log line an op carries
    // can still be `WaitingForBrewUpdate` even though the op is no longer
    // just waiting. The bar must say "cancelling", not repeat the stale
    // wait note, once status has moved off Running.
    operations = [op(5, "onyx", "CancelRequested", null, { artifact_kind: "Cask" })];
    useUiStore.getState().appendLog({ opId: 5, note: { WaitingForBrewUpdate: { minutes: 10 } } });

    const { findByText, queryByText, getByRole } = renderWithProviders(<OperationBar />);
    await findByText("onyx: Update · Cancelling…");
    expect(queryByText("onyx: Update · Waiting for Homebrew to check for new versions…")).not.toBeInTheDocument();
    // A cancel already on its way: the button stays, and cannot be pressed twice.
    expect(getByRole("button", { name: "Cancel" })).toBeDisabled();
  });

  it("keeps a finished operation visible with its outcome in place of its status, and no Cancel button", async () => {
    operations = [
      op(6, "jqq", "Done", { Failed: { exit_code: 1, summary: "No available formula with the name \"jqq\"", cause: failureCause("No available formula with the name \"jqq\"") } }, {
        kind: "Install",
      }),
    ];

    const { findByText, queryByRole, getByRole, queryByText } = renderWithProviders(<OperationBar />);

    // The tool's own words only with Show technical details on (see
    // below); its log has them.
    await findByText("jqq: Couldn't install");
    expect(queryByText(/No available formula/)).toBeNull();
    expect(queryByRole("button", { name: "Cancel" })).not.toBeInTheDocument();
    // Never the old "Installing jqq — done" beside how it went.
    expect(queryByText(/done/i)).toBeNull();
    // A failure is one to look at: its log, and a way to close the bar.
    fireEvent.click(getByRole("button", { name: "View Log" }));
    expect(useUiStore.getState().focusedOpId).toBe(6);
    expect(useUiStore.getState().drawerOpen).toBe(true);
    expect(getByRole("button", { name: "Close" })).toBeInTheDocument();
  });

  it("is a Mac window's status bar: 28 high, the window's background, a hairline over it, 11 muted words", async () => {
    operations = [op(1, "wget", "Running")];
    const { findByRole } = renderWithProviders(<OperationBar />);

    const bar = await findByRole("contentinfo", { name: "Operation status" });
    for (const look of ["h-7", "bg-content", "border-t", "border-separator", "text-small", "text-muted"]) {
      expect(bar).toHaveClass(look);
    }
    expect(bar.className).not.toMatch(/bg-surface|text-body/);
    // Its buttons small and grey.
    for (const button of within(bar).getAllByRole("button")) expect(button.className).toBe(BUTTON.small.grey);
  });

  it("shows a run's progress as a 4 by 60 capsule, the accent over the fill", async () => {
    const { findByText, getByRole, queryClient } = renderWithProviders(<OperationBar />);
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));
    await listNow(queryClient, [op(3, "wget", "Queued"), op(2, "jq", "Queued"), op(1, "git", "Running")]);
    await listNow(queryClient, [op(3, "wget", "Queued"), op(2, "jq", "Running"), op(1, "git", "Done", "Succeeded")]);

    await findByText("Working on 2 of 3");
    const track = getByRole("contentinfo").querySelector("[data-run-progress]") as HTMLElement;
    expect(track).toHaveClass("h-1", "w-15", "rounded-full", "bg-fill");
    const done = track.firstElementChild as HTMLElement;
    expect(done).toHaveClass("bg-accent");
    expect(done.style.width).toBe(`${(1 / 3) * 100}%`);
  });

  it("says why an update failed where the tool's own words say, and not the words themselves", async () => {
    const failed = op(9, "git", "Done", {
      Failed: { exit_code: 1, summary: 'curl: (6) Could not resolve host: ghcr.io\nError: git: Failed to download resource "git (2.55.1)"', cause: failureCause('curl: (6) Could not resolve host: ghcr.io\nError: git: Failed to download resource "git (2.55.1)"') },
    });
    operations = [failed];
    const { findByText, queryByText, getByRole } = renderWithProviders(<OperationBar />);

    await findByText("git: Update · Connection failed");
    expect(queryByText(/ghcr\.io|Failed to download/)).toBeNull();
    expect(getByRole("button", { name: "View Log" })).toBeInTheDocument();
  });

  it("shows a program's own words only with Show technical details on", async () => {
    technical = true;
    operations = [
      op(6, "jqq", "Done", { Failed: { exit_code: 1, summary: 'No available formula with the name "jqq"', cause: failureCause('No available formula with the name "jqq"') } }, { kind: "Install" }),
    ];
    const tech = renderWithProviders(<OperationBar />);
    await tech.findByText('jqq: Install · Couldn\'t finish: No available formula with the name "jqq"');
    tech.unmount();

    // macOS's own reason a program would not start, likewise.
    operations = [op(7, "wget", "Done", { BanagerFailed: { SpawnFailed: { detail: "Permission denied (os error 13)" } } })];
    const on = renderWithProviders(<OperationBar />);
    await on.findByText(/os error 13/);
    on.unmount();

    technical = false;
    const off = renderWithProviders(<OperationBar />);
    await off.findByText("wget: Update · Couldn't start the program. Nothing changed");
    expect(off.queryByText(/os error 13/)).toBeNull();
  });

  it("names a failure as a failure beside one that only asks to be checked, never both as needing attention", async () => {
    const { findByText, queryClient } = renderWithProviders(<OperationBar />);
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));
    await listNow(queryClient, [op(2, "jq", "Queued"), op(1, "git", "Running")]);
    await listNow(queryClient, [
      op(2, "jq", "Done", { NeedsAttention: "UnchangedAfterUpgrade" }),
      op(1, "git", "Done", { Failed: { exit_code: 1, summary: "Error: git is pinned", cause: failureCause("Error: git is pinned") } }),
    ]);
    await findByText("1 update failed, 1 needs attention");
    await act(async () => {
      await i18n.changeLanguage("zh-CN");
    });
    await findByText("1个更新失败，1个需要查看");
    await act(async () => {
      await i18n.changeLanguage("en");
    });
  });

  it("keeps 需要查看 for a run whose endings only ask to be checked", async () => {
    const { findByText, getByRole, queryClient } = renderWithProviders(<OperationBar />);
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));
    await listNow(queryClient, [op(3, "wget", "Queued"), op(2, "jq", "Queued"), op(1, "git", "Running")]);
    await listNow(queryClient, [
      op(3, "wget", "Done", "Succeeded"),
      op(2, "jq", "Done", { NeedsAttention: "UnchangedAfterUpgrade" }),
      op(1, "git", "Done", "Unconfirmed"),
    ]);
    await findByText("2 of 3 need attention");
    // Orange, not red: nothing in it failed.
    expect(getByRole("img", { name: "Needs attention" })).toBeInTheDocument();
    await act(async () => {
      await i18n.changeLanguage("zh-CN");
    });
    await findByText("3个中有2个需要查看");
    await act(async () => {
      await i18n.changeLanguage("en");
    });
  });

  it("says the cancelled apart from the ones to check after Cancel All during an Update All (r35 U1)", async () => {
    // Update All of 13 on three sources; Cancel All while the first three
    // ran: those end Unconfirmed, the ten still waiting their turn Cancelled.
    const names = ["aider-chat", "platform-tools", "claude", "gemini-cli", "gh", "git", "grok", "httpie", "jq", "ruff", "rustup", "tokei", "wget"];
    const running = new Set([0, 1, 2]);
    const { findByText, getByRole, queryByText, queryClient } = renderWithProviders(<OperationBar />);
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));
    await listNow(queryClient, names.map((name, index) => op(13 - index, name, running.has(index) ? "Running" : "Queued")));
    await findByText(/Working on/);
    for (const index of running) {
      useUiStore.getState().appendLog({ opId: 13 - index, stream: "Stdout", line: `==> Upgrading ${names[index]}` });
    }
    await listNow(
      queryClient,
      names.map((name, index) => op(13 - index, name, "Done", running.has(index) ? "Unconfirmed" : "Cancelled")),
    );
    await findByText("3 need attention, 10 cancelled");
    // Not as though the other ten were fine.
    expect(queryByText("3 of 13 need attention")).toBeNull();
    expect(getByRole("img", { name: "Needs attention" })).toBeInTheDocument();
    // The three to check, as before.
    fireEvent.click(getByRole("button", { name: "View 3 Logs" }));
    expect(useUiStore.getState().logRun).toEqual([11, 12, 13]);
    await act(async () => {
      await i18n.changeLanguage("zh-CN");
    });
    await findByText("3个需要查看，10个已取消");
    expect(queryByText("13个中有3个需要查看")).toBeNull();
    await act(async () => {
      await i18n.changeLanguage("zh-Hant");
    });
    await findByText("3個需要查看，10個已取消");
    await act(async () => {
      await i18n.changeLanguage("en");
    });
  });

  it("names what worked too, and a success with a warning among the ones to check, beside the cancelled (r35 U1)", async () => {
    const { findByText, queryClient } = renderWithProviders(<OperationBar />);
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));
    await listNow(queryClient, [op(4, "wget", "Queued"), op(3, "jq", "Queued"), op(2, "node@22", "Running"), op(1, "git", "Running")]);
    await listNow(queryClient, [
      op(4, "wget", "Done", "Cancelled"),
      op(3, "jq", "Done", "Succeeded"),
      op(2, "node@22", "Done", "Succeeded", {
        follow_up_warnings: [{ NoLongerLinked: { name: "node@22", commands: ["node", "npm"] } }],
      }),
      op(1, "git", "Done", "Unconfirmed"),
    ]);
    await findByText("1 succeeded, 2 need attention, 1 cancelled");
  });

  it("counts the failures, the successes and the cancelled of a run that was not all one kind", async () => {
    const { findByText, queryClient } = renderWithProviders(<OperationBar />);
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));
    await listNow(queryClient, [
      op(4, "ripgrep", "Queued", null, { kind: "Install" }),
      op(3, "wget", "Queued"),
      op(2, "jq", "Queued"),
      op(1, "git", "Running"),
    ]);
    await listNow(queryClient, [
      op(4, "ripgrep", "Done", { Failed: { exit_code: 1, summary: "curl: (6) Could not resolve host: ghcr.io", cause: failureCause("curl: (6) Could not resolve host: ghcr.io") } }, { kind: "Install" }),
      op(3, "wget", "Done", "Cancelled"),
      op(2, "jq", "Done", { Failed: { exit_code: 1, summary: "Error: jq is pinned", cause: failureCause("Error: jq is pinned") } }),
      op(1, "git", "Done", "Succeeded"),
    ]);
    await findByText("2 failed, 1 succeeded, 1 cancelled");
    await act(async () => {
      await i18n.changeLanguage("zh-CN");
    });
    await findByText("2个失败，1个已成功，1个已取消");
    await act(async () => {
      await i18n.changeLanguage("en");
    });
  });

  it("says a run that was not all updates in words for any operation", async () => {
    const { findByText, queryClient } = renderWithProviders(<OperationBar />);
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));
    await listNow(queryClient, [op(2, "jq", "Queued", null, { kind: "Uninstall" }), op(1, "git", "Running")]);
    await listNow(queryClient, [op(2, "jq", "Done", "Succeeded", { kind: "Uninstall" }), op(1, "git", "Done", "Succeeded")]);
    await findByText("All 2 succeeded");
  });

  it("says a plain success with a tick and nothing to look at", async () => {
    operations = [op(7, "git", "Done", "Succeeded")];

    const { findByText, queryByRole, getByRole } = renderWithProviders(<OperationBar />);

    await findByText("git: Updated");
    expect(queryByRole("button", { name: "View Log" })).toBeNull();
    expect(queryByRole("img", { name: "Needs attention" })).toBeNull();
    expect(getByRole("button", { name: "Close" })).toBeInTheDocument();
  });

  it("marks an outcome that needs attention with the shared label, not a 需要留意 prefix, and offers its log", async () => {
    operations = [op(8, "git", "Done", { NeedsAttention: "UnchangedAfterUpgrade" })];

    const { findByText, getByRole } = renderWithProviders(<OperationBar />);

    await findByText("git: Update reported success, but the version didn't change");
    expect(getByRole("img", { name: "Needs attention" })).toBeInTheDocument();
    expect(getByRole("button", { name: "View Log" })).toBeInTheDocument();
  });

  it("says an update installed though a step after it failed, with its version, and offers its log (r35 U2)", async () => {
    operations = [op(9, "python@3.13", "Done", { NeedsAttention: { UpdatedButStepFailed: { version: "3.13.8", cause: null, detail: null } } })];

    const { findByText, getByRole, queryByText } = renderWithProviders(<OperationBar />);

    await findByText("python@3.13: Updated to 3.13.8, but a step after it failed; see the log");
    expect(getByRole("img", { name: "Needs attention" })).toBeInTheDocument();
    expect(getByRole("button", { name: "View Log" })).toBeInTheDocument();
    expect(queryByText(/Couldn't update/)).toBeNull();
    await act(async () => {
      await i18n.changeLanguage("zh-Hant");
    });
    await findByText("python@3.13：已更新到3.13.8，但之後有一步失敗了，請查看記錄");
    await act(async () => {
      await i18n.changeLanguage("en");
    });
  });

  it("says an update whose link step failed is installed but not linked, not only that a step failed (skeptic of r35 U2, 1)", async () => {
    operations = [
      op(9, "node@22", "Done", { NeedsAttention: { UpdatedButStepFailed: { version: "22.23.3_1", cause: "notLinked", detail: null } } }),
    ];

    const { findByText, getByRole, queryByText } = renderWithProviders(<OperationBar />);

    await findByText("node@22: Updated to 22.23.3_1, but the new version isn't linked; see the log");
    expect(getByRole("img", { name: "Needs attention" })).toBeInTheDocument();
    expect(queryByText(/a step after it failed/)).toBeNull();
    await act(async () => {
      await i18n.changeLanguage("zh-CN");
    });
    await findByText("node@22：已更新到22.23.3_1，但新版本没有链接到终端，请查看日志");
    await act(async () => {
      await i18n.changeLanguage("en");
    });
  });

  it("counts an update installed though a step after it failed as one to look at, not as a failure (r35 U2)", async () => {
    const { findByText, queryClient, queryByText } = renderWithProviders(<OperationBar />);
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));
    await listNow(queryClient, [op(3, "wget", "Queued"), op(2, "fontconfig", "Queued"), op(1, "git", "Running")]);
    await listNow(queryClient, [
      op(3, "wget", "Done", "Succeeded"),
      op(2, "fontconfig", "Done", { NeedsAttention: { UpdatedButStepFailed: { version: "2.18.4", cause: null, detail: null } } }),
      op(1, "git", "Done", "Succeeded"),
    ]);
    await findByText("1 of 3 needs attention");
    expect(queryByText(/failed/)).toBeNull();
  });

  it("closes with × once everything is done, and comes back for the next operation", async () => {
    operations = [op(7, "git", "Done", "Succeeded")];
    const { container, findByText, getByRole, queryClient } = renderWithProviders(<OperationBar />);
    await findByText("git: Updated");

    fireEvent.click(getByRole("button", { name: "Close" }));
    expect(container).toBeEmptyDOMElement();

    // The list asked again, with nothing new in it: still closed.
    await listNow(queryClient, [op(7, "git", "Done", "Succeeded")]);
    expect(container).toBeEmptyDOMElement();

    // A new operation brings it back, about the new one alone.
    await listNow(queryClient, [op(8, "wget", "Running"), op(7, "git", "Done", "Succeeded")]);
    await findByText("wget: Updating…");
    expect(container.textContent).not.toContain("git");
  });

  it("keeps View Log the same button as its operation ends, so the focus it hands back is never on Close", async () => {
    // The log, opened from the bar and closed after the update failed,
    // hands the focus back to the button that opened it. That node must
    // still be View Log: an unkeyed one became the finished bar's Close.
    operations = [op(9, "git", "Running")];
    const { findByText, findByRole, getByRole, queryClient } = renderWithProviders(<OperationBar />);
    await findByText("git: Updating…");
    const viewLog = getByRole("button", { name: "View Log" });

    await listNow(queryClient, [op(9, "git", "Done", { Failed: { exit_code: 1, summary: "", cause: failureCause("") } })]);
    const close = await findByRole("button", { name: "Close" });

    expect(viewLog).toBeInTheDocument();
    expect(viewLog).toHaveAccessibleName("View Log");
    expect(getByRole("button", { name: "View Log" })).toBe(viewLog);
    expect(close).not.toBe(viewLog);
  });

  it("shows a new run's first operation in place of the last run's result", async () => {
    operations = [op(7, "git", "Done", "Succeeded")];
    const { findByText, queryClient, queryByText } = renderWithProviders(<OperationBar />);
    await findByText("git: Updated");

    await listNow(queryClient, [op(8, "wget", "Running"), op(7, "git", "Done", "Succeeded")]);

    await findByText("wget: Updating…");
    expect(queryByText("git: Updated")).toBeNull();
    // One operation in this run, so no count.
    expect(queryByText(/Working on/)).toBeNull();
  });

  it("says how many run at once and how many wait, where several run together", async () => {
    const { findByText, queryByText, queryClient } = renderWithProviders(<OperationBar />);
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));
    await listNow(queryClient, [
      op(15, "wget", "Queued"),
      op(14, "jq", "Queued"),
      op(13, "ruff", "Running"),
      op(12, "prettier", "Running"),
      op(11, "git", "Running"),
    ]);
    await findByText("Working on 3 at once, 2 waiting");
    expect(queryByText(/Working on 1 of/)).toBeNull();
    await listNow(queryClient, [
      op(15, "wget", "Running"),
      op(14, "jq", "Running"),
      op(13, "ruff", "Done", "Succeeded"),
      op(12, "prettier", "Done", "Succeeded"),
      op(11, "git", "Done", "Succeeded"),
    ]);
    await findByText("Working on 2 at once");
    await act(async () => {
      await i18n.changeLanguage("zh-CN");
    });
    await findByText("正在同时处理2个");
    await act(async () => {
      await i18n.changeLanguage("en");
    });
  });

  it("counts a run of several as it goes, and says what it came to", async () => {
    // Update all: three operations started together, one after another on
    // Homebrew's lock.
    const { findByText, getByText, getByRole, queryByRole, queryClient } = renderWithProviders(<OperationBar />);
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));

    await listNow(queryClient, [op(13, "wget", "Queued"), op(12, "jq", "Queued"), op(11, "git", "Running")]);
    await findByText("Working on 1 of 3");
    // The one doing something; Cancel is for the whole run.
    expect(getByText("git: Updating…")).toBeInTheDocument();
    expect(getByRole("button", { name: "Cancel All" })).toBeEnabled();

    await listNow(queryClient, [
      op(13, "wget", "Queued"),
      op(12, "jq", "Running"),
      op(11, "git", "Done", "Succeeded"),
    ]);
    await findByText("Working on 2 of 3");
    expect(getByText("jq: Updating…")).toBeInTheDocument();

    await listNow(queryClient, [
      op(13, "wget", "Done", "Succeeded"),
      op(12, "jq", "Done", { Failed: { exit_code: 1, summary: "Error: jq is pinned", cause: failureCause("Error: jq is pinned") } }),
      op(11, "git", "Done", "Succeeded"),
    ]);
    // The failure said as one, with the ones that worked beside it.
    await findByText("1 update failed, 2 succeeded");
    expect(queryByRole("button", { name: /^Stop/ })).toBeNull();
    // Its log is the one that needs it: one, so nothing to step through.
    fireEvent.click(getByRole("button", { name: "View Log" }));
    expect(useUiStore.getState().focusedOpId).toBe(12);
    expect(useUiStore.getState().logRun).toEqual([]);
    await act(async () => {
      await i18n.changeLanguage("zh-CN");
    });
    await findByText("1个更新失败，2个已成功");
    await act(async () => {
      await i18n.changeLanguage("en");
    });
  });

  it("says each step and how it went in one live line, the same node throughout, so a screen reader hears the change", async () => {
    const { findByText, container, queryClient } = renderWithProviders(<OperationBar />);
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));

    await listNow(queryClient, [op(12, "jq", "Queued"), op(11, "git", "Running")]);
    await findByText("git: Updating…");
    const lines = () => container.querySelectorAll("[aria-live]");
    expect(lines()).toHaveLength(1);
    const line = lines()[0];
    expect(line).toHaveAttribute("aria-live", "polite");
    expect(line).toHaveTextContent("Working on 1 of 2");

    await listNow(queryClient, [op(12, "jq", "Running"), op(11, "git", "Done", "Succeeded")]);
    await findByText("jq: Updating…");
    expect(lines()[0]).toBe(line);

    // How it went, where it stood: the same node, not a new one.
    await listNow(queryClient, [op(12, "jq", "Done", "Succeeded"), op(11, "git", "Done", "Succeeded")]);
    await findByText("Updated 2 tools");
    expect(lines()).toHaveLength(1);
    expect(lines()[0]).toBe(line);
    expect(line).toHaveTextContent("Updated 2 tools");
  });

  describe("the run's Cancel, in a run of several", () => {
    // rustup's self update: NoCancel, which nothing stops once it runs.
    const rustup = (id: number, status: OpStatus, outcome: Outcome | null = null) =>
      op(id, "rustup", status, outcome, {
        instance_id: "standalone-rustup",
        artifact_kind: "Binary",
        cancel_policy: "NoCancel",
      });

    it("cancels every operation of the run still to do, the queued ones and each one running, as Cancel all", async () => {
      // Homebrew's two, one after the other on its lock, and npm's beside
      // them: up to three run at once on different locks.
      const { findByRole, queryByRole, queryClient } = renderWithProviders(<OperationBar />);
      await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));
      await listNow(queryClient, [
        op(14, "typescript", "Running", null, { instance_id: "npm:/usr/local", artifact_kind: "Package" }),
        op(13, "wget", "Queued"),
        op(12, "jq", "Queued"),
        op(11, "git", "Running"),
      ]);

      fireEvent.click(await findByRole("button", { name: "Cancel All" }));
      expect(queryByRole("button", { name: "Cancel" })).toBeNull();
      expect(queryByRole("button", { name: "Cancel the Rest" })).toBeNull();

      // The queued ones first, so neither starts as the one ahead of it stops.
      await waitFor(() => expect(calledToCancel()).toEqual([12, 13, 11, 14]));
    });

    it("says Cancel all over a NoCancel operation still waiting its turn, which a cancel still drops", async () => {
      const { findByRole, findByText, queryClient } = renderWithProviders(<OperationBar />);
      await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));
      await listNow(queryClient, [rustup(12, "Queued"), op(11, "git", "Running")]);

      await findByText("git: Updating…");
      fireEvent.click(await findByRole("button", { name: "Cancel All" }));
      await waitFor(() => expect(calledToCancel()).toEqual([12, 11]));
    });

    it("says Cancel the rest while one running cannot be stopped, cancels the queued ones, and goes on naming that one", async () => {
      // rustup running; two queued behind it.
      const { findByRole, findByText, getByText, queryByRole, queryClient } = renderWithProviders(<OperationBar />);
      await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));
      await listNow(queryClient, [op(11, "wget", "Queued"), op(10, "jq", "Queued"), rustup(9, "Running")]);

      await findByText("rustup: Updating…");
      expect(queryByRole("button", { name: "Cancel All" })).toBeNull();
      fireEvent.click(await findByRole("button", { name: "Cancel the Rest" }));
      await waitFor(() => expect(calledToCancel()).toEqual([10, 11]));

      // Their cancels on the way: held, not pressable twice.
      await listNow(queryClient, [
        op(11, "wget", "CancelRequested"),
        op(10, "jq", "CancelRequested"),
        rustup(9, "Running"),
      ]);
      expect(await findByRole("button", { name: "Cancel the Rest" })).toBeDisabled();
      expect(getByText("rustup: Updating…")).toBeInTheDocument();

      // Only rustup left, which nothing can stop: no button, and the bar
      // goes on naming it until it has finished.
      await listNow(queryClient, [
        op(11, "wget", "Done", "Cancelled"),
        op(10, "jq", "Done", "Cancelled"),
        rustup(9, "Running"),
      ]);
      await waitFor(() => expect(queryByRole("button", { name: /^Stop/ })).toBeNull());
      expect(getByText("rustup: Updating…")).toBeInTheDocument();
      expect(calledToCancel()).toEqual([10, 11]);
    });

    it("names the one that cannot be stopped over an older one running, which Cancel the rest cancels", async () => {
      // git on Homebrew's lock and rustup on its own, both running, with
      // wget queued behind git.
      const { findByRole, findByText, getByRole, getByText, queryByText, queryClient } = renderWithProviders(
        <OperationBar />,
      );
      await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));
      await listNow(queryClient, [op(13, "wget", "Queued"), rustup(12, "Running"), op(11, "git", "Running")]);

      // "The rest" is everything but the one on the bar.
      await findByText("rustup: Updating…");
      expect(queryByText("git: Updating…")).toBeNull();
      fireEvent.click(getByRole("button", { name: "View Log" }));
      expect(useUiStore.getState().focusedOpId).toBe(12);
      fireEvent.click(getByRole("button", { name: "Cancel the Rest" }));
      await waitFor(() => expect(calledToCancel()).toEqual([13, 11]));

      // git stopping, wget dropped: rustup still on the bar.
      await listNow(queryClient, [op(13, "wget", "CancelRequested"), rustup(12, "Running"), op(11, "git", "Cancelling")]);
      expect(await findByRole("button", { name: "Cancel the Rest" })).toBeDisabled();
      expect(getByText("rustup: Updating…")).toBeInTheDocument();

      await listNow(queryClient, [
        op(13, "wget", "Done", "Cancelled"),
        rustup(12, "Done", "Succeeded"),
        op(11, "git", "Done", "Cancelled"),
      ]);
      await findByText("1 succeeded, 2 cancelled");
      expect(calledToCancel()).toEqual([13, 11]);
    });

    it("calls them 全部取消 and 取消其余 in Chinese, and a single operation's 取消", async () => {
      await i18n.changeLanguage("zh-CN");
      try {
        const { findByRole, queryClient } = renderWithProviders(<OperationBar />);
        await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));
        await listNow(queryClient, [op(5, "ffmpeg", "Running")]);
        expect(await findByRole("button", { name: "取消" })).toBeEnabled();

        await listNow(queryClient, [op(6, "jq", "Queued"), op(5, "ffmpeg", "Running")]);
        expect(await findByRole("button", { name: "全部取消" })).toBeEnabled();

        await listNow(queryClient, [
          rustup(8, "Running"),
          op(7, "wget", "Queued"),
          op(6, "jq", "Queued"),
          op(5, "ffmpeg", "Running"),
        ]);
        expect(await findByRole("button", { name: "取消其余" })).toBeEnabled();
      } finally {
        await i18n.changeLanguage("en");
      }
    });
  });

  it("names its button View Steps where sudo wanted the Mac's password, as the log has the command for Terminal", async () => {
    // Walk-2 W2-5: the way on was behind 「查看日志」, which a person who
    // does not write code reads as something for programmers.
    const { findByText, getByRole, queryByRole, queryClient } = renderWithProviders(<OperationBar />);
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));
    const password: Outcome = {
      Failed: { exit_code: 1, summary: "sudo: a terminal is required to read the password\nsudo: a password is required", cause: failureCause("sudo: a terminal is required to read the password\nsudo: a password is required") },
    };
    await listNow(queryClient, [op(21, "android-platform-tools", "Done", password)]);
    await findByText("android-platform-tools: Update · Needs your password");
    expect(queryByRole("button", { name: "View Log" })).toBeNull();
    fireEvent.click(getByRole("button", { name: "View Steps" }));
    expect(useUiStore.getState()).toMatchObject({ focusedOpId: 21, logRun: [], drawerOpen: true });
    await act(async () => {
      await i18n.changeLanguage("zh-CN");
    });
    expect(getByRole("button", { name: "查看步骤" })).toBeInTheDocument();
    await act(async () => {
      await i18n.changeLanguage("en");
    });
  });

  it("keeps View Log for a password stop whose log has no command for Terminal: a source other than Homebrew (r26 W6 skeptic)", async () => {
    // `PasswordCommand` hands over only Homebrew's command; npm's log has
    // the cause and Copy Log, no steps (`copyOnly`).
    const { findByText, findByRole, queryByRole, queryClient } = renderWithProviders(<OperationBar />);
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));
    const sudo = "sudo: a terminal is required to read the password\nsudo: a password is required";
    await listNow(queryClient, [
      op(22, "prettier", "Done", { Failed: { exit_code: 1, summary: sudo, cause: failureCause(sudo) } }, {
        instance_id: "npm:/opt/homebrew/bin/npm",
        artifact_kind: "Package",
        argv_preview: ["/opt/homebrew/bin/npm", "install", "-g", "prettier@latest"],
      }),
    ]);
    await findByText("prettier: Update · Needs your password");
    expect(await findByRole("button", { name: "View Log" })).toBeInTheDocument();
    expect(queryByRole("button", { name: "View Steps" })).toBeNull();
  });

  it("opens every log of a run that needs a look, from the first, and a log opened on one has none to step through", async () => {
    const { findByText, getByRole, queryClient } = renderWithProviders(<OperationBar />);
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));
    await listNow(queryClient, [
      op(34, "wget", "Done", "Succeeded"),
      op(33, "jq", "Done", { Failed: { exit_code: 1, summary: "Error: jq is pinned", cause: failureCause("Error: jq is pinned") } }),
      op(32, "git", "Done", { NeedsAttention: "UnchangedAfterUpgrade" }),
      op(31, "gh", "Done", { Failed: { exit_code: 1, summary: "curl: (6) Could not resolve host", cause: failureCause("curl: (6) Could not resolve host") } }),
    ]);
    await findByText("2 updates failed, 1 succeeded, 1 needs attention");
    fireEvent.click(getByRole("button", { name: "View 3 Logs" }));
    expect(useUiStore.getState()).toMatchObject({ focusedOpId: 31, logRun: [31, 32, 33], drawerOpen: true });
    // A row's own word opens its log alone.
    act(() => useUiStore.getState().setFocusedOpId(33));
    expect(useUiStore.getState().logRun).toEqual([]);
  });

  it("says a run of several all succeeded, with nothing to look at, or how many were cancelled, with their log", async () => {
    const { findByText, getByRole, queryByRole, queryClient } = renderWithProviders(<OperationBar />);
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));

    await listNow(queryClient, [op(2, "jq", "Running"), op(1, "git", "Running")]);
    await listNow(queryClient, [op(2, "jq", "Done", "Succeeded"), op(1, "git", "Done", "Succeeded")]);
    await findByText("Updated 2 tools");
    expect(queryByRole("button", { name: "View Log" })).toBeNull();

    // A new run: one cancelled, one updated.
    await listNow(queryClient, [
      op(4, "wget", "Queued"),
      op(3, "gh", "Running"),
      op(2, "jq", "Done", "Succeeded"),
      op(1, "git", "Done", "Succeeded"),
    ]);
    await findByText("Working on 1 of 2");
    await listNow(queryClient, [
      op(4, "wget", "Done", "Cancelled"),
      op(3, "gh", "Done", "Succeeded"),
      op(2, "jq", "Done", "Succeeded"),
      op(1, "git", "Done", "Succeeded"),
    ]);
    await findByText("1 succeeded, 1 cancelled");
    // Cancelled while it waited its turn, it printed nothing: no log to
    // offer, which would open on an empty page (r24 W5, skeptic).
    expect(queryByRole("button", { name: "View Log" })).toBeNull();

    // A new run: one cancelled once it had started and printed, one updated.
    await listNow(queryClient, [
      op(6, "httpie", "Running"),
      op(5, "tree", "Running"),
      op(4, "wget", "Done", "Cancelled"),
      op(3, "gh", "Done", "Succeeded"),
      op(2, "jq", "Done", "Succeeded"),
      op(1, "git", "Done", "Succeeded"),
    ]);
    await findByText(/Working on/);
    useUiStore.getState().appendLog({ opId: 6, stream: "Stdout", line: "==> Upgrading httpie" });
    await listNow(queryClient, [
      op(6, "httpie", "Done", "Cancelled"),
      op(5, "tree", "Done", "Succeeded"),
      op(4, "wget", "Done", "Cancelled"),
      op(3, "gh", "Done", "Succeeded"),
      op(2, "jq", "Done", "Succeeded"),
      op(1, "git", "Done", "Succeeded"),
    ]);
    await findByText("1 succeeded, 1 cancelled");
    // Its log, as a single operation's bar offers it after a cancel: what
    // had already happened (r24 W5).
    fireEvent.click(getByRole("button", { name: "View Log" }));
    expect(useUiStore.getState().focusedOpId).toBe(6);
  });

  it("leaves out a zero when every one of a run was cancelled, and offers their logs (r24 W5)", async () => {
    // Cancel All while each was still waiting: not 「0个已成功，2个已取消」.
    await i18n.changeLanguage("zh-CN");
    try {
      const { findByText, getByRole, queryByText, queryClient, unmount } = renderWithProviders(<OperationBar />);
      await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));
      await listNow(queryClient, [op(2, "httpie", "Running"), op(1, "git", "Running")]);
      // Each had started, and printed something, before Cancel All.
      useUiStore.getState().appendLog({ opId: 1, stream: "Stdout", line: "==> Upgrading git" });
      useUiStore.getState().appendLog({ opId: 2, stream: "Stdout", line: "==> Upgrading httpie" });
      await listNow(queryClient, [op(2, "httpie", "Done", "Cancelled"), op(1, "git", "Done", "Cancelled")]);

      await findByText("2个已取消");
      expect(queryByText(/0个已成功/)).toBeNull();
      // Both logs, the first one opened, from where 下一个 steps to the other.
      fireEvent.click(getByRole("button", { name: "查看2个日志" }));
      expect(useUiStore.getState().logRun).toEqual([1, 2]);
      unmount();
    } finally {
      await i18n.changeLanguage("en");
    }

    // In English: never "0 succeeded" first either.
    const { findByText, queryByText, queryClient } = renderWithProviders(<OperationBar />);
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toBeDefined());
    await listNow(queryClient, [op(4, "wget", "Queued"), op(3, "gh", "Running")]);
    await listNow(queryClient, [op(4, "wget", "Done", "Cancelled"), op(3, "gh", "Done", "Cancelled")]);
    await findByText("2 cancelled");
    expect(queryByText(/0 succeeded/)).toBeNull();
  });

  it("offers the logs only of the cancelled ones that printed something, never an empty page (r24 W5, skeptic)", async () => {
    // Cancel All while one ran and the rest still waited their turn: only
    // the one that ran has a log with anything in it.
    const { findByText, getByRole, queryByRole, queryClient } = renderWithProviders(<OperationBar />);
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));
    await listNow(queryClient, [op(3, "node@22", "Queued"), op(2, "gemini-cli", "Queued"), op(1, "git", "Running")]);
    useUiStore.getState().appendLog({ opId: 1, stream: "Stdout", line: "==> Upgrading git" });
    await listNow(queryClient, [
      op(3, "node@22", "Done", "Cancelled"),
      op(2, "gemini-cli", "Done", "Cancelled"),
      op(1, "git", "Done", "Cancelled"),
    ]);
    await findByText("3 cancelled");
    expect(queryByRole("button", { name: /View \d+ Logs/ })).toBeNull();
    fireEvent.click(getByRole("button", { name: "View Log" }));
    expect(useUiStore.getState().focusedOpId).toBe(1);
    expect(useUiStore.getState().logRun).toEqual([]);

    // A new run, every one cancelled before its turn: nothing to look at.
    await listNow(queryClient, [
      op(5, "tree", "Queued"),
      op(4, "wget", "Queued"),
      op(3, "node@22", "Done", "Cancelled"),
      op(2, "gemini-cli", "Done", "Cancelled"),
      op(1, "git", "Done", "Cancelled"),
    ]);
    await findByText("Working on 1 of 2");
    await listNow(queryClient, [
      op(5, "tree", "Done", "Cancelled"),
      op(4, "wget", "Done", "Cancelled"),
      op(3, "node@22", "Done", "Cancelled"),
      op(2, "gemini-cli", "Done", "Cancelled"),
      op(1, "git", "Done", "Cancelled"),
    ]);
    await findByText("2 cancelled");
    expect(queryByRole("button", { name: "View Log" })).toBeNull();
    expect(queryByRole("button", { name: /View \d+ Logs/ })).toBeNull();
    expect(getByRole("button", { name: "Close" })).toBeInTheDocument();
  });

  it("offers a single cancelled operation's log only where it printed something", async () => {
    // Cancelled before it printed a line: its log would be an empty page.
    operations = [op(7, "wget", "Done", "Cancelled")];
    const first = renderWithProviders(<OperationBar />);
    await first.findByText("wget: Update · Cancelled");
    expect(first.queryByRole("button", { name: "View Log" })).toBeNull();
    first.unmount();

    // Once it has: what had already happened.
    useUiStore.getState().appendLog({ opId: 7, stream: "Stdout", line: "==> Downloading wget" });
    const second = renderWithProviders(<OperationBar />);
    await second.findByText("wget: Update · Cancelled");
    fireEvent.click(second.getByRole("button", { name: "View Log" }));
    expect(useUiStore.getState().focusedOpId).toBe(7);
  });

  it("calls what it acts on by the name its row shows, and keeps it once the row is gone", async () => {
    // A tool with its own installer: the operation carries its key's name,
    // `claude`; the row says Claude Code, and leaves the snapshot once the
    // uninstall has finished.
    const claude = { instance_id: "standalone-claude", kind: "Binary" as const, name: "claude" };
    const listed: Snapshot = {
      generation: 1,
      round: 1,
      detect: "Found",
      instances: [],
      artifacts: [
        {
          key: claude,
          display_name: "Claude Code",
          version: "2.1.2",
          reason: "Requested",
          description: null,
          homepage: null,
          size_bytes: null,
          installed_at: null,
          path: null,
          auto_updates: true,
          uninstall_blocked: null,
          facts: NO_FACTS,
        },
      ],
      updates: [],
      refreshed_at: 1_789_700_000,
      stale: false,
      errors: [],
    };
    snapshot = listed;
    const uninstall = { kind: "Uninstall" as const, instance_id: "standalone-claude", artifact_kind: "Binary" as const };
    operations = [op(3, "claude", "Running", null, uninstall)];
    const { findByText, queryClient } = renderWithProviders(<OperationBar />);
    await findByText("Claude Code: Uninstalling…");

    act(() => {
      queryClient.setQueryData<Snapshot>(queryKeys.snapshot, { ...listed, generation: 2, artifacts: [] });
    });
    await listNow(queryClient, [op(3, "claude", "Done", "Succeeded", uninstall)]);

    await findByText("Claude Code: Uninstalled");
  });

  it("disables Cancel while the finished command's result is being verified", async () => {
    // `OperationManager::cancel` (ops/mod.rs) answers `NotPending` for a
    // Verifying op and the IPC turns that into a silent Ok, so an enabled
    // button here would do nothing when clicked.
    operations = [op(8, "jq", "Verifying")];

    const { findByRole, getByText } = renderWithProviders(<OperationBar />);
    expect(await findByRole("button", { name: "Cancel" })).toBeDisabled();
    expect(getByText("jq: Update · Checking the result…")).toBeInTheDocument();
  });

  it("offers no Cancel button for a running rustup self update, whose plan says NoCancel", async () => {
    // `OperationManager::cancel` (ops/mod.rs) refuses such an op once it
    // is Running, so a button here would promise something the backend
    // will not do. rustup's `self update` and `self uninstall` are the
    // plans that say NoCancel (crates/banager-core/src/adapters/
    // standalone/recipes.rs): the first unlinks and re-copies the binary
    // every Rust proxy runs, the second removes Rust directory by
    // directory.
    operations = [
      op(7, "rustup", "Running", null, {
        instance_id: "standalone-rustup",
        artifact_kind: "Binary",
        argv_preview: ["/Users/me/.cargo/bin/rustup", "self", "update"],
        cancel_policy: "NoCancel",
      }),
    ];

    const { findByText, queryByRole } = renderWithProviders(<OperationBar />);

    await findByText("rustup: Updating…");
    expect(queryByRole("button", { name: "Cancel" })).not.toBeInTheDocument();
  });

  it("offers Cancel for a queued rustup self update, whose plan says NoCancel, and it reaches cancel_operation", async () => {
    // A Queued op has started nothing, so `OperationManager::cancel`
    // (ops/mod.rs) accepts its cancel whatever the plan says and the
    // command never runs; NoCancel only bites once the op is Running.
    // Without the button the user could not drop a NoCancel op stuck
    // behind another op's lock -- rustup's plans also hold the cargo
    // instance's lock, so one queued behind a cargo install is real.
    operations = [
      op(8, "rustup", "Queued", null, {
        instance_id: "standalone-rustup",
        artifact_kind: "Binary",
        argv_preview: ["/Users/me/.cargo/bin/rustup", "self", "update"],
        cancel_policy: "NoCancel",
      }),
    ];

    const { findByRole, findByText } = renderWithProviders(<OperationBar />);

    await findByText("rustup: Update · Queued");
    const cancel = await findByRole("button", { name: "Cancel" });
    expect(cancel).toBeEnabled();
    fireEvent.click(cancel);
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith("cancel_operation", { opId: 8 }),
    );
  });
});

describe("OperationBar, after a batch uninstall", () => {
  const uninstall = (id: number, name: string, status: OpStatus, outcome: Outcome | null = null) =>
    op(id, name, status, outcome, { kind: "Uninstall", argv_preview: ["/opt/homebrew/bin/brew", "uninstall", name] });

  it("says how many tools it uninstalled, or how many it couldn't, as it says updates", async () => {
    const { findByText, getByRole, queryByRole, queryClient } = renderWithProviders(<OperationBar />);
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));

    await listNow(queryClient, [uninstall(3, "wget", "Queued"), uninstall(2, "jq", "Queued"), uninstall(1, "git", "Running")]);
    await findByText("Working on 1 of 3");
    expect(getByRole("button", { name: "Cancel All" })).toBeEnabled();
    await listNow(queryClient, [
      uninstall(3, "wget", "Done", "Succeeded"),
      uninstall(2, "jq", "Done", "Succeeded"),
      uninstall(1, "git", "Done", "Succeeded"),
    ]);
    await findByText("Uninstalled 3 tools");
    expect(queryByRole("button", { name: "View Log" })).toBeNull();

    // A new run: two couldn't be uninstalled.
    await listNow(queryClient, [
      uninstall(6, "python@3.13", "Queued"),
      uninstall(5, "pipx", "Running"),
      uninstall(4, "htop", "Queued"),
      uninstall(3, "wget", "Done", "Succeeded"),
      uninstall(2, "jq", "Done", "Succeeded"),
      uninstall(1, "git", "Done", "Succeeded"),
    ]);
    await findByText("Working on 1 of 3");
    const failed: Outcome = { Failed: { exit_code: 1, summary: "Error: Refusing to uninstall", cause: failureCause("Error: Refusing to uninstall") } };
    await listNow(queryClient, [
      uninstall(6, "python@3.13", "Done", failed),
      uninstall(5, "pipx", "Done", failed),
      uninstall(4, "htop", "Done", "Succeeded"),
      uninstall(3, "wget", "Done", "Succeeded"),
      uninstall(2, "jq", "Done", "Succeeded"),
      uninstall(1, "git", "Done", "Succeeded"),
    ]);
    // htop succeeded in the same run: said with the two that didn't.
    await findByText("Uninstalled 1; 2 weren't uninstalled");
    // Both logs, from the first of them, in the order they ran (walk-2
    // W2-4): View Log used to open the last one's alone.
    expect(queryByRole("button", { name: "View Log" })).toBeNull();
    fireEvent.click(getByRole("button", { name: "View 2 Logs" }));
    expect(useUiStore.getState()).toMatchObject({ focusedOpId: 5, logRun: [5, 6], drawerOpen: true });

    await act(async () => {
      await i18n.changeLanguage("zh-CN");
    });
    await findByText("已卸载1个，2个没有卸载");
    expect(getByRole("button", { name: "查看2个日志" })).toBeInTheDocument();
    await act(async () => {
      await i18n.changeLanguage("en");
    });
  });

  it("says a run with a cancelled and a failed uninstall as the result block above the list says it", async () => {
    // The same batch, as the Installed page records it for its result block.
    const key = (name: string) => ({ instance_id: "brew:/opt/homebrew", kind: "Formula" as const, name });
    useUiStore.getState().setUninstallBatch({
      id: 1,
      items: [
        { key: key("git"), name: "git", opId: 1, after: [] },
        { key: key("jq"), name: "jq", opId: 2, after: [] },
        { key: key("wget"), name: "wget", opId: 3, after: [] },
      ],
    });
    const { findAllByText, queryClient } = renderWithProviders(
      <>
        <OperationBar />
        <BatchUninstallResult />
      </>,
    );
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));
    await listNow(queryClient, [uninstall(3, "wget", "Queued"), uninstall(2, "jq", "Queued"), uninstall(1, "git", "Running")]);
    await listNow(queryClient, [
      uninstall(3, "wget", "Done", "Cancelled"),
      uninstall(2, "jq", "Done", { Failed: { exit_code: 1, summary: "Error: Refusing to uninstall", cause: failureCause("Error: Refusing to uninstall") } }),
      uninstall(1, "git", "Done", "Succeeded"),
    ]);
    // The cancelled one is not counted as uninstalled, and the bar and the
    // block say the run the same way: once each.
    expect(await findAllByText("Uninstalled 1; 2 weren't uninstalled")).toHaveLength(2);
    await act(async () => {
      await i18n.changeLanguage("zh-CN");
    });
    expect(await findAllByText("已卸载1个，2个没有卸载")).toHaveLength(2);
    await act(async () => {
      await i18n.changeLanguage("en");
    });
  });

  it("says a mixed run in words for any operation", async () => {
    const { findByText, queryClient } = renderWithProviders(<OperationBar />);
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));
    await listNow(queryClient, [uninstall(2, "jq", "Running"), op(1, "git", "Running")]);
    await listNow(queryClient, [uninstall(2, "jq", "Done", "Succeeded"), op(1, "git", "Done", "Succeeded")]);
    await findByText("All 2 succeeded");
  });
});

it("keeps warning and log access for a successful single update and successful batch", async () => {
  await i18n.changeLanguage("zh-CN");
  operations = [op(1, "node@22", "Done", "Succeeded", {
    follow_up_warnings: [{ NoLongerLinked: { name: "node@22", commands: ["node", "npm"] } }],
  })];
  const view = renderWithProviders(<OperationBar />);
  const log = await view.findByRole("button", { name: "查看日志" });
  expect(view.getByText(/已更新.*警告/)).toBeInTheDocument();
  fireEvent.click(log);
  expect(useUiStore.getState().focusedOpId).toBe(1);
  // A new batch starts with both operations active, then finishes successfully.
  await listNow(view.queryClient, [op(3, "node@22", "Running"), op(2, "jq", "Queued"), ...operations]);
  await view.findByText(/正在更新/);
  await listNow(view.queryClient, [op(3, "node@22", "Done", "Succeeded", {
    follow_up_warnings: [{ NoLongerLinked: { name: "node@22", commands: ["node", "npm"] } }],
  }), op(2, "jq", "Done", "Succeeded"), op(1, "old", "Done", "Succeeded")]);
  await view.findByText("已更新2个工具，1个有警告");
  fireEvent.click(view.getByRole("button", { name: "查看日志" }));
  expect(useUiStore.getState().focusedOpId).toBe(3);
});

it("counts a batch's follow-up warnings in English as one or many, and calls them warnings (f13b review, r21 C5)", async () => {
  await i18n.changeLanguage("en");
  const warned = { follow_up_warnings: [{ OldVersionsNotCleanedUp: { name: "git", exit_code: 1 } }] };
  operations = [op(2, "git", "Running"), op(1, "jq", "Queued")];
  const view = renderWithProviders(<OperationBar />);
  await view.findByText(/Working on/);
  await listNow(view.queryClient, [op(2, "git", "Done", "Succeeded", warned), op(1, "jq", "Done", "Succeeded")]);
  expect(await view.findByText("Updated 2 tools · 1 with a warning")).toBeInTheDocument();
  await listNow(view.queryClient, [op(4, "git", "Running"), op(3, "wget", "Queued"), ...operations]);
  await view.findByText(/Working on/);
  await listNow(view.queryClient, [
    op(4, "git", "Done", "Succeeded", warned),
    op(3, "wget", "Done", "Succeeded", { follow_up_warnings: [{ OldVersionsNotCleanedUp: { name: "wget", exit_code: 1 } }] }),
    op(2, "git", "Done", "Succeeded", warned),
    op(1, "jq", "Done", "Succeeded"),
  ]);
  expect(await view.findByText("Updated 2 tools · 2 with warnings")).toBeInTheDocument();
});

it("says a run that updated one tool and uninstalled another, one update leaving a warning, in words for any operation (r31 E1)", async () => {
  await i18n.changeLanguage("en");
  const warned = { follow_up_warnings: [{ OldVersionsNotCleanedUp: { name: "git", exit_code: 1 } }] };
  const uninstalled = { kind: "Uninstall" as const, artifact_kind: "Package" as const, instance_id: "pipx:/Users/test/.local", argv_preview: ["/opt/homebrew/bin/pipx", "uninstall", "aider-chat"] };
  // git's update, and aider-chat's uninstall started from Installed while it ran.
  operations = [op(2, "aider-chat", "Queued", null, uninstalled), op(1, "git", "Running")];
  const view = renderWithProviders(<OperationBar />);
  await view.findByText(/Working on/);
  await listNow(view.queryClient, [
    op(2, "aider-chat", "Done", "Succeeded", uninstalled),
    op(1, "git", "Done", "Succeeded", warned),
  ]);
  // Not "Updated 2 tools": aider-chat was uninstalled.
  expect(await view.findByText("All 2 succeeded · 1 with a warning")).toBeInTheDocument();
  expect(view.queryByText(/Updated 2 tools/)).toBeNull();
  // Its log is still the warned one's.
  fireEvent.click(view.getByRole("button", { name: "View Log" }));
  expect(useUiStore.getState().focusedOpId).toBe(1);
  for (const [language, words] of [
    ["zh-CN", "2个都已成功，1个有警告"],
    ["zh-Hant", "2個都已成功，1個有警告"],
  ] as const) {
    await act(async () => {
      await i18n.changeLanguage(language);
    });
    expect(await view.findByText(words)).toBeInTheDocument();
    expect(view.queryByText(/已更新2/)).toBeNull();
  }
  await act(async () => {
    await i18n.changeLanguage("en");
  });
});
