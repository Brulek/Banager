import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import type { QueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import i18n from "../i18n";
import { BatchUninstallResult } from "./BatchUninstallResult";
import { artifactKeyId, useUiStore, type UninstallBatchRecord } from "../store/ui";
import { queryKeys } from "../lib/queryKeys";
import type { ArtifactKey, OpStatus, OpSummary, Outcome } from "../lib/types";

const mockInvoke = vi.mocked(invoke);

const key = (name: string): ArtifactKey => ({ instance_id: "brew:/opt/homebrew", kind: "Formula", name });

function op(id: number, name: string, status: OpStatus, outcome: Outcome | null = null): OpSummary {
  return {
    id,
    kind: "Uninstall",
    instance_id: "brew:/opt/homebrew",
    artifact_kind: "Formula",
    name,
    status,
    outcome,
    argv_preview: ["/opt/homebrew/bin/brew", "uninstall", name],
    cancel_policy: "KillThenReconcile",
  };
}

const refused: Outcome = { Failed: { exit_code: 1, summary: "Error: Refusing to uninstall /opt/homebrew/Cellar/python@3.13/3.13.8" } };

// pipx first, then python@3.13, which ran after it; then wget, and htop,
// which never started.
const record: UninstallBatchRecord = {
  id: 1,
  items: [
    { key: key("pipx"), name: "pipx", opId: 11, after: [] },
    { key: key("python@3.13"), name: "python@3.13", opId: 12, after: [artifactKeyId(key("pipx"))] },
    { key: key("wget"), name: "wget", opId: 13, after: [] },
    { key: key("htop"), name: "htop", opId: null, after: [] },
  ],
};

let operations: OpSummary[];

beforeEach(() => {
  mockInvoke.mockReset();
  operations = [];
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "list_operations") return Promise.resolve(operations);
    if (cmd === "get_settings") {
      return Promise.resolve({
        language: "System",
        show_technical_details: false,
        ignored_updates: [],
        skipped_versions: [],
        include_self_updating: false,
        auto_check: false,
        notify_updates: false,
      });
    }
    return Promise.resolve(undefined);
  });
});

afterEach(async () => {
  await i18n.changeLanguage("en");
});

async function listNow(queryClient: QueryClient, next: OpSummary[]) {
  operations = next;
  await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.operations }));
}

