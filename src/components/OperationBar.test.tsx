import { describe, expect, it, vi, beforeEach } from "vitest";
import { act, fireEvent, waitFor, within } from "@testing-library/react";
import type { QueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { OperationBar } from "./OperationBar";
import { BUTTON } from "./ui/controls";
import { useUiStore } from "../store/ui";
import { queryKeys } from "../lib/queryKeys";
import i18n from "../i18n";
import type { OpStatus, OpSummary, Outcome, Snapshot } from "../lib/types";

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
  useUiStore.setState({ drawerOpen: false, focusedOpId: null, logs: [] });
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

    await findByText("Update wget: Running");
    const bar = getByRole("contentinfo", { name: "Operation status" });
    expect(within(bar).getByRole("button", { name: "Cancel" })).toBeEnabled();
    expect(within(bar).getByRole("button", { name: "View Log" })).toBeInTheDocument();
    // Nothing to close while something runs.
    expect(within(bar).queryByRole("button", { name: "Close" })).toBeNull();
  });

  it("shows the running operation and cancels it on click", async () => {
    operations = [op(5, "onyx", "Running", null, { artifact_kind: "Cask" })];

    const { findByRole, findByText } = renderWithProviders(<OperationBar />);
    await findByText("Update onyx: Running");
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
    await findByText("Update onyx: Waiting for Homebrew's software list…");
    // Cancel must still work: the wait is still part of an active op.
    expect(await findByRole("button", { name: "Cancel" })).toBeEnabled();
  });

  it("says it in Chinese as the copy table has it: what it does, to what, then where it stands", async () => {
    operations = [op(5, "ffmpeg", "Running")];
    useUiStore.getState().appendLog({ opId: 5, note: { WaitingForBrewUpdate: { minutes: 10 } } });
    await i18n.changeLanguage("zh-CN");
    try {
      const { findByText } = renderWithProviders(<OperationBar />);
      await findByText("更新ffmpeg：正在等待Homebrew更新软件清单…");
    } finally {
      await i18n.changeLanguage("en");
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
    await findByText("Update onyx: Cancelling…");
    expect(queryByText("Update onyx: Waiting for Homebrew's software list…")).not.toBeInTheDocument();
    // A cancel already on its way: the button stays, and cannot be pressed twice.
    expect(getByRole("button", { name: "Cancel" })).toBeDisabled();
  });

  it("keeps a finished operation visible with its outcome in place of its status, and no Cancel button", async () => {
    operations = [
      op(6, "jqq", "Done", { Failed: { exit_code: 1, summary: "No available formula with the name \"jqq\"" } }, {
        kind: "Install",
      }),
    ];

    const { findByText, queryByRole, getByRole, queryByText } = renderWithProviders(<OperationBar />);

    // The tool's own words only with Show technical details on (see
    // below); its log has them.
    await findByText("Install jqq: Couldn't finish");
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
      Failed: { exit_code: 1, summary: 'curl: (6) Could not resolve host: ghcr.io\nError: git: Failed to download resource "git (2.55.1)"' },
    });
    operations = [failed];
    const { findByText, queryByText, getByRole } = renderWithProviders(<OperationBar />);

    await findByText("Update git: Connection failed");
    expect(queryByText(/ghcr\.io|Failed to download/)).toBeNull();
    expect(getByRole("button", { name: "View Log" })).toBeInTheDocument();
  });

  it("shows a program's own words only with Show technical details on", async () => {
    technical = true;
    operations = [
      op(6, "jqq", "Done", { Failed: { exit_code: 1, summary: 'No available formula with the name "jqq"' } }, { kind: "Install" }),
    ];
    const tech = renderWithProviders(<OperationBar />);
    await tech.findByText('Install jqq: Couldn\'t finish: No available formula with the name "jqq"');
    tech.unmount();

    // macOS's own reason a program would not start, likewise.
    operations = [op(7, "wget", "Done", { CanagerFailed: { SpawnFailed: { detail: "Permission denied (os error 13)" } } })];
    const on = renderWithProviders(<OperationBar />);
    await on.findByText(/os error 13/);
    on.unmount();

    technical = false;
    const off = renderWithProviders(<OperationBar />);
    await off.findByText("Update wget: Couldn't start the program. Nothing changed");
    expect(off.queryByText(/os error 13/)).toBeNull();
  });

  it("says how many of a run need a look where one of them only asks to be checked", async () => {
    const { findByText, queryClient } = renderWithProviders(<OperationBar />);
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));
    await listNow(queryClient, [op(2, "jq", "Queued"), op(1, "git", "Running")]);
    await listNow(queryClient, [
      op(2, "jq", "Done", { NeedsAttention: "UnchangedAfterUpgrade" }),
      op(1, "git", "Done", { Failed: { exit_code: 1, summary: "Error: git is pinned" } }),
    ]);
    await findByText("2 of 2 need attention");
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

    await findByText("Update git: Completed");
    expect(queryByRole("button", { name: "View Log" })).toBeNull();
    expect(queryByRole("img", { name: "Needs attention" })).toBeNull();
    expect(getByRole("button", { name: "Close" })).toBeInTheDocument();
  });

  it("marks an outcome that needs attention with the shared label, not a 需要留意 prefix, and offers its log", async () => {
    operations = [op(8, "git", "Done", { NeedsAttention: "UnchangedAfterUpgrade" })];

    const { findByText, getByRole } = renderWithProviders(<OperationBar />);

    await findByText("Update git: Update reported success, but the version didn't change");
    expect(getByRole("img", { name: "Needs attention" })).toBeInTheDocument();
    expect(getByRole("button", { name: "View Log" })).toBeInTheDocument();
  });

  it("closes with × once everything is done, and comes back for the next operation", async () => {
    operations = [op(7, "git", "Done", "Succeeded")];
    const { container, findByText, getByRole, queryClient } = renderWithProviders(<OperationBar />);
    await findByText("Update git: Completed");

    fireEvent.click(getByRole("button", { name: "Close" }));
    expect(container).toBeEmptyDOMElement();

    // The list asked again, with nothing new in it: still closed.
    await listNow(queryClient, [op(7, "git", "Done", "Succeeded")]);
    expect(container).toBeEmptyDOMElement();

    // A new operation brings it back, about the new one alone.
    await listNow(queryClient, [op(8, "wget", "Running"), op(7, "git", "Done", "Succeeded")]);
    await findByText("Update wget: Running");
    expect(container.textContent).not.toContain("git");
  });

  it("shows a new run's first operation in place of the last run's result", async () => {
    operations = [op(7, "git", "Done", "Succeeded")];
    const { findByText, queryClient, queryByText } = renderWithProviders(<OperationBar />);
    await findByText("Update git: Completed");

    await listNow(queryClient, [op(8, "wget", "Running"), op(7, "git", "Done", "Succeeded")]);

    await findByText("Update wget: Running");
    expect(queryByText("Update git: Completed")).toBeNull();
    // One operation in this run, so no count.
    expect(queryByText(/Working on/)).toBeNull();
  });

  it("counts a run of several as it goes, and says what it came to", async () => {
    // Update all: three operations started together, one after another on
    // Homebrew's lock.
    const { findByText, getByText, getByRole, queryByRole, queryClient } = renderWithProviders(<OperationBar />);
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));

    await listNow(queryClient, [op(13, "wget", "Queued"), op(12, "jq", "Queued"), op(11, "git", "Running")]);
    await findByText("Working on 1 of 3");
    // The one doing something; Cancel is for the whole run.
    expect(getByText("Update git: Running")).toBeInTheDocument();
    expect(getByRole("button", { name: "Cancel All" })).toBeEnabled();

    await listNow(queryClient, [
      op(13, "wget", "Queued"),
      op(12, "jq", "Running"),
      op(11, "git", "Done", "Succeeded"),
    ]);
    await findByText("Working on 2 of 3");
    expect(getByText("Update jq: Running")).toBeInTheDocument();

    await listNow(queryClient, [
      op(13, "wget", "Done", "Succeeded"),
      op(12, "jq", "Done", { Failed: { exit_code: 1, summary: "Error: jq is pinned" } }),
      op(11, "git", "Done", "Succeeded"),
    ]);
    // Only failures, of updates: how many did not update.
    await findByText("1 couldn't be updated");
    expect(queryByRole("button", { name: /^Stop/ })).toBeNull();
    // Its log is the one that needs it.
    fireEvent.click(getByRole("button", { name: "View Log" }));
    expect(useUiStore.getState().focusedOpId).toBe(12);
  });

  it("says each step and how it went in one live line, the same node throughout, so a screen reader hears the change", async () => {
    const { findByText, container, queryClient } = renderWithProviders(<OperationBar />);
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));

    await listNow(queryClient, [op(12, "jq", "Queued"), op(11, "git", "Running")]);
    await findByText("Update git: Running");
    const lines = () => container.querySelectorAll("[aria-live]");
    expect(lines()).toHaveLength(1);
    const line = lines()[0];
    expect(line).toHaveAttribute("aria-live", "polite");
    expect(line).toHaveTextContent("Working on 1 of 2");

    await listNow(queryClient, [op(12, "jq", "Running"), op(11, "git", "Done", "Succeeded")]);
    await findByText("Update jq: Running");
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

      await findByText("Update git: Running");
      fireEvent.click(await findByRole("button", { name: "Cancel All" }));
      await waitFor(() => expect(calledToCancel()).toEqual([12, 11]));
    });

    it("says Cancel the rest while one running cannot be stopped, cancels the queued ones, and goes on naming that one", async () => {
      // rustup running; two queued behind it.
      const { findByRole, findByText, getByText, queryByRole, queryClient } = renderWithProviders(<OperationBar />);
      await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));
      await listNow(queryClient, [op(11, "wget", "Queued"), op(10, "jq", "Queued"), rustup(9, "Running")]);

      await findByText("Update rustup: Running");
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
      expect(getByText("Update rustup: Running")).toBeInTheDocument();

      // Only rustup left, which nothing can stop: no button, and the bar
      // goes on naming it until it has finished.
      await listNow(queryClient, [
        op(11, "wget", "Done", "Cancelled"),
        op(10, "jq", "Done", "Cancelled"),
        rustup(9, "Running"),
      ]);
      await waitFor(() => expect(queryByRole("button", { name: /^Stop/ })).toBeNull());
      expect(getByText("Update rustup: Running")).toBeInTheDocument();
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
      await findByText("Update rustup: Running");
      expect(queryByText("Update git: Running")).toBeNull();
      fireEvent.click(getByRole("button", { name: "View Log" }));
      expect(useUiStore.getState().focusedOpId).toBe(12);
      fireEvent.click(getByRole("button", { name: "Cancel the Rest" }));
      await waitFor(() => expect(calledToCancel()).toEqual([13, 11]));

      // git stopping, wget dropped: rustup still on the bar.
      await listNow(queryClient, [op(13, "wget", "CancelRequested"), rustup(12, "Running"), op(11, "git", "Cancelling")]);
      expect(await findByRole("button", { name: "Cancel the Rest" })).toBeDisabled();
      expect(getByText("Update rustup: Running")).toBeInTheDocument();

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

  it("says a run of several all succeeded, or how many were cancelled, with nothing to look at", async () => {
    const { findByText, queryByRole, queryClient } = renderWithProviders(<OperationBar />);
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
    expect(queryByRole("button", { name: "View Log" })).toBeNull();
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
    await findByText("Uninstall Claude Code: Running");

    act(() => {
      queryClient.setQueryData<Snapshot>(queryKeys.snapshot, { ...listed, generation: 2, artifacts: [] });
    });
    await listNow(queryClient, [op(3, "claude", "Done", "Succeeded", uninstall)]);

    await findByText("Uninstall Claude Code: Completed");
  });

  it("disables Cancel while the finished command's result is being verified", async () => {
    // `OperationManager::cancel` (ops/mod.rs) answers `NotPending` for a
    // Verifying op and the IPC turns that into a silent Ok, so an enabled
    // button here would do nothing when clicked.
    operations = [op(8, "jq", "Verifying")];

    const { findByRole, getByText } = renderWithProviders(<OperationBar />);
    expect(await findByRole("button", { name: "Cancel" })).toBeDisabled();
    expect(getByText("Update jq: Checking the result…")).toBeInTheDocument();
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

    await findByText("Update rustup: Running");
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

    await findByText("Update rustup: Queued");
    const cancel = await findByRole("button", { name: "Cancel" });
    expect(cancel).toBeEnabled();
    fireEvent.click(cancel);
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith("cancel_operation", { opId: 8 }),
    );
  });
});
