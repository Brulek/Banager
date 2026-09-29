import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { QueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { fakeMenuBar } from "../test/menuBar";
import { QUIT_REQUESTED_EVENT } from "../lib/api";
import { queryKeys } from "../lib/queryKeys";
import i18n from "../i18n";
import { QuitQuestion } from "./QuitQuestion";
import type { OpStatus, OpSummary, Snapshot } from "../lib/types";

const mockInvoke = vi.mocked(invoke);

// What `list_operations` answers, and what `get_snapshot` answers --
// nothing, until a test gives it one: the question reads it only for the
// names the rows show.
let operations: OpSummary[];
let snapshot: Snapshot | null;
// What `quit_anyway` answers: at once, unless a test holds it.
let quitReply: () => Promise<void>;

function op(id: number, name: string, status: OpStatus, extra: Partial<OpSummary> = {}): OpSummary {
  return {
    id,
    kind: "Upgrade",
    instance_id: "brew:/opt/homebrew",
    artifact_kind: "Formula",
    name,
    status,
    outcome: status === "Done" ? "Succeeded" : null,
    argv_preview: [],
    cancel_policy: "KillThenReconcile",
    ...extra,
  };
}

/** rustup's self update, which cannot be cancelled once it has started. */
function rustup(id: number, status: OpStatus): OpSummary {
  return op(id, "rustup", status, {
    instance_id: "standalone-rustup",
    artifact_kind: "Binary",
    cancel_policy: "NoCancel",
  });
}

beforeEach(() => {
  operations = [];
  snapshot = null;
  quitReply = () => Promise.resolve();
  mockInvoke.mockReset();
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "list_operations") return Promise.resolve(operations);
    // Never answered without a snapshot: an answer of nothing would be an
    // error to the query, which is not what these tests are about.
    if (cmd === "get_snapshot") return snapshot === null ? new Promise(() => {}) : Promise.resolve(snapshot);
    if (cmd === "quit_anyway") return quitReply();
    return Promise.resolve(undefined);
  });
});

/** How many times the page sent `cmd`. */
function sent(cmd: string): number {
  return mockInvoke.mock.calls.filter(([called]) => called === cmd).length;
}

/**
 * The question, mounted, once it listens for Rust's question and has told
 * Rust to ask (`ask_before_quit`); and Rust, to ask it.
 */
async function mounted() {
  const rust = fakeMenuBar();
  const rendered = renderWithProviders(<QuitQuestion />);
  await waitFor(() => expect(sent("ask_before_quit")).toBe(1));
  return { rust, ...rendered };
}

/** Rust asks, as a quit comes while something is under way; resolves to the question. */
async function asked(rust: ReturnType<typeof fakeMenuBar>, name: string | RegExp): Promise<HTMLElement> {
  rust.hear(QUIT_REQUESTED_EVENT);
  return screen.findByRole("dialog", { name });
}

/** The backend's list has moved on: what a `Status` or `Finished` event makes the page ask again. */
async function listNow(queryClient: QueryClient, next: OpSummary[]) {
  operations = next;
  await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.operations }));
  // And the page has drawn it: the query tells the page in a task of its own.
  await act(() => new Promise((resolve) => setTimeout(resolve, 10)));
}

