import { describe, expect, it, vi, beforeEach } from "vitest";
import { fireEvent, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { OperationBar } from "./OperationBar";
import { useUiStore } from "../store/ui";

const mockInvoke = vi.mocked(invoke);

beforeEach(() => {
  mockInvoke.mockReset();
  useUiStore.setState({ drawerOpen: false, focusedOpId: null, logs: [] });
});

describe("OperationBar", () => {
  it("shows an idle message when nothing is running", async () => {
    mockInvoke.mockResolvedValue([]);
    const { findByText } = renderWithProviders(<OperationBar />);

    await findByText("No operation running");
  });

  it("shows the running operation and cancels it on click", async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "list_operations") {
        return Promise.resolve([
          {
            id: 5,
            kind: "Upgrade",
            instance_id: "brew:/opt/homebrew",
            artifact_kind: "Cask",
            name: "onyx",
            status: "Running",
            outcome: null,
            argv_preview: ["/opt/homebrew/bin/brew", "upgrade", "--cask", "onyx"],
            cancel_policy: "KillThenReconcile",
          },
        ]);
      }
      return Promise.resolve(undefined);
    });

    const { findByRole, findByText } = renderWithProviders(<OperationBar />);
    await findByText("Updating onyx — running");
    fireEvent.click(await findByRole("button", { name: "Cancel" }));

    // This is the only proof in the plan that Cancel really reaches
    // cancel_operation; the mutation calls invoke after awaiting onMutate.
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith("cancel_operation", { opId: 5 }),
    );
  });

  it("shows the wait for a brew update, not just \"running\", while one is in progress", async () => {
    // Item (2) of the loose-ends pass: `LogNote::WaitingForBrewUpdate` used
    // to reach only the log drawer, so this bar -- the only thing on
    // screen before the drawer is opened -- still said "running" while an
    // install waited on Homebrew for up to ten minutes.
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "list_operations") {
        return Promise.resolve([
          {
            id: 5,
            kind: "Upgrade",
            instance_id: "brew:/opt/homebrew",
            artifact_kind: "Cask",
            name: "onyx",
            status: "Running",
            outcome: null,
            argv_preview: ["/opt/homebrew/bin/brew", "upgrade", "--cask", "onyx"],
            cancel_policy: "KillThenReconcile",
          },
        ]);
      }
      return Promise.resolve(undefined);
    });
    useUiStore.getState().appendLog({ opId: 5, note: { WaitingForBrewUpdate: { minutes: 10 } } });

    const { findByRole, findByText } = renderWithProviders(<OperationBar />);
    await findByText("Updating onyx — waiting for Homebrew to finish updating");
    // Cancel must still work: the wait is still part of an active op.
    expect(await findByRole("button", { name: "Cancel" })).toBeEnabled();
  });

  it("shows cancelling, not the brew-update wait, once Cancel is pressed during the wait", async () => {
    // F5 fix: `cancel()` (ops/mod.rs) moves the record straight to
    // CancelRequested when the user presses Cancel, without waiting for
    // `wait_for_update` to notice -- so the last log line an op carries
    // can still be `WaitingForBrewUpdate` even though the op is no longer
    // just waiting. The bar must say "cancelling", not repeat the stale
    // wait note, once status has moved off Running.
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "list_operations") {
        return Promise.resolve([
          {
            id: 5,
            kind: "Upgrade",
            instance_id: "brew:/opt/homebrew",
            artifact_kind: "Cask",
            name: "onyx",
            status: "CancelRequested",
            outcome: null,
            argv_preview: ["/opt/homebrew/bin/brew", "upgrade", "--cask", "onyx"],
            cancel_policy: "KillThenReconcile",
          },
        ]);
      }
      return Promise.resolve(undefined);
    });
    useUiStore.getState().appendLog({ opId: 5, note: { WaitingForBrewUpdate: { minutes: 10 } } });

    const { findByText, queryByText } = renderWithProviders(<OperationBar />);
    await findByText("Updating onyx — cancelling");
    expect(queryByText("Updating onyx — waiting for Homebrew to finish updating")).not.toBeInTheDocument();
  });

  it("keeps a finished operation visible with its outcome and no Cancel button", async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "list_operations") {
        return Promise.resolve([
          {
            id: 6,
            kind: "Install",
            instance_id: "brew:/opt/homebrew",
            artifact_kind: "Formula",
            name: "jqq",
            status: "Done",
            outcome: { Failed: { exit_code: 1, summary: "No available formula with the name \"jqq\"" } },
            argv_preview: ["/opt/homebrew/bin/brew", "install", "--formula", "jqq"],
            cancel_policy: "KillThenReconcile",
          },
        ]);
      }
      return Promise.resolve(undefined);
    });

    const { findByText, queryByRole } = renderWithProviders(<OperationBar />);

    await findByText('Failed: No available formula with the name "jqq"');
    expect(queryByRole("button", { name: "Cancel" })).not.toBeInTheDocument();
  });

  it("disables Cancel while the finished command's result is being verified", async () => {
    // `OperationManager::cancel` (ops/mod.rs) answers `NotPending` for a
    // Verifying op and the IPC turns that into a silent Ok, so an enabled
    // button here would do nothing when clicked.
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "list_operations") {
        return Promise.resolve([
          {
            id: 8,
            kind: "Upgrade",
            instance_id: "brew:/opt/homebrew",
            artifact_kind: "Formula",
            name: "jq",
            status: "Verifying",
            outcome: null,
            argv_preview: ["/opt/homebrew/bin/brew", "upgrade", "--formula", "jq"],
            cancel_policy: "KillThenReconcile",
          },
        ]);
      }
      return Promise.resolve(undefined);
    });

    const { findByRole } = renderWithProviders(<OperationBar />);
    expect(await findByRole("button", { name: "Cancel" })).toBeDisabled();
  });

  it("offers no Cancel button for a running operation whose plan says NoCancel", async () => {
    // `OperationManager::cancel` (ops/mod.rs) refuses such an op once it
    // is Running, so a button here would promise something the backend
    // will not do. No adapter produces `NoCancel` yet; this is the shape a
    // standalone self-updating installer is expected to send.
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "list_operations") {
        return Promise.resolve([
          {
            id: 7,
            kind: "Upgrade",
            instance_id: "claude:/Users/me/.local/bin/claude",
            artifact_kind: "Binary",
            name: "claude",
            status: "Running",
            outcome: null,
            argv_preview: ["/Users/me/.local/bin/claude", "update"],
            cancel_policy: "NoCancel",
          },
        ]);
      }
      return Promise.resolve(undefined);
    });

    const { findByText, queryByRole } = renderWithProviders(<OperationBar />);

    await findByText("Updating claude — running");
    expect(queryByRole("button", { name: "Cancel" })).not.toBeInTheDocument();
  });

  it("offers Cancel for a queued operation whose plan says NoCancel, and it reaches cancel_operation", async () => {
    // A Queued op has started nothing, so `OperationManager::cancel`
    // (ops/mod.rs) accepts its cancel whatever the plan says and the
    // command never runs; NoCancel only bites once the op is Running.
    // Without the button the user could not drop a NoCancel op stuck
    // behind another op's lock, and nothing else bounds that wait.
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "list_operations") {
        return Promise.resolve([
          {
            id: 8,
            kind: "Upgrade",
            instance_id: "claude:/Users/me/.local/bin/claude",
            artifact_kind: "Binary",
            name: "claude",
            status: "Queued",
            outcome: null,
            argv_preview: ["/Users/me/.local/bin/claude", "update"],
            cancel_policy: "NoCancel",
          },
        ]);
      }
      return Promise.resolve(undefined);
    });

    const { findByRole, findByText } = renderWithProviders(<OperationBar />);

    await findByText("Updating claude — queued");
    const cancel = await findByRole("button", { name: "Cancel" });
    expect(cancel).toBeEnabled();
    fireEvent.click(cancel);
    await waitFor(() =>
      expect(mockInvoke).toHaveBeenCalledWith("cancel_operation", { opId: 8 }),
    );
  });
});