describe("what a batch uninstall did not uninstall", () => {
  it("says nothing while any of it still runs, then names each one that did not, with how it ended", async () => {
    useUiStore.getState().setUninstallBatch(record);
    operations = [op(13, "wget", "Queued"), op(12, "python@3.13", "Queued"), op(11, "pipx", "Running")];
    const { container, queryClient } = renderWithProviders(<BatchUninstallResult />);
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual(operations));
    expect(container).toBeEmptyDOMElement();

    await listNow(queryClient, [
      op(13, "wget", "Running"),
      op(12, "python@3.13", "Done", refused),
      op(11, "pipx", "Done", "Cancelled"),
    ]);
    expect(container).toBeEmptyDOMElement();

    await listNow(queryClient, [
      op(13, "wget", "Done", "Succeeded"),
      op(12, "python@3.13", "Done", refused),
      op(11, "pipx", "Done", "Cancelled"),
    ]);
    const block = await screen.findByRole("region", { name: "Uninstalled 1; 2 weren't uninstalled" });
    expect(within(block).getByRole("alert")).toHaveTextContent("Uninstalled 1; 2 weren't uninstalled");
    const items = [...block.querySelectorAll("[data-batch-result-item]")] as HTMLElement[];
    expect(items.map((item) => item.textContent)).toEqual([
      "pipxCancelledView Log",
      "python@3.13Couldn't finishView Logpipx wasn't uninstalled, and Homebrew doesn't uninstall software that's still needed.",
    ]);
    // Neither wget, which succeeded, nor htop, which never started.
    expect(within(block).queryByText("wget")).toBeNull();
    expect(within(block).queryByText("htop")).toBeNull();
    // Nothing opened the log by itself; its button does.
    expect(useUiStore.getState().drawerOpen).toBe(false);
    fireEvent.click(within(block).getByRole("button", { name: "View the log of python@3.13" }));
    expect(useUiStore.getState().focusedOpId).toBe(12);
    expect(useUiStore.getState().drawerOpen).toBe(true);
  });

  it("says nothing when everything it started succeeded", async () => {
    useUiStore.getState().setUninstallBatch(record);
    operations = [op(13, "wget", "Done", "Succeeded"), op(12, "python@3.13", "Done", "Succeeded"), op(11, "pipx", "Done", "Succeeded")];
    const { container, queryClient } = renderWithProviders(<BatchUninstallResult />);
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual(operations));
    expect(container).toBeEmptyDOMElement();
  });

  it("goes with its ×, and gives way to the next batch's", async () => {
    useUiStore.getState().setUninstallBatch(record);
    operations = [op(13, "wget", "Done", refused), op(12, "python@3.13", "Done", "Succeeded"), op(11, "pipx", "Done", "Succeeded")];
    renderWithProviders(<BatchUninstallResult />);
    const block = await screen.findByRole("region", { name: "Uninstalled 2; 1 wasn't uninstalled" });
    expect(within(block).getByText("wget")).toBeInTheDocument();
    act(() =>
      useUiStore.getState().setUninstallBatch({ id: 2, items: [{ key: key("pipx"), name: "pipx", opId: 11, after: [] }] }),
    );
    await waitFor(() => expect(screen.queryByRole("region")).toBeNull());

    act(() => useUiStore.getState().setUninstallBatch(record));
    fireEvent.click(await screen.findByRole("button", { name: "Close uninstall results" }));
    expect(useUiStore.getState().uninstallBatch).toBeNull();
    await waitFor(() => expect(screen.queryByRole("region")).toBeNull());
  });

  it("hands the focus its × had to the page's title, not the window's body", async () => {
    useUiStore.getState().setUninstallBatch(record);
    operations = [op(13, "wget", "Done", refused), op(12, "python@3.13", "Done", "Succeeded"), op(11, "pipx", "Done", "Succeeded")];
    renderWithProviders(
      <>
        <h1 tabIndex={-1} data-focus-fallback="">
          Installed
        </h1>
        <BatchUninstallResult />
      </>,
    );
    const close = await screen.findByRole("button", { name: "Close uninstall results" });
    close.focus();
    fireEvent.click(close);
    await waitFor(() => expect(screen.queryByRole("region")).toBeNull());
    expect(screen.getByRole("heading", { name: "Installed" })).toHaveFocus();
  });

  it("says only how many weren't uninstalled when none was", async () => {
    useUiStore.getState().setUninstallBatch(record);
    operations = [op(13, "wget", "Done", refused), op(12, "python@3.13", "Done", refused), op(11, "pipx", "Done", "Cancelled")];
    renderWithProviders(<BatchUninstallResult />);
    const block = await screen.findByRole("region", { name: "3 weren't uninstalled" });
    expect(within(block).getByRole("alert")).toHaveTextContent(/^3 weren't uninstalled$/);
  });

  it("says it in Chinese", async () => {
    await i18n.changeLanguage("zh-CN");
    useUiStore.getState().setUninstallBatch(record);
    operations = [op(13, "wget", "Done", "Succeeded"), op(12, "python@3.13", "Done", refused), op(11, "pipx", "Done", refused)];
    renderWithProviders(<BatchUninstallResult />);
    const block = await screen.findByRole("region", { name: "已卸载1个，2个没有卸载" });
    expect(within(block).getByText("“pipx”没有卸载。还有软件要用到它们，Homebrew不会卸载。")).toBeInTheDocument();
    expect(within(block).getByRole("button", { name: "查看“python@3.13”的日志" })).toHaveTextContent("查看日志");
  });
});
