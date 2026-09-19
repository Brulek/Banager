import { describe, expect, it, vi, beforeEach } from "vitest";
import { fireEvent, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { OperationBar } from "./OperationBar";
import { useUiStore } from "../store/ui";

const mockInvoke = vi.mocked(invoke);

beforeEach(() => {
  mockInvoke.mockReset();
  useUiStore.setState({ drawerOpen: false, focusedOpId: null });
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