describe("the question before a quit", () => {
  it("draws nothing until Rust asks", async () => {
    operations = [op(1, "wget", "Running")];
    await mounted();

    expect(screen.queryByRole("dialog")).toBeNull();
    expect(sent("quit_anyway")).toBe(0);
  });

  it("asks while an update runs: how many, what quitting does, and its two buttons, Keep waiting in focus", async () => {
    operations = [op(1, "wget", "Running")];
    const { rust } = await mounted();

    const dialog = await asked(rust, "1 operation hasn't finished");

    expect(
      within(dialog).getByText("Quitting now stops it, and the tool it's updating can be left half-updated."),
    ).toBeInTheDocument();
    // The quiet one first, then the one it asks for, as every sheet has them.
    expect(within(dialog).getAllByRole("button").map((button) => button.textContent)).toEqual([
      "Quit anyway",
      "Keep waiting",
    ]);
    const keepWaiting = within(dialog).getByRole("button", { name: "Keep waiting" });
    await waitFor(() => expect(document.activeElement).toBe(keepWaiting));
    expect(sent("quit_anyway")).toBe(0);
  });

  it.each([
    ["Keep waiting", async (user: ReturnType<typeof userEvent.setup>, dialog: HTMLElement) =>
      user.click(within(dialog).getByRole("button", { name: "Keep waiting" }))],
    ["Escape", async (user: ReturnType<typeof userEvent.setup>) => user.keyboard("{Escape}")],
    ["Return on the button in focus", async (user: ReturnType<typeof userEvent.setup>, dialog: HTMLElement) => {
      await waitFor(() =>
        expect(document.activeElement).toBe(within(dialog).getByRole("button", { name: "Keep waiting" })),
      );
      await user.keyboard("{Enter}");
    }],
  ])("goes away on %s, and Canager stays", async (_how, answer) => {
    const user = userEvent.setup();
    operations = [op(1, "wget", "Running"), op(2, "jq", "Queued")];
    const { rust } = await mounted();
    const dialog = await asked(rust, "2 operations haven't finished");

    await answer(user, dialog);

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(sent("quit_anyway")).toBe(0);
  });

  it("asks again at the next quit after Keep waiting", async () => {
    const user = userEvent.setup();
    operations = [op(1, "wget", "Running")];
    const { rust } = await mounted();
    const first = await asked(rust, "1 operation hasn't finished");
    await user.click(within(first).getByRole("button", { name: "Keep waiting" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());

    await asked(rust, "1 operation hasn't finished");

    expect(sent("quit_anyway")).toBe(0);
  });

  it("quits on Quit anyway, both buttons held while it does", async () => {
    const user = userEvent.setup();
    let quit: () => void = () => {};
    quitReply = () =>
      new Promise((resolve) => {
        quit = resolve;
      });
    operations = [op(1, "wget", "Running")];
    const { rust } = await mounted();
    const dialog = await asked(rust, "1 operation hasn't finished");

    await user.click(within(dialog).getByRole("button", { name: "Quit anyway" }));

    expect(sent("quit_anyway")).toBe(1);
    expect(within(dialog).getByRole("button", { name: "Quit anyway" })).toBeDisabled();
    expect(within(dialog).getByRole("button", { name: "Keep waiting" })).toBeDisabled();
    // In the app, Canager is gone by now.
    await act(async () => quit());
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(sent("quit_anyway")).toBe(1);
  });

  it("asks again, the buttons back, should quitting fail", async () => {
    const user = userEvent.setup();
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    quitReply = () => Promise.reject("quit_anyway went wrong");
    operations = [op(1, "wget", "Running")];
    const { rust } = await mounted();
    const dialog = await asked(rust, "1 operation hasn't finished");

    await user.click(within(dialog).getByRole("button", { name: "Quit anyway" }));

    await waitFor(() => expect(within(dialog).getByRole("button", { name: "Quit anyway" })).toBeEnabled());
    expect(screen.getByRole("dialog", { name: "1 operation hasn't finished" })).toBe(dialog);
    expect(error).toHaveBeenCalledWith("quit_anyway failed", expect.any(Error));
    error.mockRestore();
  });

  it("quits without asking when nothing is left undone by the time it looks", async () => {
    // Rust saw one not done; by the time the page asked the backend, it
    // had finished. The user asked to quit, and nothing holds it now.
    operations = [op(1, "wget", "Running")];
    const { rust, queryClient } = await mounted();
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual(operations));
    operations = [op(1, "wget", "Done")];

    rust.hear(QUIT_REQUESTED_EVENT);

    await waitFor(() => expect(sent("quit_anyway")).toBe(1));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("goes by the backend's list when Rust asks, not one an event has not refetched yet", async () => {
    // Update was pressed a moment ago: the page's list does not have it yet.
    const { rust, queryClient } = await mounted();
    await waitFor(() => expect(queryClient.getQueryData(queryKeys.operations)).toEqual([]));
    operations = [op(1, "wget", "Queued")];

    await asked(rust, "1 operation hasn't finished");

    expect(sent("quit_anyway")).toBe(0);
  });

  it("counts them as they go on, and goes away by itself once every one has finished", async () => {
    operations = [op(1, "wget", "Running"), op(2, "jq", "Queued")];
    const { rust, queryClient } = await mounted();
    await asked(rust, "2 operations haven't finished");

    await listNow(queryClient, [op(1, "wget", "Done"), op(2, "jq", "Running")]);
    expect(await screen.findByRole("dialog", { name: "1 operation hasn't finished" })).toBeInTheDocument();

    await listNow(queryClient, [op(1, "wget", "Done"), op(2, "jq", "Done")]);
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    // Nothing is left to wait for, and Canager stays, with how they went
    // on the operation bar.
    expect(sent("quit_anyway")).toBe(0);

    // Nor does it come back by itself when something starts again.
    await listNow(queryClient, [op(3, "git", "Running")]);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("asks once, however many times Rust asks", async () => {
    operations = [op(1, "wget", "Running")];
    const { rust } = await mounted();
    await asked(rust, "1 operation hasn't finished");

    rust.hear(QUIT_REQUESTED_EVENT);
    rust.hear(QUIT_REQUESTED_EVENT);

    await waitFor(() => expect(sent("list_operations")).toBeGreaterThanOrEqual(3));
    expect(screen.getAllByRole("dialog")).toHaveLength(1);
  });

  it("says so of one that has started and that nothing can stop, and not of one still queued", async () => {
    operations = [rustup(1, "Running"), op(2, "wget", "Queued")];
    const { rust, queryClient } = await mounted();
    const dialog = await asked(rust, "2 operations haven't finished");

    expect(
      within(dialog).getByText(
        "rustup's update has started and can't be cancelled. Wait for it to finish before you quit.",
      ),
    ).toBeInTheDocument();

    // Queued, it can still be cancelled: nothing is said of it.
    await listNow(queryClient, [rustup(3, "Queued"), op(2, "wget", "Running")]);
    await waitFor(() => expect(within(dialog).queryByText(/can't be cancelled/)).toBeNull());
  });

  it("names the one that nothing can stop as its row does", async () => {
    snapshot = {
      generation: 1,
      round: 1,
      detect: "Found",
      instances: [],
      artifacts: [
        {
          key: { instance_id: "standalone-rustup", kind: "Binary", name: "rustup" },
          display_name: "Rust toolchain installer",
          version: "1.29.1",
          reason: "Requested",
          description: null,
          homepage: null,
          size_bytes: null,
          installed_at: null,
          path: null,
          auto_updates: false,
          uninstall_blocked: null,
        },
      ],
      updates: [],
      refreshed_at: 1790586000,
      stale: false,
      errors: [],
    };
    operations = [rustup(1, "Running")];
    const { rust } = await mounted();
    const dialog = await asked(rust, "1 operation hasn't finished");

    expect(
      await within(dialog).findByText(
        "Rust toolchain installer's update has started and can't be cancelled. Wait for it to finish before you quit.",
      ),
    ).toBeInTheDocument();
  });

  it.each([
    ["only that quitting stops them, when nothing has started", [op(1, "wget", "Queued"), op(2, "jq", "Verifying")], "Quitting now stops them."],
    [
      "what an uninstall may leave",
      [op(1, "wget", "Running", { kind: "Uninstall" })],
      "Quitting now stops it, and the tool it's uninstalling can be left half-removed.",
    ],
    [
      "what an update being cancelled may leave",
      [op(1, "wget", "CancelRequested"), op(2, "jq", "Queued", { kind: "Uninstall" })],
      "Quitting now stops them, and a tool that's being updated can be left half-updated.",
    ],
    [
      "it in words for any kind, when an update and an uninstall both run",
      [op(1, "wget", "Running"), op(2, "jq", "Running", { kind: "Uninstall" })],
      "Quitting now stops them, and the tools they're working on can be left half-done.",
    ],
  ])("says %s", async (_what, running, line) => {
    operations = running;
    const { rust } = await mounted();
    const dialog = await asked(rust, /hasn't finished|haven't finished/);

    expect(within(dialog).getByText(line)).toBeInTheDocument();
  });

  it("asks in Chinese", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      operations = [rustup(1, "Running"), op(2, "wget", "Running")];
      const { rust } = await mounted();

      const dialog = await asked(rust, "还有 2 个操作没完成");

      expect(within(dialog).getByText("现在退出会中断操作，正在更新的工具有只更新一半的风险。")).toBeInTheDocument();
      expect(within(dialog).getByText("rustup 的更新已经开始，不能取消，请等它完成再退出。")).toBeInTheDocument();
      expect(within(dialog).getAllByRole("button").map((button) => button.textContent)).toEqual([
        "仍然退出",
        "继续等待",
      ]);
      await waitFor(() =>
        expect(document.activeElement).toBe(within(dialog).getByRole("button", { name: "继续等待" })),
      );
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it.each([
    ["nothing started", [op(1, "wget", "Queued")], "现在退出会中断操作。"],
    [
      "an uninstall",
      [op(1, "wget", "Running", { kind: "Uninstall" })],
      "现在退出会中断操作，正在卸载的工具有只卸载一半的风险。",
    ],
    [
      "an update and an uninstall",
      [op(1, "wget", "Running"), op(2, "jq", "Running", { kind: "Uninstall" })],
      "现在退出会中断操作，正在处理的工具有只完成一半的风险。",
    ],
  ])("says in Chinese what quitting does to %s", async (_what, running, line) => {
    await i18n.changeLanguage("zh-CN");
    try {
      operations = running;
      const { rust } = await mounted();

      const dialog = await asked(rust, `还有 ${running.length} 个操作没完成`);

      expect(within(dialog).getByText(line)).toBeInTheDocument();
    } finally {
      await i18n.changeLanguage("en");
    }
  });
});
