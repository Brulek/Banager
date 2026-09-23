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
          },
        ]);
      }
      return Promise.resolve(undefined);
    });

    const { findByText, queryByRole } = renderWithProviders(<OperationBar />);

    await findByText('Failed: No available formula with the name "jqq"');
    expect(queryByRole("button", { name: "Cancel" })).not.toBeInTheDocument();
  });
});
