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
import { failureCause } from "../lib/failureCause";

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

const refused: Outcome = { Failed: { exit_code: 1, summary: "Error: Refusing to uninstall /opt/homebrew/Cellar/python@3.13/3.13.8", cause: failureCause("Error: Refusing to uninstall /opt/homebrew/Cellar/python@3.13/3.13.8") } };

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
let technical: boolean;

beforeEach(() => {
  mockInvoke.mockReset();
  operations = [];
  technical = false;
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
      "python@3.13Couldn't uninstallView Logpipx wasn't uninstalled, and Homebrew doesn't uninstall software that's still needed.",
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

  it("says nothing until the list has every operation it started, even when those listed have all failed", async () => {
    useUiStore.getState().setUninstallBatch(record);
    // pipx and python@3.13 failed at once; wget started after, and the list
    // was fetched before it was.
    operations = [op(12, "python@3.13", "Done", refused), op(11, "pipx", "Done", "Cancelled")];
    const { container, queryClient } = renderWithProviders(<BatchUninstallResult />);
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual(operations));
    expect(container).toBeEmptyDOMElement();

    await listNow(queryClient, [
      op(13, "wget", "Done", "Succeeded"),
      op(12, "python@3.13", "Done", refused),
      op(11, "pipx", "Done", "Cancelled"),
    ]);
    expect(await screen.findByRole("region", { name: "Uninstalled 1; 2 weren't uninstalled" })).toBeInTheDocument();
  });

  it("leaves out an operation the backend no longer lists, older than those it does", async () => {
    useUiStore.getState().setUninstallBatch(record);
    // pipx's (11) was let go of: the backend keeps the newest 200, and 20 is newer.
    operations = [
      op(20, "jq", "Done", "Succeeded"),
      op(13, "wget", "Done", refused),
      op(12, "python@3.13", "Done", "Succeeded"),
    ];
    renderWithProviders(<BatchUninstallResult />);
    const block = await screen.findByRole("region", { name: "Uninstalled 1; 1 wasn't uninstalled" });
    expect(within(block).getByText("wget")).toBeInTheDocument();
    expect(within(block).queryByText("pipx")).toBeNull();
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

  it("says a known cause as what to do, in the label colour with a ⚠︎, and ticks the ones left to try again", async () => {
    useUiStore.getState().setUninstallBatch(record);
    useUiStore.getState().deselectUninstalls(record.items.map((item) => item.key));
    const denied: Outcome = { Failed: { exit_code: 1, summary: "Error: Permission denied @ apply2files - /opt/homebrew/bin/wget", cause: failureCause("Error: Permission denied @ apply2files - /opt/homebrew/bin/wget") } };
    operations = [op(13, "wget", "Done", denied), op(12, "python@3.13", "Done", "Succeeded"), op(11, "pipx", "Done", "Succeeded")];
    renderWithProviders(<BatchUninstallResult />);
    const heading = await screen.findByRole("alert");
    expect(heading.className).not.toMatch(/danger/);
    expect(heading.querySelector("svg")).not.toBeNull();
    const item = document.querySelector("[data-batch-result-item]") as HTMLElement;
    expect(item).toHaveTextContent(/Permission/i);
    expect(item).toHaveTextContent("There's no permission to change its files. Check the permissions, then try again.");
    fireEvent.click(screen.getByRole("button", { name: "Select It Again" }));
    expect(useUiStore.getState().selectedUninstalls).toContainEqual(artifactKeyId(key("wget")));
  });

  it("with technical details on, says under each tool's own words whose they are and what to do next", async () => {
    technical = true;
    const unknown: Outcome = { Failed: { exit_code: 1, summary: "Error: wget: something went wrong", cause: failureCause("Error: wget: something went wrong") } };
    const denied: Outcome = { Failed: { exit_code: 1, summary: "Error: Permission denied @ apply2files - /opt/homebrew/bin/wget", cause: failureCause("Error: Permission denied @ apply2files - /opt/homebrew/bin/wget") } };
    const password: Outcome = {
      Failed: {
        exit_code: 1,
        summary:
          "sudo: a terminal is required to read the password; either use the -S option to read from standard input or configure an askpass helper", cause: failureCause("sudo: a terminal is required to read the password; either use the -S option to read from standard input or configure an askpass helper"),
      },
    };
    // macOS's words for a path it would not move to the Trash: no command ran.
    const trash: Outcome = { Failed: { exit_code: null, summary: "Operation not permitted", cause: failureCause("Operation not permitted") } };
    const sentences = {
      en: {
        generic:
          "The words above are the error message from Homebrew itself. You can uninstall it again later. If it still fails, click View Log, then Copy Log, and send the log to someone who can help.",
        withCause:
          "The words above are the error message from Homebrew itself. There's no permission to change its files. Check the permissions, then try again.",
        inTerminal:
          "The words above are the error message from Homebrew itself. This needs your Mac login password, which can't be entered here. You can click View Steps and run the command it gives in Terminal.",
      },
      "zh-CN": {
        generic: "上面是Homebrew自己的报错。可以稍后重新卸载；还是失败，就点按“查看日志”，再点按“拷贝日志”，发给懂的人看。",
        withCause: "上面是Homebrew自己的报错。没有权限修改它的文件，请确认权限后重试。",
        inTerminal: "上面是Homebrew自己的报错。需要输入Mac的登录密码，无法在这里输入。可以点按“查看步骤”，在终端里运行那里给出的命令。",
      },
    };
    try {
      for (const lang of ["en", "zh-CN"] as const) {
        await i18n.changeLanguage(lang);
        useUiStore.getState().setUninstallBatch(record);
        operations = [op(13, "wget", "Done", unknown), op(12, "python@3.13", "Done", denied), op(11, "pipx", "Done", password)];
        const view = renderWithProviders(<BatchUninstallResult />);
        const items = await waitFor(() => {
          const found = Array.from(document.querySelectorAll<HTMLElement>("[data-batch-result-item]"));
          expect(found).toHaveLength(3);
          return found;
        });
        const byName = (name: string) => items.find((item) => item.textContent?.startsWith(name)) as HTMLElement;
        const stepOf = (name: string) => byName(name).querySelector("[data-failure-next-step]");
        await waitFor(() => expect(stepOf("wget")).toHaveTextContent(sentences[lang].generic));
        // The tool's own words are on the row, over the sentence.
        expect(byName("wget")).toHaveTextContent("Error: wget: something went wrong");
        expect(stepOf("python@3.13")).toHaveTextContent(sentences[lang].withCause);
        expect(stepOf("pipx")).toHaveTextContent(sentences[lang].inTerminal);
        // The button the sentence names is the row's, by that name -- the
        // operation bar's for a password stop (r24 W6) -- and the others'
        // stay View Log.
        const buttonOf = (name: string) => byName(name).querySelector("button")?.textContent;
        expect(buttonOf("pipx")).toBe(i18n.t("needsPassword.viewSteps"));
        expect(buttonOf("wget")).toBe(i18n.t("common.viewLog"));
        expect(buttonOf("python@3.13")).toBe(i18n.t("common.viewLog"));
        expect(byName("pipx").querySelector("button")).toHaveAccessibleName(
          i18n.t("needsPassword.viewStepsLabel", { name: "pipx" }),
        );
        view.unmount();

        operations = [op(13, "wget", "Done", trash), op(12, "python@3.13", "Done", "Cancelled"), op(11, "pipx", "Done", "Succeeded")];
        const none = renderWithProviders(<BatchUninstallResult />);
        await waitFor(() => expect(document.querySelectorAll("[data-batch-result-item]")).toHaveLength(2));
        await waitFor(() => expect(none.getByText(/Operation not permitted/)).toBeInTheDocument());
        expect(document.querySelector("[data-failure-next-step]")).toBeNull();
        none.unmount();
      }
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("says no such sentence with technical details off: the row says the cause, or 「未能卸载」", async () => {
    useUiStore.getState().setUninstallBatch(record);
    const unknown: Outcome = { Failed: { exit_code: 1, summary: "Error: wget: something went wrong", cause: failureCause("Error: wget: something went wrong") } };
    operations = [op(13, "wget", "Done", unknown), op(12, "python@3.13", "Done", "Succeeded"), op(11, "pipx", "Done", "Succeeded")];
    renderWithProviders(<BatchUninstallResult />);
    await screen.findByRole("alert");
    expect(document.querySelector("[data-batch-result-item]")).not.toHaveTextContent(/something went wrong/);
    expect(document.querySelector("[data-failure-next-step]")).toBeNull();
  });
});
