import { describe, expect, it, beforeEach, vi } from "vitest";
import { act } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { LogDrawer } from "./LogDrawer";
import { useUiStore } from "../store/ui";

const mockInvoke = vi.mocked(invoke);

const runningOp = {
  id: 1,
  kind: "Install",
  instance_id: "brew:/opt/homebrew",
  artifact_kind: "Formula",
  name: "jq",
  status: "Running",
  outcome: null,
  argv_preview: ["/opt/homebrew/bin/brew", "install", "--formula", "jq"],
};

beforeEach(() => {
  mockInvoke.mockReset();
  mockInvoke.mockResolvedValue([runningOp]);
  useUiStore.setState({ logs: [], drawerOpen: true, focusedOpId: 1 });
});

describe("LogDrawer", () => {
  it("renders a synthetic sequence of streamed log lines in order", async () => {
    const { findByText, getByRole } = renderWithProviders(<LogDrawer />);

    act(() => {
      useUiStore.getState().appendLog({ opId: 1, stream: "Stdout", line: "Fetching jq" });
      useUiStore.getState().appendLog({ opId: 1, stream: "Stdout", line: "Installing jq" });
      useUiStore
        .getState()
        .appendLog({ opId: 1, stream: "Stderr", line: "warning: cask deprecated" });
    });

    await findByText("warning: cask deprecated");
    // "in order" means the DOM order, not merely that each line exists.
    const lines = Array.from(getByRole("log").querySelectorAll("p")).map((p) => p.textContent);
    expect(lines).toEqual(["Fetching jq", "Installing jq", "warning: cask deprecated"]);
  });

  it("only shows log lines for the focused operation", async () => {
    const { findByText, queryByText } = renderWithProviders(<LogDrawer />);

    act(() => {
      useUiStore.getState().appendLog({ opId: 1, stream: "Stdout", line: "for op 1" });
      useUiStore.getState().appendLog({ opId: 2, stream: "Stdout", line: "for op 2" });
    });

    await findByText("for op 1");
    expect(queryByText("for op 2")).not.toBeInTheDocument();
  });

  it("shows the terminal outcome once the operation finishes", async () => {
    mockInvoke.mockResolvedValue([{ ...runningOp, status: "Done", outcome: "Succeeded" }]);

    const { findByText } = renderWithProviders(<LogDrawer />);

    await findByText("Succeeded");
  });

  it("does not render when the drawer is closed", () => {
    useUiStore.setState({ drawerOpen: false });
    const { queryByRole } = renderWithProviders(<LogDrawer />);

    expect(queryByRole("dialog")).not.toBeInTheDocument();
  });
});
