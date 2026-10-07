import { describe, expect, it, beforeEach, vi } from "vitest";
import { act, fireEvent, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { LogDrawer } from "./LogDrawer";
import { useUiStore } from "../store/ui";
import { queryKeys } from "../lib/queryKeys";
import i18n from "../i18n";
import type { OpSummary, Outcome } from "../lib/types";
import { BUTTON } from "./ui/controls";
import { failureCause } from "../lib/failureCause";

/// The drawer as it actually appears: something opened it, and there is
/// page behind it. Both matter for the keyboard, which is why the focus
/// tests render this rather than the drawer on its own.
function DrawerInPage() {
  const setDrawerOpen = useUiStore((s) => s.setDrawerOpen);
  return (
    <>
      <button type="button" onClick={() => setDrawerOpen(true)}>
        show the log
      </button>
      <LogDrawer />
      <button type="button">behind the drawer</button>
    </>
  );
}

const mockInvoke = vi.mocked(invoke);

const runningOp: OpSummary = {
  id: 1,
  kind: "Install",
  instance_id: "brew:/opt/homebrew",
  artifact_kind: "Formula",
  name: "jq",
  status: "Running",
  outcome: null,
  argv_preview: ["/opt/homebrew/bin/brew", "install", "--formula", "jq"],
  cancel_policy: "KillThenReconcile",
};

// What `list_operations` answers. `get_snapshot` is never answered: the
// drawer reads it only for the source's avatar and the row's name, and
// falls back to the operation's own without it.
let operations: OpSummary[];

beforeEach(() => {
  operations = [runningOp];
  mockInvoke.mockReset();
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "list_operations") return Promise.resolve(operations);
    if (cmd === "get_snapshot") return new Promise(() => {});
    return Promise.resolve(undefined);
  });
  useUiStore.setState({ logs: [], drawerOpen: true, focusedOpId: 1, logRun: [] });
});

