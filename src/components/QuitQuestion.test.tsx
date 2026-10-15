import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { QueryClient } from "@tanstack/react-query";
import { invoke, type InvokeArgs } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { fakeMenuBar } from "../test/menuBar";
import { QUIT_REQUESTED_EVENT } from "../lib/api";
import { queryKeys } from "../lib/queryKeys";
import i18n from "../i18n";
import { QuitQuestion } from "./QuitQuestion";
import { BUTTON } from "./ui/controls";
import type { OpStatus, OpSummary, Snapshot } from "../lib/types";
import { NO_FACTS } from "../lib/types";

const mockInvoke = vi.mocked(invoke);

// What `list_operations` answers, and what `get_snapshot` answers --
// nothing, until a test gives it one: the question reads it only for the
// names the rows show.
let operations: OpSummary[];
let snapshot: Snapshot | null;
// What `quit_anyway` answers: at once, unless a test holds it.
let quitReply: () => Promise<void>;
// What `quit_question_shown` answers: at once, unless a test fails it.
let shownReply: () => Promise<void>;
// Each `quit_question_shown` the page sent: the question's number, and
// whether the sheet was in the page as it did.
let shown: { question: unknown; onScreen: boolean }[];
// The number Rust gave the newest question (`asked`).
let questions: number;

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
  shownReply = () => Promise.resolve();
  shown = [];
  questions = 0;
  mockInvoke.mockReset();
  mockInvoke.mockImplementation((cmd: string, args?: InvokeArgs) => {
    if (cmd === "list_operations") return Promise.resolve(operations);
    // Never answered without a snapshot: an answer of nothing would be an
    // error to the query, which is not what these tests are about.
    if (cmd === "get_snapshot") return snapshot === null ? new Promise(() => {}) : Promise.resolve(snapshot);
    if (cmd === "quit_anyway") return quitReply();
    if (cmd === "quit_question_shown") {
      const { question } = args as { question: unknown };
      shown.push({ question, onScreen: screen.queryByRole("alertdialog") !== null });
      return shownReply();
    }
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

/** Rust asks, as a quit comes, with the question's number, counting from 1. */
function ask(rust: ReturnType<typeof fakeMenuBar>): number {
  questions += 1;
  rust.hear(QUIT_REQUESTED_EVENT, questions);
  return questions;
}

/** Rust asks, as a quit comes while something is under way; resolves to the question. */
async function asked(rust: ReturnType<typeof fakeMenuBar>, name: string | RegExp): Promise<HTMLElement> {
  ask(rust);
  return screen.findByRole("alertdialog", { name });
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

    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(sent("quit_anyway")).toBe(0);
    expect(sent("quit_question_shown")).toBe(0);
  });

  it("asks while an update runs: how many, what quitting does, and its two buttons, Keep waiting in focus", async () => {
    operations = [op(1, "wget", "Running")];
    const { rust } = await mounted();

    const dialog = await asked(rust, "1 operation hasn't finished");

    expect(
      within(dialog).getByText("Quitting now stops it, and the tool it's updating can be left half-updated."),
    ).toBeInTheDocument();
    // Its text describes it to a screen reader as it opens.
    expect(dialog).toHaveAccessibleDescription(
      "Quitting now stops it, and the tool it's updating can be left half-updated.",
    );
    // One over the other, as wide as the dialog, the default on top: as
    // NSAlert stacks answers too long to stand side by side (spec §3.6).
    expect(within(dialog).getAllByRole("button").map((button) => button.textContent)).toEqual([
      "Keep Waiting",
      "Quit",
    ]);
    const footer = dialog.querySelector("[data-dialog-footer]") as HTMLElement;
    expect(footer.className).toMatch(/\bflex-col\b/);
    expect(footer.className).toMatch(/\[&>button\]:w-full/);
    const keepWaiting = within(dialog).getByRole("button", { name: "Keep Waiting" });
    expect(keepWaiting.className).toBe(BUTTON.large.default);
    expect(within(dialog).getByRole("button", { name: "Quit" }).className).toBe(BUTTON.large.grey);
    await waitFor(() => expect(document.activeElement).toBe(keepWaiting));
    expect(sent("quit_anyway")).toBe(0);
  });

  it.each([
    ["Keep Waiting", async (user: ReturnType<typeof userEvent.setup>, dialog: HTMLElement) =>
      user.click(within(dialog).getByRole("button", { name: "Keep Waiting" }))],
    ["Escape", async (user: ReturnType<typeof userEvent.setup>) => user.keyboard("{Escape}")],
    ["Return on the button in focus", async (user: ReturnType<typeof userEvent.setup>, dialog: HTMLElement) => {
      await waitFor(() =>
        expect(document.activeElement).toBe(within(dialog).getByRole("button", { name: "Keep Waiting" })),
      );
      await user.keyboard("{Enter}");
    }],
  ])("goes away on %s, and Banager stays", async (_how, answer) => {
    const user = userEvent.setup();
    operations = [op(1, "wget", "Running"), op(2, "jq", "Queued")];
    const { rust } = await mounted();
    const dialog = await asked(rust, "2 operations haven't finished");

    await answer(user, dialog);

    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(sent("quit_anyway")).toBe(0);
    // Rust is told, so that its wait for word from the page does not quit.
    expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "quit_kept_waiting")).toEqual([
      ["quit_kept_waiting", { question: 1 }],
    ]);
  });

  it("is an alert dialog, as NSAlert reads, described by what quitting now would do", async () => {
    // Decision I21c. With nothing quitting can stop, the line about what
    // nothing can stop is what it says.
    operations = [rustup(1, "Running")];
    const { rust, queryClient } = await mounted();
    ask(rust);
    const dialog = await screen.findByRole("alertdialog", { name: "1 operation hasn't finished" });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(dialog).toHaveAccessibleDescription(
      "rustup's update has started and can't be cancelled. Wait for it to finish before you quit.",
    );

    await listNow(queryClient, [op(2, "wget", "Running")]);
    await waitFor(() =>
      expect(dialog).toHaveAccessibleDescription(
        "Quitting now stops it, and the tool it's updating can be left half-updated.",
      ),
    );
  });

  it("asks again at the next quit after Keep waiting", async () => {
    const user = userEvent.setup();
    operations = [op(1, "wget", "Running")];
    const { rust } = await mounted();
    const first = await asked(rust, "1 operation hasn't finished");
    await user.click(within(first).getByRole("button", { name: "Keep Waiting" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());

    await asked(rust, "1 operation hasn't finished");

    expect(sent("quit_anyway")).toBe(0);
    // Each time on screen, and said so, by the question it answers.
    await waitFor(() => expect(shown.map(({ question }) => question)).toEqual([1, 2]));
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

    await user.click(within(dialog).getByRole("button", { name: "Quit" }));

    expect(sent("quit_anyway")).toBe(1);
    expect(within(dialog).getByRole("button", { name: "Quit" })).toBeDisabled();
    expect(within(dialog).getByRole("button", { name: "Keep Waiting" })).toBeDisabled();
    // In the app, Banager is gone by now.
    await act(async () => quit());
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(sent("quit_anyway")).toBe(1);
  });

  it("does not tell Rust it keeps waiting when the operations Quit anyway cancels finish", async () => {
    // Rust cancels them, and waits for them to stop before it quits.
    const user = userEvent.setup();
    quitReply = () => new Promise(() => {});
    operations = [op(1, "wget", "Running")];
    const { rust, queryClient } = await mounted();
    const dialog = await asked(rust, "1 operation hasn't finished");

    await user.click(within(dialog).getByRole("button", { name: "Quit" }));
    await listNow(queryClient, [op(1, "wget", "Done")]);

    expect(sent("quit_kept_waiting")).toBe(0);
  });

  it("asks again, the buttons back, should quitting fail", async () => {
    const user = userEvent.setup();
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    quitReply = () => Promise.reject("quit_anyway went wrong");
    operations = [op(1, "wget", "Running")];
    const { rust } = await mounted();
    const dialog = await asked(rust, "1 operation hasn't finished");

    await user.click(within(dialog).getByRole("button", { name: "Quit" }));

    await waitFor(() => expect(within(dialog).getByRole("button", { name: "Quit" })).toBeEnabled());
    expect(screen.getByRole("alertdialog", { name: "1 operation hasn't finished" })).toBe(dialog);
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

    ask(rust);

    await waitFor(() => expect(sent("quit_anyway")).toBe(1));
    expect(screen.queryByRole("alertdialog")).toBeNull();
    // Nothing on screen, and nothing said: Banager is quitting.
    expect(sent("quit_question_shown")).toBe(0);
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
    expect(await screen.findByRole("alertdialog", { name: "1 operation hasn't finished" })).toBeInTheDocument();

    await listNow(queryClient, [op(1, "wget", "Done"), op(2, "jq", "Done")]);
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    // Nothing is left to wait for, and Banager stays, with how they went
    // on the operation bar -- Rust told so, once.
    expect(sent("quit_anyway")).toBe(0);
    expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "quit_kept_waiting")).toEqual([
      ["quit_kept_waiting", { question: 1 }],
    ]);

    // Nor does it come back by itself when something starts again.
    await listNow(queryClient, [op(3, "git", "Running")]);
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("counts every unfinished uninstall of a batch, which submits them all at once", async () => {
    const uninstall = { kind: "Uninstall" as const };
    operations = [
      op(1, "pipx", "Done", uninstall),
      op(2, "wget", "Running", uninstall),
      op(3, "jq", "Queued", uninstall),
      op(4, "python@3.13", "Queued", uninstall),
    ];
    const { rust } = await mounted();
    expect(await asked(rust, "3 operations haven't finished")).toBeInTheDocument();
  });

  it("asks once, however many times Rust asks", async () => {
    operations = [op(1, "wget", "Running")];
    const { rust } = await mounted();
    await asked(rust, "1 operation hasn't finished");

    ask(rust);
    ask(rust);

    await waitFor(() => expect(sent("list_operations")).toBeGreaterThanOrEqual(3));
    expect(screen.getAllByRole("alertdialog")).toHaveLength(1);
    // The one sheet answers every quit: said so up to the newest, whose
    // number stands for those before it (`QuitGuard::shown`).
    await waitFor(() => expect(shown[shown.length - 1]).toEqual({ question: 3, onScreen: true }));
    expect(shown.every(({ onScreen }) => onScreen)).toBe(true);
  });

  it("tells Rust the question is on screen once it is, by the number Rust gave it", async () => {
    // Rust quits 2 seconds after asking unless told so: a page that never
    // shows the question is not there to answer it (src-tauri/src/quit.rs).
    operations = [op(1, "wget", "Running")];
    const { rust } = await mounted();
    questions = 6;

    await asked(rust, "1 operation hasn't finished");

    await waitFor(() => expect(shown).toEqual([{ question: 7, onScreen: true }]));
  });

  it("stays on screen, to be answered, when Rust cannot be told it is, having tried twice", async () => {
    // Rust then quits once its wait is over, as it does with nobody here --
    // unless Keep waiting reaches it first.
    const user = userEvent.setup();
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    shownReply = () => Promise.reject("quit_question_shown went wrong");
    operations = [op(1, "wget", "Running")];
    const { rust } = await mounted();

    const dialog = await asked(rust, "1 operation hasn't finished");

    await waitFor(() =>
      expect(error).toHaveBeenCalledWith("quit_question_shown failed", new Error("quit_question_shown went wrong")),
    );
    expect(error).toHaveBeenCalledWith(
      "quit_question_shown failed, sending it once more",
      new Error("quit_question_shown went wrong"),
    );
    expect(sent("quit_question_shown")).toBe(2);
    expect(screen.getByRole("alertdialog", { name: "1 operation hasn't finished" })).toBe(dialog);
    expect(sent("quit_anyway")).toBe(0);

    await user.click(within(dialog).getByRole("button", { name: "Keep Waiting" }));
    await waitFor(() => expect(sent("quit_kept_waiting")).toBe(1));
    error.mockRestore();
  });

  it("sends the word that the question is on screen once more when it fails once", async () => {
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    let replies = 0;
    shownReply = () => (++replies === 1 ? Promise.reject("busy") : Promise.resolve());
    operations = [op(1, "wget", "Running")];
    const { rust } = await mounted();

    await asked(rust, "1 operation hasn't finished");

    await waitFor(() => expect(shown).toEqual([
      { question: 1, onScreen: true },
      { question: 1, onScreen: true },
    ]));
    expect(error).toHaveBeenCalledTimes(1);
    error.mockRestore();
  });

  it("stops Rust asking once it is gone from the page", async () => {
    operations = [op(1, "wget", "Running")];
    const { unmount } = await mounted();

    unmount();

    expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "ask_before_quit")).toEqual([
      ["ask_before_quit", { ask: true }],
      ["ask_before_quit", { ask: false }],
    ]);
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
    // Quitting stops the queued one, and not rustup's update.
    expect(within(dialog).getByText("Quitting now stops the other one.")).toBeInTheDocument();
    expect(within(dialog).queryByText(/Quitting now stops it/)).toBeNull();

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
          facts: NO_FACTS,
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
    // Quitting does not stop it: no line says it does.
    expect(within(dialog).queryByText(/Quitting now stops/)).toBeNull();

    expect(
      await within(dialog).findByText(
        "Rust toolchain installer's update has started and can't be cancelled. Wait for it to finish before you quit.",
      ),
    ).toBeInTheDocument();
  });

  it.each([
    ["only that quitting stops them, when nothing has started", [op(1, "wget", "Queued"), op(2, "jq", "Queued")], "Quitting now stops them."],
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

  it("says that quitting waits for one checking its result, not that it stops it", async () => {
    // r38 skeptic 1: a quit lets it finish -- 7 seconds at the most -- so
    // that its record is kept (src-tauri/src/quit.rs, `waits_for`).
    operations = [op(1, "jq", "Verifying")];
    const { rust, queryClient } = await mounted();
    const dialog = await asked(rust, "1 operation hasn't finished");

    const line = "The result of jq's update is being checked. Quitting now waits up to a few seconds for that.";
    expect(within(dialog).getByText(line)).toBeInTheDocument();
    expect(within(dialog).queryByText(/Quitting now stops/)).toBeNull();
    expect(dialog).toHaveAccessibleDescription(line);

    // Beside one that quitting stops, that one is "the other one".
    await listNow(queryClient, [op(1, "jq", "Verifying"), op(2, "wget", "Queued", { kind: "Uninstall" })]);
    await waitFor(() => expect(within(dialog).getByText("Quitting now stops the other one.")).toBeInTheDocument());
    expect(dialog).toHaveAccessibleDescription(`Quitting now stops the other one. ${line}`);
  });

  it.each([
    ["Install", "The result of jq's install is being checked. Quitting now waits up to a few seconds for that."],
    ["Uninstall", "The result of jq's uninstall is being checked. Quitting now waits up to a few seconds for that."],
    ["Link", "The result of jq's link is being checked. Quitting now waits up to a few seconds for that."],
  ] as const)("says it of an operation checking its result in the words of its kind: %s", async (kind, line) => {
    operations = [op(1, "jq", "Verifying", { kind })];
    const { rust } = await mounted();
    const dialog = await asked(rust, "1 operation hasn't finished");

    expect(within(dialog).getByText(line)).toBeInTheDocument();
  });

  it.each([
    ["zh-CN", "还有1个操作未完成", "正在核对“jq”的更新结果。现在退出会先等它核对完，最多几秒。"],
    ["zh-Hant", "還有1個操作未完成", "正在核對「jq」的更新結果。現在結束會先等它核對完，最多幾秒。"],
  ])("says in %s that quitting waits for one checking its result", async (language, title, line) => {
    await i18n.changeLanguage(language);
    try {
      operations = [op(1, "jq", "Verifying")];
      const { rust } = await mounted();
      const dialog = await asked(rust, title);

      expect(within(dialog).getByText(line)).toBeInTheDocument();
      expect(within(dialog).queryByText(/中断|中斷/)).toBeNull();
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("asks in Chinese", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      operations = [rustup(1, "Running"), op(2, "wget", "Running")];
      const { rust } = await mounted();

      const dialog = await asked(rust, "还有2个操作未完成");

      // Quitting stops wget's update, and not rustup's.
      expect(within(dialog).getByText("现在退出会中断其余操作，正在处理的工具有只完成一半的风险。")).toBeInTheDocument();
      expect(within(dialog).getByText("“rustup”的更新已开始，无法取消。请等它完成后再退出。")).toBeInTheDocument();
      expect(within(dialog).getAllByRole("button").map((button) => button.textContent)).toEqual([
        "继续等待",
        "退出",
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

      const dialog = await asked(rust, `还有${running.length}个操作未完成`);

      expect(within(dialog).getByText(line)).toBeInTheDocument();
    } finally {
      await i18n.changeLanguage("en");
    }
  });
});

describe("quit request identity", () => {
  async function heldRequests() {
    operations = [op(1, "wget", "Running")];
    const mountedQuestion = await mounted();
    await waitFor(() => expect(mountedQuestion.queryClient.getQueryData(queryKeys.operations)).toEqual(operations));
    const replies: ((value: OpSummary[]) => void)[] = [];
    const original = mockInvoke.getMockImplementation()!;
    mockInvoke.mockImplementation((cmd, args) => cmd === "list_operations"
      ? new Promise<OpSummary[]>((resolve) => replies.push(resolve))
      : original(cmd, args));
    return { ...mountedQuestion, replies };
  }

  it.each([{ late: [] }, { late: [op(1, "wget", "Running")] }])("coalesces repeated numbers and retires Keep Waiting before late replies $late", async ({ late }) => {
    const { rust, replies } = await heldRequests();
    act(() => {
      rust.hear(QUIT_REQUESTED_EVENT, 1);
      rust.hear(QUIT_REQUESTED_EVENT, 1);
    });
    await act(async () => replies[0](operations));
    const dialog = await screen.findByRole("alertdialog");
    await userEvent.setup().click(within(dialog).getByRole("button", { name: "Keep Waiting" }));
    await act(async () => {
      replies.slice(1).forEach((reply) => reply(late));
      rust.hear(QUIT_REQUESTED_EVENT, 1); // Even a delayed duplicate is retired.
    });
    expect(replies).toHaveLength(1);
    expect(sent("quit_anyway")).toBe(0);
    expect(screen.queryByRole("alertdialog")).toBeNull();
  });

  it("ignores an older reply after a newer question is on screen", async () => {
    const { rust, replies } = await heldRequests();
    act(() => {
      rust.hear(QUIT_REQUESTED_EVENT, 1);
      rust.hear(QUIT_REQUESTED_EVENT, 2);
    });
    await act(async () => replies[1](operations));
    await screen.findByRole("alertdialog");
    await act(async () => replies[0]([]));
    expect(sent("quit_anyway")).toBe(0);
    expect(screen.getByRole("alertdialog")).toBeInTheDocument();
    await userEvent.setup().click(screen.getByRole("button", { name: "Keep Waiting" }));
    expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "quit_kept_waiting")).toEqual([["quit_kept_waiting", { question: 2 }]]);
  });

  it("ignores a pending reply after unmount", async () => {
    const { rust, replies, unmount } = await heldRequests();
    act(() => rust.hear(QUIT_REQUESTED_EVENT, 1));
    unmount();
    await act(async () => replies[0]([]));
    expect(sent("quit_anyway")).toBe(0);
  });
});