describe("LogDrawer", () => {
  it("steps through each log of a run the operation bar opened, with Previous and Next, and keeps the focus on one", async () => {
    // Walk-2 W2-4: six updates failed, and the bar's View Log opened the
    // last one's alone.
    const failed = (id: number, name: string): OpSummary => ({
      ...runningOp,
      id,
      kind: "Upgrade",
      name,
      status: "Done",
      outcome: { Failed: { exit_code: 1, summary: "Error: something went wrong", cause: failureCause("Error: something went wrong") } },
    });
    operations = [failed(3, "wget"), failed(2, "jq"), failed(1, "git")];
    act(() => useUiStore.getState().openLogRun([1, 2, 3], 1));
    const { findByRole, getByRole, queryByText } = renderWithProviders(<LogDrawer />);

    const dialog = await findByRole("dialog", { name: "git" });
    // Where it is and which log, in one announcement: the dialog's title
    // changes with no announcement of its own (walk-2 review 2.1).
    const status = within(dialog).getByText("1 of 3");
    expect(status).toHaveAttribute("role", "status");
    expect(status).toHaveTextContent("1 of 3, git: Couldn't update");
    expect(status.querySelector(".sr-only")).toHaveTextContent(", git: Couldn't update");
    const previous = getByRole("button", { name: "Previous Log" });
    const next = getByRole("button", { name: "Next Log" });
    expect(previous).toHaveTextContent("Previous");
    expect(previous).toBeDisabled();
    expect(previous.className).toBe(BUTTON.small.grey);

    act(() => next.focus());
    fireEvent.click(next);
    expect(await findByRole("dialog", { name: "jq" })).toBeInTheDocument();
    expect(within(await findByRole("dialog")).getByText("2 of 3")).toHaveTextContent("2 of 3, jq: Couldn't update");
    expect(getByRole("button", { name: "Previous Log" })).toBeEnabled();
    fireEvent.click(getByRole("button", { name: "Next Log" }));
    expect(await findByRole("dialog", { name: "wget" })).toBeInTheDocument();
    expect(queryByText("3 of 3")).toBeInTheDocument();
    // At the end, Next is off, and the focus is on Previous, not lost.
    expect(getByRole("button", { name: "Next Log" })).toBeDisabled();
    await waitFor(() => expect(document.activeElement).toBe(getByRole("button", { name: "Previous Log" })));

    // Opened on one operation, it has nothing to step through.
    act(() => useUiStore.getState().setFocusedOpId(2));
    expect(await findByRole("dialog", { name: "jq" })).toBeInTheDocument();
    expect(queryByText(/ of 3$/)).toBeNull();
    expect(document.querySelector("[data-log-run]")).toBeNull();
  });

  it("starts each log of a run at its end, however far up the last one was scrolled", async () => {
    // Walk-2 review 2.2: the tool's error is at the end of its log.
    const failed = (id: number, name: string): OpSummary => ({
      ...runningOp,
      id,
      kind: "Upgrade",
      name,
      status: "Done",
      outcome: { Failed: { exit_code: 1, summary: "Error: something went wrong", cause: failureCause("Error: something went wrong") } },
    });
    operations = [failed(2, "jq"), failed(1, "git")];
    act(() => useUiStore.getState().openLogRun([1, 2], 1));
    const { findByRole, getByRole } = renderWithProviders(<LogDrawer />);
    await findByRole("dialog", { name: "git" });
    const viewport = document.querySelector<HTMLElement>("[data-radix-scroll-area-viewport]");
    if (viewport === null) throw new Error("no viewport");
    let top = 0;
    Object.defineProperty(viewport, "scrollHeight", { configurable: true, get: () => 900 });
    Object.defineProperty(viewport, "clientHeight", { configurable: true, get: () => 300 });
    Object.defineProperty(viewport, "scrollTop", {
      configurable: true,
      get: () => top,
      set: (value: number) => {
        top = value;
      },
    });
    // Scrolled up, away from the end.
    top = 0;
    fireEvent.scroll(viewport);
    fireEvent.click(getByRole("button", { name: "Next Log" }));
    await findByRole("dialog", { name: "jq" });
    await waitFor(() => expect(top).toBe(900));
  });

  it("says where it is in the run in Chinese", async () => {
    operations = [
      { ...runningOp, id: 2, status: "Done", outcome: "Unconfirmed" },
      { ...runningOp, status: "Done", outcome: "Unconfirmed" },
    ];
    act(() => useUiStore.getState().openLogRun([1, 2], 2));
    await i18n.changeLanguage("zh-CN");
    try {
      const { findByText, getByRole } = renderWithProviders(<LogDrawer />);
      const status = await findByText("第2个，共2个");
      await waitFor(() => expect(status).toHaveTextContent("第2个，共2个，jq：安装 · 结果未确认"));
      expect(getByRole("button", { name: "上一个日志" })).toBeEnabled();
      expect(getByRole("button", { name: "下一个日志" })).toBeDisabled();
    } finally {
      await i18n.changeLanguage("en");
    }
  });

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
    // The tool's own words in a typewriter face, the way it printed them.
    expect(getByRole("log").className).toContain("font-mono");
  });

  it("renders Banager's own notes in the user's language, in place among the tool's lines", async () => {
    // A note is Banager speaking, so it goes through the locale files: a
    // Chinese user must not meet English in the one place the app has no
    // excuse for it. The tool's own line beside it stays verbatim.
    await i18n.changeLanguage("zh-CN");
    try {
      const { findByText, getByRole } = renderWithProviders(<LogDrawer />);

      act(() => {
        useUiStore.getState().appendLog({ opId: 1, note: { WaitingForBrewUpdate: { minutes: 10 } } });
        useUiStore.getState().appendLog({ opId: 1, stream: "Stdout", line: "==> Pouring jq" });
        useUiStore.getState().appendLog({
          opId: 1,
          note: { ReadFailed: { stream: "Stderr", error: "Input/output error (os error 5)" } },
        });
      });

      await findByText("==> Pouring jq");
      const lines = Array.from(getByRole("log").querySelectorAll("p"));
      expect(lines.map((p) => p.textContent)).toEqual([
        "Homebrew正在联网查找新版本，完成后开始，最多等待10分钟。现在取消不会有任何改动。",
        "==> Pouring jq",
        "无法读取后续错误信息：Input/output error (os error 5)",
      ]);
      // A sentence in the window's own type, not the tool's.
      expect(lines[0].className).toContain("font-sans");
      expect(lines[1].className).not.toContain("font-sans");
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("says where an update's brew cleanup starts, and how it ended when it did not finish or kept versions (U9)", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      const { findByText, getByRole } = renderWithProviders(<LogDrawer />);

      act(() => {
        useUiStore.getState().appendLog({ opId: 1, stream: "Stdout", line: "==> Upgrading wget" });
        useUiStore.getState().appendLog({ opId: 1, note: { CleaningUpOldVersions: { name: "wget" } } });
        useUiStore.getState().appendLog({ opId: 1, stream: "Stderr", line: "Error: Permission denied" });
        useUiStore
          .getState()
          .appendLog({ opId: 1, note: { OldVersionsNotCleanedUp: { name: "wget", exit_code: 1 } } });
        // No exit code: cancelled, out of time, or never started -- it
        // did not finish, whichever.
        useUiStore
          .getState()
          .appendLog({ opId: 1, note: { OldVersionsNotCleanedUp: { name: "wget", exit_code: null } } });
        // It exited 0 and Homebrew kept versions the preview named.
        useUiStore
          .getState()
          .appendLog({ opId: 1, note: { OldVersionsKept: { name: "wget", versions: ["1.24.0", "1.25.0"] } } });
      });

      await findByText("Error: Permission denied");
      const lines = Array.from(getByRole("log").querySelectorAll("p")).map((p) => p.textContent);
      expect(lines).toEqual([
        "==> Upgrading wget",
        "更新已完成，接着运行brew cleanup删除wget的旧版本。",
        "Error: Permission denied",
        "brew cleanup没有完成。更新本身已经完成，没删掉的旧版本仍列在“其他版本”中。",
        "brew cleanup没有完成。更新本身已经完成，没删掉的旧版本仍列在“其他版本”中。",
        "brew cleanup保留了wget的旧版本1.24.0、1.25.0，仍列在“其他版本”中。",
      ]);
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("says an update's brew cleanup did not run when the settings no longer allow it, in each language (F4)", async () => {
    const expected: [string, string][] = [
      [
        "en",
        "brew cleanup didn't run: Homebrew's settings changed after the confirmation opened, or couldn't be read. The update itself is done; any old versions left are still listed under Other versions.",
      ],
      [
        "zh-CN",
        "没有运行brew cleanup：确认窗口打开后，Homebrew的设置有了变化，或无法读取。更新本身已经完成，留下的旧版本仍列在“其他版本”中。",
      ],
      [
        "zh-Hant",
        "沒有執行brew cleanup：確認視窗開啟後，Homebrew的設定有了變化，或無法讀取。更新本身已經完成，留下的舊版本仍列在「其他版本」中。",
      ],
    ];
    try {
      for (const [language, line] of expected) {
        await i18n.changeLanguage(language);
        act(() => useUiStore.setState({ logs: [] }));
        const { findByText, getByRole, unmount } = renderWithProviders(<LogDrawer />);
        act(() => {
          useUiStore.getState().appendLog({ opId: 1, stream: "Stdout", line: "==> Upgrading wget" });
          useUiStore.getState().appendLog({ opId: 1, note: { OldVersionsCleanupSkipped: { name: "wget" } } });
        });
        await findByText("==> Upgrading wget");
        const lines = Array.from(getByRole("log").querySelectorAll("p")).map((p) => p.textContent);
        expect(lines).toEqual(["==> Upgrading wget", line]);
        unmount();
      }
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("says what became of a keg-only formula's link after its update, in each language (y1-keg)", async () => {
    const notes = [
      { RelinkingAfterUpdate: { name: "node@22" } },
      { StillLinkedAfterUpdate: { name: "node@22" } },
      { NoLongerLinked: { name: "node@22", commands: ["node", "npm"] } },
    ] as const;
    const expected: [string, string[]][] = [
      [
        "en",
        [
          "The update is done. Running brew link --formula --force node@22 to link it back into Terminal.",
          "Homebrew linked node@22 back into Terminal itself, so brew link wasn't needed.",
          "node@22 isn't linked back into Terminal, so typing node, npm no longer runs it. To link it back, run brew link --force node@22 in Terminal; if a file is in the way, it says which.",
        ],
      ],
      [
        "zh-CN",
        [
          "更新已完成，接着运行brew link --formula --force node@22，把它重新接到终端里。",
          "Homebrew已把node@22重新接到终端里，不需要运行brew link。",
          "node@22没有重新接到终端里，输入node、npm不再运行它。要接回去，可以在终端里运行brew link --force node@22；如果有文件挡住，它会说出是哪个。",
        ],
      ],
      [
        "zh-Hant",
        [
          "更新已完成，接著執行brew link --formula --force node@22，把它重新接到終端機裡。",
          "Homebrew已把node@22重新接到終端機裡，不需要執行brew link。",
          "node@22沒有重新接到終端機裡，輸入node、npm不再執行它。要接回去，可以在終端機裡執行brew link --force node@22；如果有檔案擋住，它會說出是哪一個。",
        ],
      ],
    ];
    try {
      for (const [language, lines] of expected) {
        await i18n.changeLanguage(language);
        act(() => useUiStore.setState({ logs: [] }));
        const { findByText, getByRole, unmount } = renderWithProviders(<LogDrawer />);
        act(() => {
          useUiStore.getState().appendLog({ opId: 1, stream: "Stdout", line: "==> Upgrading node@22" });
          for (const note of notes) useUiStore.getState().appendLog({ opId: 1, note });
        });
        await findByText("==> Upgrading node@22");
        const shown = Array.from(getByRole("log").querySelectorAll("p")).map((p) => p.textContent);
        expect(shown).toEqual(["==> Upgrading node@22", ...lines]);
        unmount();
      }
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("says which stream a failed read cut short", async () => {
    const { findByText } = renderWithProviders(<LogDrawer />);

    act(() => {
      useUiStore.getState().appendLog({
        opId: 1,
        note: { ReadFailed: { stream: "Stdout", error: "EIO" } },
      });
    });

    await findByText("Couldn't read any more output: EIO");
  });

  it("lets the log be selected, to be copied, and nothing else in the drawer", async () => {
    const { findByText, getByRole } = renderWithProviders(<LogDrawer />);

    act(() => {
      useUiStore.getState().appendLog({ opId: 1, stream: "Stderr", line: "Error: No such keg: /opt/homebrew/Cellar/jq" });
      useUiStore.getState().appendLog({ opId: 1, note: { WaitingForBrewUpdate: { minutes: 10 } } });
    });

    await findByText("Error: No such keg: /opt/homebrew/Cellar/jq");
    // The whole log, Banager's notes among the tool's lines.
    const log = getByRole("log");
    expect(log).toHaveClass("select-text");
    // Not its title, where it stands, nor Cancel.
    expect([...getByRole("dialog").querySelectorAll(".select-text")]).toEqual([log]);
  });

  it("words each path a path-list uninstall moved, and the one macOS refused", async () => {
    // Banager's own two lines in an uninstall that runs no command: where
    // each path went, and the system's words for one it would not move.
    const { findByText } = renderWithProviders(<LogDrawer />);

    act(() => {
      useUiStore.getState().appendLog({
        opId: 1,
        note: { MovedToTrash: { path: "~/.local/share/claude", trashed_to: "~/.Trash/claude" } },
      });
      useUiStore.getState().appendLog({
        opId: 1,
        note: { TrashFailed: { path: "~/.local/bin/claude", error: "Operation not permitted" } },
      });
    });

    await findByText("Moved ~/.local/share/claude to the Trash: ~/.Trash/claude");
    await findByText(
      "Couldn't move ~/.local/bin/claude to the Trash, so the uninstall stopped: Operation not permitted",
    );
  });

  it("says which item a path-list uninstall stopped before when its time ran out", async () => {
    // The stop nobody asked for: without this line the log would end at
    // the last move and the outcome would say only "Result unconfirmed".
    // The number of seconds is the plan's, carried in the note.
    const { findByText } = renderWithProviders(<LogDrawer />);

    act(() => {
      useUiStore.getState().appendLog({
        opId: 1,
        note: { OutOfTime: { path: "~/.local/bin/claude", seconds: 120 } },
      });
    });

    await findByText(
      "Time ran out after 120 seconds, so the uninstall stopped before moving ~/.local/bin/claude. What was moved is in the Trash; uninstall again to move the rest.",
    );
  });

  it("names what it found after a path-list uninstall moved everything, under the outcome and what to do", async () => {
    // The run's own last look, once the pause after its last move is
    // over: one line per path it found there, and the outcome pointing at
    // those lines -- with the launcher gone, nothing else on screen shows
    // them. What to do is the drawer's to say, under the outcome.
    operations = [{ ...runningOp, kind: "Uninstall", status: "Done", outcome: { NeedsAttention: "BackAfterUninstall" } }];
    const { findByText, getByRole } = renderWithProviders(<LogDrawer />);

    act(() => {
      useUiStore.getState().appendLog({
        opId: 1,
        note: { BackAfterUninstall: { path: "~/.local/share/claude" } },
      });
    });

    await findByText("Found ~/.local/share/claude at the final check and left it where it is.");
    await findByText("Files showed up again after the uninstall");
    expect(getByRole("img", { name: "Needs attention" })).toBeInTheDocument();
    await findByText(
      "Quit the tool first. If it's still listed, uninstall it again; otherwise move the files named in the log to the Trash yourself.",
    );
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

  it("titles itself with the tool it acts on, and says what it does under it: Installing… (walk-3 W3-3)", async () => {
    const { findByRole, getByText } = renderWithProviders(<LogDrawer />);

    // Not "Install jq", which in English reads as a command.
    const drawer = await findByRole("dialog", { name: "jq" });
    expect(getByText("Installing…")).toBeInTheDocument();
    // The log itself keeps the drawer's old name.
    expect(within(drawer).getByRole("log", { name: "Operation log" })).toBeInTheDocument();
  });

  it("closes with Close while the operation runs, and with Done once it has ended (walk-3 W3-4)", async () => {
    const running = renderWithProviders(<LogDrawer />);
    const dialog = await running.findByRole("dialog", { name: "jq" });
    // Beside Cancel Install, a Done would read as the install being done.
    expect(within(dialog).getByRole("button", { name: "Cancel Install" })).toBeInTheDocument();
    expect(within(dialog).queryByRole("button", { name: "Done" })).toBeNull();
    fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
    expect(useUiStore.getState().drawerOpen).toBe(false);
    running.unmount();

    act(() => useUiStore.getState().setDrawerOpen(true));
    operations = [{ ...runningOp, status: "Done", outcome: "Succeeded" }];
    const done = renderWithProviders(<LogDrawer />);
    const ended = await done.findByRole("dialog", { name: "jq" });
    await done.findByText("Installed");
    expect(within(ended).queryByRole("button", { name: "Close" })).toBeNull();
    expect(within(ended).getByRole("button", { name: "Done" })).toBeInTheDocument();
    done.unmount();

    // In Chinese: 关闭 while it runs, never 完成.
    act(() => useUiStore.getState().setDrawerOpen(true));
    operations = [runningOp];
    await i18n.changeLanguage("zh-CN");
    try {
      const zh = renderWithProviders(<LogDrawer />);
      const sheet = await zh.findByRole("dialog", { name: "jq" });
      expect(within(sheet).getByRole("button", { name: "关闭" })).toBeInTheDocument();
      expect(within(sheet).queryByRole("button", { name: "完成" })).toBeNull();
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("says what the operation does under its name wherever the words don't, in both languages (walk-3 review 1.1)", async () => {
    const network: Outcome = {
      Failed: { exit_code: 1, summary: 'Error: jq: Failed to download resource "jq (1.8.1)"', cause: failureCause('Error: jq: Failed to download resource "jq (1.8.1)"') },
    };
    const cases: [OpSummary, string, string][] = [
      [{ ...runningOp, status: "Queued" }, "Install · Queued", "安装 · 排队中"],
      [{ ...runningOp, kind: "Uninstall", status: "Done", outcome: "Cancelled" }, "Uninstall · Cancelled", "卸载 · 已取消"],
      [{ ...runningOp, kind: "Upgrade", status: "Done", outcome: network }, "Update · Connection failed", "更新 · 网络连接失败"],
      [{ ...runningOp, status: "Done", outcome: "Unconfirmed" }, "Install · Result unconfirmed", "安装 · 结果未确认"],
      // Words that say it already stand alone.
      [{ ...runningOp, kind: "Uninstall" }, "Uninstalling…", "正在卸载…"],
    ];
    for (const [operation, english, chinese] of cases) {
      operations = [operation];
      const en = renderWithProviders(<LogDrawer />);
      const dialog = await en.findByRole("dialog", { name: "jq" });
      expect(await within(dialog).findByText(english)).toBeInTheDocument();
      en.unmount();
      act(() => useUiStore.getState().setDrawerOpen(true));
      await i18n.changeLanguage("zh-CN");
      try {
        const zh = renderWithProviders(<LogDrawer />);
        const sheet = await zh.findByRole("dialog", { name: "jq" });
        expect(await within(sheet).findByText(chinese)).toBeInTheDocument();
        zh.unmount();
      } finally {
        await i18n.changeLanguage("en");
      }
      act(() => useUiStore.getState().setDrawerOpen(true));
    }
  });

  it("turns its Close into Done in place, in the sheet already open, as the operation ends (walk-3 review 5.2)", async () => {
    const { findByRole, queryClient } = renderWithProviders(<LogDrawer />);
    const dialog = await findByRole("dialog", { name: "jq" });
    const button = within(dialog).getByRole("button", { name: "Close" });

    operations = [{ ...runningOp, status: "Done", outcome: "Succeeded" }];
    await act(() => queryClient.invalidateQueries({ queryKey: queryKeys.operations }));

    // The same open sheet and the same button: only its word changed.
    await waitFor(() => expect(button).toHaveTextContent(/^Done$/));
    expect(within(dialog).queryByRole("button", { name: "Close" })).toBeNull();
    expect(within(dialog).getByText("Installed")).toBeInTheDocument();
    expect(useUiStore.getState().drawerOpen).toBe(true);
  });

  it("is simply the operation log until the list of operations has it", async () => {
    operations = [];
    const { findByRole } = renderWithProviders(<LogDrawer />);

    expect(await findByRole("dialog", { name: "Operation log" })).toBeInTheDocument();
  });

  it("shows the terminal outcome in place of where it stood once the operation finishes", async () => {
    operations = [{ ...runningOp, status: "Done", outcome: "Succeeded" }];

    const { findByText, queryByText } = renderWithProviders(<LogDrawer />);

    await findByText("Installed");
    expect(queryByText("Installing…")).toBeNull();
  });

  it("offers Cancel while the operation runs, named for what it stops, and it reaches cancel_operation", async () => {
    // The page under the drawer, the operation bar's Cancel with it, is
    // out of reach while it is open.
    const { findByRole } = renderWithProviders(<LogDrawer />);

    fireEvent.click(await findByRole("button", { name: "Cancel Install" }));

    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("cancel_operation", { opId: 1 }));
  });

  it("offers no Cancel once the operation is done, nor for a running one that cannot be stopped", async () => {
    operations = [{ ...runningOp, status: "Done", outcome: "Succeeded" }];
    const done = renderWithProviders(<LogDrawer />);
    await done.findByText("Installed");
    expect(done.queryByRole("button", { name: "Cancel Install" })).toBeNull();
    done.unmount();

    operations = [{ ...runningOp, kind: "Upgrade", name: "rustup", cancel_policy: "NoCancel" }];
    const noCancel = renderWithProviders(<LogDrawer />);
    await noCancel.findByRole("dialog", { name: "rustup" });
    expect(noCancel.queryByRole("button", { name: "Cancel Update" })).toBeNull();
  });

  it("says what to do next under an outcome that leaves the user a step", async () => {
    // A crash may have run the command (T9): the next step is to look,
    // never "nothing changed".
    operations = [{ ...runningOp, status: "Done", outcome: { BanagerFailed: "Panicked" } }];

    const { findByText } = renderWithProviders(<LogDrawer />);

    await findByText("Install · Couldn't finish because of an internal error");
    await findByText("Check the list to see whether anything changed.");
  });

  it("words Banager's own failure in the user's language, quoting only the path", async () => {
    // This used to arrive as `Failed` with Rust's English in its summary
    // ("runner: program not found: /opt/homebrew/bin/brew"), printed inside
    // the translated "失败：" frame (now 「未能开始：」).
    operations = [
      {
        ...runningOp,
        status: "Done",
        outcome: { BanagerFailed: { ProgramMissing: { program: "/opt/homebrew/bin/brew" } } },
      },
    ];
    await i18n.changeLanguage("zh-CN");
    try {
      const { findByText, queryByText, findByRole } = renderWithProviders(<LogDrawer />);
      await findByRole("dialog", { name: "jq" });
      await findByText("安装 · 未能开始：找不到/opt/homebrew/bin/brew，没有改动");
      expect(queryByText(/program not found/)).not.toBeInTheDocument();
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("is a dialog 560 wide, the log in a grouped container in 11 monospace, stderr in the red for text", async () => {
    const { findByText, getByRole } = renderWithProviders(<LogDrawer />);

    act(() => {
      useUiStore.getState().appendLog({ opId: 1, stream: "Stdout", line: "==> Fetching jq" });
      useUiStore.getState().appendLog({ opId: 1, stream: "Stderr", line: "curl: (6) Could not resolve host: ghcr.io" });
    });

    const stderr = await findByText("curl: (6) Could not resolve host: ghcr.io");
    const dialog = getByRole("dialog");
    expect(dialog).toHaveAttribute("data-dialog-width", "560");
    const log = getByRole("log");
    expect(log).toHaveClass("font-mono", "text-small");
    expect(log.closest(".bg-group")).toHaveClass("rounded-group");
    expect(stderr).toHaveClass("text-danger-text");
    expect(getByRole("log").querySelector("p")).not.toHaveClass("text-danger-text");
  });

  it("has Copy Log on the left of its foot and Close, the default button while it runs, on the right", async () => {
    const writeText = vi.fn(() => Promise.resolve());
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    try {
      const { findByText, findByRole, getByRole } = renderWithProviders(<LogDrawer />);
      act(() => {
        useUiStore.getState().appendLog({ opId: 1, stream: "Stdout", line: "Fetching jq" });
        useUiStore.getState().appendLog({ opId: 1, note: { WaitingForBrewUpdate: { minutes: 10 } } });
      });
      await findByText("Fetching jq");
      await findByRole("button", { name: "Cancel Install" });

      const footer = getByRole("dialog").querySelector("[data-dialog-footer]") as HTMLElement;
      const buttons = within(footer).getAllByRole("button");
      // Copy Log, then Cancel Install while it can still be stopped, then Close.
      expect(buttons.map((button) => button.textContent)).toEqual(["Copy Log", "Cancel Install", "Close"]);
      expect(buttons[0].closest(".mr-auto")).not.toBeNull();
      expect(buttons[0].className).toBe(BUTTON.large.grey);
      expect(buttons[2].className).toBe(BUTTON.large.default);

      fireEvent.click(buttons[0]);
      // The whole log, Banager's notes in the user's words.
      await waitFor(() =>
        expect(writeText).toHaveBeenCalledWith(
          "Fetching jq\nHomebrew is checking online for new versions; this starts when it's done, waiting up to 10 minutes. Cancelling now changes nothing.",
        ),
      );
      await findByText("Copied");

      fireEvent.click(buttons[2]);
      expect(useUiStore.getState().drawerOpen).toBe(false);
    } finally {
      Object.defineProperty(navigator, "clipboard", { value: undefined, configurable: true });
    }
  });

  it("says how a failure ended in the operation bar's words, its cause where the tool's words give one, and the next step under it", async () => {
    operations = [
      {
        ...runningOp,
        kind: "Upgrade",
        status: "Done",
        outcome: { Failed: { exit_code: 1, summary: 'Error: jq: Failed to download resource "jq (1.8.1)"', cause: failureCause('Error: jq: Failed to download resource "jq (1.8.1)"') } },
      },
    ];
    const { findByText, getByRole } = renderWithProviders(<LogDrawer />);
    act(() => {
      useUiStore.getState().appendLog({ opId: 1, stream: "Stderr", line: 'Error: jq: Failed to download resource "jq (1.8.1)"' });
    });

    // The cause, as the row and the bar say it, then what to do.
    // What it does in front: the words don't say it (walk-3 review 1.1).
    const cause = await findByText("Update · Connection failed");
    await findByText("Check your internet connection, then try again.");
    // The tool's own words only in the log, below: not in the header,
    // where Show technical details is off.
    const dialog = getByRole("dialog");
    const log = getByRole("log");
    for (const element of dialog.querySelectorAll("*")) {
      if (log.contains(element) || element.contains(log)) continue;
      expect(element.textContent).not.toContain("Failed to download");
    }
    expect(within(log).getByText('Error: jq: Failed to download resource "jq (1.8.1)"')).toBeInTheDocument();
    expect(cause.textContent).not.toContain("Couldn't install");
  });

  it("says 未能更新 of a failure whose cause it cannot tell, as the row does, and 网络连接失败 of one it can, in Chinese", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      operations = [
        {
          ...runningOp,
          kind: "Upgrade",
          name: "git",
          status: "Done",
          outcome: { Failed: { exit_code: 1, summary: 'Error: Failed to download resource "git (2.55.1)"', cause: failureCause('Error: Failed to download resource "git (2.55.1)"') } },
        },
      ];
      const network = renderWithProviders(<LogDrawer />);
      await network.findByRole("dialog", { name: "git" });
      expect(await network.findByText("更新 · 网络连接失败")).toBeInTheDocument();
      expect(network.queryByText(/^未能完成：Error/)).toBeNull();
      expect(network.getByRole("button", { name: "拷贝日志" })).toBeInTheDocument();
      network.unmount();

      operations = [
        {
          ...runningOp,
          kind: "Upgrade",
          name: "git",
          status: "Done",
          outcome: { Failed: { exit_code: 1, summary: "Error: git: something went wrong", cause: failureCause("Error: git: something went wrong") } },
        },
      ];
      const unknown = renderWithProviders(<LogDrawer />);
      expect(await unknown.findByText("未能更新")).toBeInTheDocument();
      expect(unknown.queryByText(/something went wrong/)).toBeNull();
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("gives the tool's own words in the header too with Show technical details on", async () => {
    operations = [
      {
        ...runningOp,
        kind: "Upgrade",
        status: "Done",
        outcome: { Failed: { exit_code: 1, summary: 'Error: jq: Failed to download resource "jq (1.8.1)"', cause: failureCause('Error: jq: Failed to download resource "jq (1.8.1)"') } },
      },
    ];
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "list_operations") return Promise.resolve(operations);
      if (cmd === "get_snapshot") return new Promise(() => {});
      if (cmd === "get_settings") {
        return Promise.resolve({
          language: "en",
          show_technical_details: true,
          ignored_updates: [],
          skipped_versions: [],
          include_self_updating: false,
          auto_check: false,
          notify_updates: false,
        });
      }
      return Promise.resolve(undefined);
    });
    const { findByText } = renderWithProviders(<LogDrawer />);

    await findByText('Update · Couldn\'t finish: Error: jq: Failed to download resource "jq (1.8.1)"');
    await findByText("Check your internet connection, then try again.");
  });

  it("does not render when the drawer is closed", () => {
    useUiStore.setState({ drawerOpen: false });
    const { queryByRole } = renderWithProviders(<LogDrawer />);

    expect(queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("takes focus when it opens and gives it back to whatever opened it", async () => {
    // Closing a panel should leave a keyboard user where they were, not
    // on the document body at the top of the window.
    useUiStore.setState({ drawerOpen: false });
    const user = userEvent.setup();
    const { getByRole } = renderWithProviders(<DrawerInPage />);
    const opener = getByRole("button", { name: "show the log" });

    await user.click(opener);
    // The panel itself, not its close button, one Enter from shutting it.
    expect(getByRole("dialog")).toHaveFocus();

    await user.keyboard("{Escape}");
    expect(useUiStore.getState().drawerOpen).toBe(false);
    await waitFor(() => expect(opener).toHaveFocus());
  });

  it("closes on Escape from a control inside it, not just from the panel", async () => {
    const user = userEvent.setup();
    const { getByRole } = renderWithProviders(<DrawerInPage />);

    await user.tab();
    expect(getByRole("dialog").contains(document.activeElement)).toBe(true);

    await user.keyboard("{Escape}");
    expect(useUiStore.getState().drawerOpen).toBe(false);
  });

  it("keeps Tab inside the drawer instead of letting it wander behind", async () => {
    // The drawer sits over the page. Tabbing out of it puts the cursor on
    // controls the user can neither see nor tell they are on. (The page
    // behind is hidden from assistive technology while the drawer is
    // open, which is why its buttons are found with `hidden: true`.)
    const user = userEvent.setup();
    const { getByRole } = renderWithProviders(<DrawerInPage />);
    const dialog = getByRole("dialog");
    const behind = getByRole("button", { name: "behind the drawer", hidden: true });
    const opener = getByRole("button", { name: "show the log", hidden: true });

    for (let i = 0; i < 6; i += 1) {
      await user.tab();
      expect(document.activeElement).not.toBe(behind);
      expect(document.activeElement).not.toBe(opener);
      expect(dialog.contains(document.activeElement)).toBe(true);
    }

    // And backwards, which is the direction that used to walk straight
    // out of the front of the drawer.
    for (let i = 0; i < 6; i += 1) {
      await user.tab({ shift: true });
      expect(document.activeElement).not.toBe(behind);
      expect(document.activeElement).not.toBe(opener);
      expect(dialog.contains(document.activeElement)).toBe(true);
    }
  });
});

describe("LogDrawer, under a tool's own words", () => {
  const failedUpdate = (summary: string, extra: Partial<OpSummary> = {}): OpSummary => ({
    ...runningOp,
    kind: "Upgrade",
    name: "wget",
    status: "Done",
    outcome: { Failed: { exit_code: 1, summary, cause: failureCause(summary) } },
    ...extra,
  });
  const said = (line: string) =>
    act(() => {
      useUiStore.getState().appendLog({ opId: 1, stream: "Stderr", line });
    });

  it("says whose words they are and what to do next, under the log and above Copy Log", async () => {
    operations = [failedUpdate("Error: wget: something went wrong")];
    const { findByText, getByRole } = renderWithProviders(<LogDrawer />);
    said("Error: wget: something went wrong");

    const step = await findByText(
      "The lines above are the error message from Homebrew itself. You can click Retry later. If it still fails, click Copy Log and send the log to someone who can help.",
    );
    // After the log, in the dialog's body; Copy Log is in the foot below it.
    const log = getByRole("log");
    expect(log.compareDocumentPosition(step) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(step).toHaveClass("text-body", "text-foreground");
    // With no step over the log, it is what the dialog says it is about.
    expect(getByRole("dialog").getAttribute("aria-describedby")?.split(" ")).toContain(step.id);
  });

  it("says it in both languages, and with a cause, leaves its step to the sentence over the log", async () => {
    const generic = "Error: wget: something went wrong";
    const afterStep = [
      "curl: (6) Could not resolve host: ghcr.io",
      "Error: No space left on device @ rb_sysopen",
      "Error: Permission denied @ dir_s_mkdir - /opt/homebrew/Cellar",
      "Error: Another active Homebrew process is already in progress.",
      "sudo: 3 incorrect password attempts",
    ];
    const needsPassword =
      "sudo: a terminal is required to read the password; either use the -S option to read from standard input or configure an askpass helper";
    const sentences = {
      en: {
        generic: "The lines above are the error message from Homebrew itself. You can click Retry later. If it still fails, click Copy Log and send the log to someone who can help.",
        install: "The lines above are the error message from Homebrew itself. You can install it again later. If it still fails, click Copy Log and send the log to someone who can help.",
        afterStep: "The lines above are the error message from Homebrew itself. If it still fails after the step above, click Copy Log and send the log to someone who can help.",
        inTerminal: "The lines above are the error message from Homebrew itself. If the command above still fails in Terminal, click Copy Log and send the log to someone who can help.",
        copyOnly: "The lines above are the error message from npm itself. You can click Copy Log and send the log to someone who can help.",
      },
      "zh-CN": {
        generic: "上面是Homebrew自己的报错。可以稍后点按“重试”；还是失败，就点按“拷贝日志”，发给懂的人看。",
        install: "上面是Homebrew自己的报错。可以稍后重新安装；还是失败，就点按“拷贝日志”，发给懂的人看。",
        afterStep: "上面是Homebrew自己的报错。照上面说的做了还是失败，就点按“拷贝日志”，发给懂的人看。",
        inTerminal: "上面是Homebrew自己的报错。在终端里运行上面那条命令还是失败，就点按“拷贝日志”，发给懂的人看。",
        copyOnly: "上面是npm自己的报错。可以点按“拷贝日志”，发给懂的人看。",
      },
    };
    const npmUpdate = {
      instance_id: "npm:/opt/homebrew/bin/npm",
      argv_preview: ["/opt/homebrew/bin/npm", "install", "-g", "prettier@latest"],
    };
    try {
      for (const lang of ["en", "zh-CN"] as const) {
        await i18n.changeLanguage(lang);
        const want = sentences[lang];
        const cases: Array<[OpSummary, string, string]> = [
          [failedUpdate(generic), generic, want.generic],
          // An install is done again; it has no Retry.
          [{ ...failedUpdate(generic), kind: "Install" }, generic, want.install],
          ...afterStep.map((line): [OpSummary, string, string] => [failedUpdate(line), line, want.afterStep]),
          // No Retry (the row has none): the Terminal command over the log.
          [failedUpdate(needsPassword), needsPassword, want.inTerminal],
          // The same from a source with no command to hand over.
          [failedUpdate(needsPassword, npmUpdate), needsPassword, want.copyOnly],
        ];
        for (const [failed, line, sentence] of cases) {
          useUiStore.setState({ logs: [] });
          operations = [failed];
          const view = renderWithProviders(<LogDrawer />);
          said(line);
          const step = await view.findByText(sentence);
          expect(step).toHaveAttribute("data-failure-next-step");
          view.unmount();
        }
      }
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("names the source that wrote the lines and how to try again for an uninstall, in both languages", async () => {
    operations = [
      failedUpdate("npm error code EUNEXPECTED", {
        kind: "Uninstall",
        instance_id: "npm:/opt/homebrew/bin/npm",
        argv_preview: ["/opt/homebrew/bin/npm", "uninstall", "-g", "prettier"],
      }),
    ];
    const en = renderWithProviders(<LogDrawer />);
    said("npm error code EUNEXPECTED");
    await en.findByText(
      "The lines above are the error message from npm itself. You can uninstall it again later. If it still fails, click Copy Log and send the log to someone who can help.",
    );
    en.unmount();

    await i18n.changeLanguage("zh-CN");
    try {
      const zh = renderWithProviders(<LogDrawer />);
      await zh.findByText("上面是npm自己的报错。可以稍后重新卸载；还是失败，就点按“拷贝日志”，发给懂的人看。");
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("says nothing of the kind where no tool's words are above it", async () => {
    // Banager's own failure: its words, not a tool's.
    operations = [{ ...runningOp, status: "Done", outcome: { BanagerFailed: "Panicked" } }];
    const own = renderWithProviders(<LogDrawer />);
    await own.findByText("Install · Couldn't finish because of an internal error");
    expect(own.container.ownerDocument.querySelector("[data-failure-next-step]")).toBeNull();
    own.unmount();

    // A failure whose log this window does not have: nothing is above.
    operations = [failedUpdate("Error: wget: something went wrong")];
    const empty = renderWithProviders(<LogDrawer />);
    await empty.findByText("Couldn't update");
    expect(empty.container.ownerDocument.querySelector("[data-failure-next-step]")).toBeNull();
    empty.unmount();

    // A success, with lines on stderr all the same.
    operations = [{ ...runningOp, status: "Done", outcome: "Succeeded" }];
    const fine = renderWithProviders(<LogDrawer />);
    said("Warning: jq 1.8.1 is already installed");
    await fine.findByText("Warning: jq 1.8.1 is already installed");
    expect(fine.container.ownerDocument.querySelector("[data-failure-next-step]")).toBeNull();
  });

  it("leaves the cause's step over the log as what the dialog is about", async () => {
    operations = [failedUpdate("curl: (6) Could not resolve host: ghcr.io")];
    const { findByText, getByRole } = renderWithProviders(<LogDrawer />);
    said("curl: (6) Could not resolve host: ghcr.io");
    const over = await findByText("Check your internet connection, then try again.");
    const under = await findByText(/^The lines above are the error message from Homebrew itself\. If it still fails after the step above/);
    const described = getByRole("dialog").getAttribute("aria-describedby")?.split(" ") ?? [];
    expect(described).toContain(over.id);
    expect(described).not.toContain(under.id);
  });
});

describe("LogDrawer, a tool's own words left only in the subtitle", () => {
  // "Show technical details" on: the subtitle has the tool's words.
  const technicalOn = () =>
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "list_operations") return Promise.resolve(operations);
      if (cmd === "get_snapshot") return new Promise(() => {});
      if (cmd === "get_settings") {
        return Promise.resolve({
          language: "System",
          show_technical_details: true,
          ignored_updates: [],
          skipped_versions: [],
          include_self_updating: false,
          auto_check: false,
          notify_updates: false,
        });
      }
      return Promise.resolve(undefined);
    });
  const failed = (summary: string, exitCode: number | null = 1): OpSummary => ({
    ...runningOp,
    kind: "Upgrade",
    name: "wget",
    status: "Done",
    outcome: { Failed: { exit_code: exitCode, summary, cause: failureCause(summary) } },
  });
  const sentence = {
    en: "The words above are the error message from Homebrew itself. You can click Retry later. If it still fails, show these words to someone who can help.",
    "zh-CN": "上面是Homebrew自己的报错。可以稍后点按“重试”；还是失败，就把这段报错告诉懂的人。",
  };

  it("says whose they are under the subtitle when this window's log has none of its lines, in both languages", async () => {
    technicalOn();
    operations = [failed("Error: wget: something went wrong")];
    try {
      for (const lang of ["en", "zh-CN"] as const) {
        await i18n.changeLanguage(lang);
        const view = renderWithProviders(<LogDrawer />);
        const step = await view.findByText(sentence[lang]);
        expect(step).toHaveAttribute("data-failure-next-step");
        // Over the log, and what the dialog says it is about.
        expect(step.compareDocumentPosition(view.getByRole("log")) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
        expect(view.getByRole("dialog").getAttribute("aria-describedby")?.split(" ")).toContain(step.id);
        view.unmount();
      }
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("says it once only: not over the log when the log has the lines, nor with a cause, nor with details off", async () => {
    technicalOn();
    // The log has the lines: the sentence is under the log, not over it.
    operations = [failed("Error: wget: something went wrong")];
    const withLog = renderWithProviders(<LogDrawer />);
    act(() => {
      useUiStore.getState().appendLog({ opId: 1, stream: "Stderr", line: "Error: wget: something went wrong" });
    });
    await withLog.findByText(/^The lines above are the error message from Homebrew itself\./);
    expect(withLog.queryByText(sentence.en)).toBeNull();
    withLog.unmount();
    useUiStore.setState({ logs: [] });

    // A cause: its step is over the log already.
    operations = [failed("curl: (6) Could not resolve host: ghcr.io")];
    const cause = renderWithProviders(<LogDrawer />);
    await cause.findByText("Check your internet connection, then try again.");
    expect(cause.container.ownerDocument.querySelector("[data-failure-next-step]")).toBeNull();
    cause.unmount();

    // macOS's words for a path it would not move to the Trash: no command
    // ran, so they are not the source's.
    operations = [failed("Operation not permitted", null)];
    const trash = renderWithProviders(<LogDrawer />);
    await trash.findByText(/Operation not permitted/);
    expect(trash.container.ownerDocument.querySelector("[data-failure-next-step]")).toBeNull();
    trash.unmount();
  });

  it("says nothing over the log with technical details off", async () => {
    operations = [failed("Error: wget: something went wrong")];
    const view = renderWithProviders(<LogDrawer />);
    await view.findByText("Couldn't update");
    expect(view.container.ownerDocument.querySelector("[data-failure-next-step]")).toBeNull();
  });
});
