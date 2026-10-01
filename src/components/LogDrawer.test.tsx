import { describe, expect, it, beforeEach, vi } from "vitest";
import { act, fireEvent, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { LogDrawer } from "./LogDrawer";
import { useUiStore } from "../store/ui";
import i18n from "../i18n";
import type { OpSummary } from "../lib/types";
import { BUTTON } from "./ui/controls";

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
        "Homebrew正在更新软件清单，完成后开始，最多等待10分钟。现在取消不会有任何改动。",
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

  it("titles itself with what the operation does and to what, and says where it stands", async () => {
    const { findByRole, getByText } = renderWithProviders(<LogDrawer />);

    const drawer = await findByRole("dialog", { name: "Install jq" });
    expect(getByText("Running")).toBeInTheDocument();
    // The log itself keeps the drawer's old name.
    expect(within(drawer).getByRole("log", { name: "Operation log" })).toBeInTheDocument();
  });

  it("is simply the operation log until the list of operations has it", async () => {
    operations = [];
    const { findByRole } = renderWithProviders(<LogDrawer />);

    expect(await findByRole("dialog", { name: "Operation log" })).toBeInTheDocument();
  });

  it("shows the terminal outcome in place of where it stood once the operation finishes", async () => {
    operations = [{ ...runningOp, status: "Done", outcome: "Succeeded" }];

    const { findByText, queryByText } = renderWithProviders(<LogDrawer />);

    await findByText("Completed");
    expect(queryByText("Running")).toBeNull();
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
    await done.findByText("Completed");
    expect(done.queryByRole("button", { name: "Cancel Install" })).toBeNull();
    done.unmount();

    operations = [{ ...runningOp, kind: "Upgrade", name: "rustup", cancel_policy: "NoCancel" }];
    const noCancel = renderWithProviders(<LogDrawer />);
    await noCancel.findByRole("dialog", { name: "Update rustup" });
    expect(noCancel.queryByRole("button", { name: "Cancel Update" })).toBeNull();
  });

  it("says what to do next under an outcome that leaves the user a step", async () => {
    // A crash may have run the command (T9): the next step is to look,
    // never "nothing changed".
    operations = [{ ...runningOp, status: "Done", outcome: { BanagerFailed: "Panicked" } }];

    const { findByText } = renderWithProviders(<LogDrawer />);

    await findByText("Couldn't finish because of an internal error");
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
      await findByRole("dialog", { name: "安装jq" });
      await findByText("未能开始：找不到/opt/homebrew/bin/brew，没有改动");
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

  it("has Copy Log on the left of its foot and Done, the default button, on the right", async () => {
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
      // Copy Log, then Cancel Install while it can still be stopped, then Done.
      expect(buttons.map((button) => button.textContent)).toEqual(["Copy Log", "Cancel Install", "Done"]);
      expect(buttons[0].closest(".mr-auto")).not.toBeNull();
      expect(buttons[0].className).toBe(BUTTON.large.grey);
      expect(buttons[2].className).toBe(BUTTON.large.default);

      fireEvent.click(buttons[0]);
      // The whole log, Banager's notes in the user's words.
      await waitFor(() =>
        expect(writeText).toHaveBeenCalledWith(
          "Fetching jq\nHomebrew is updating its software list; this starts when it's done, waiting up to 10 minutes. Cancelling now changes nothing.",
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
        outcome: { Failed: { exit_code: 1, summary: 'Error: jq: Failed to download resource "jq (1.8.1)"' } },
      },
    ];
    const { findByText, getByRole } = renderWithProviders(<LogDrawer />);
    act(() => {
      useUiStore.getState().appendLog({ opId: 1, stream: "Stderr", line: 'Error: jq: Failed to download resource "jq (1.8.1)"' });
    });

    // The cause, as the row and the bar say it, then what to do.
    const cause = await findByText("Connection failed");
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
    expect(cause.textContent).not.toContain("Couldn't finish");
  });

  it("says 未能完成 of a failure whose cause it cannot tell, and 网络连接失败 of one it can, in Chinese", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      operations = [
        {
          ...runningOp,
          kind: "Upgrade",
          name: "git",
          status: "Done",
          outcome: { Failed: { exit_code: 1, summary: 'Error: Failed to download resource "git (2.55.1)"' } },
        },
      ];
      const network = renderWithProviders(<LogDrawer />);
      await network.findByRole("dialog", { name: "更新git" });
      expect(await network.findByText("网络连接失败")).toBeInTheDocument();
      expect(network.queryByText(/^未能完成：Error/)).toBeNull();
      expect(network.getByRole("button", { name: "拷贝日志" })).toBeInTheDocument();
      network.unmount();

      operations = [
        {
          ...runningOp,
          kind: "Upgrade",
          name: "git",
          status: "Done",
          outcome: { Failed: { exit_code: 1, summary: "Error: git: something went wrong" } },
        },
      ];
      const unknown = renderWithProviders(<LogDrawer />);
      expect(await unknown.findByText("未能完成")).toBeInTheDocument();
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
        outcome: { Failed: { exit_code: 1, summary: 'Error: jq: Failed to download resource "jq (1.8.1)"' } },
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

    await findByText('Couldn\'t finish: Error: jq: Failed to download resource "jq (1.8.1)"');
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
    outcome: { Failed: { exit_code: 1, summary } },
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
      "The lines above are Homebrew's own error. You can click Retry later. If it still fails, click Copy Log and send the log to someone who can help.",
    );
    // After the log, in the dialog's body; Copy Log is in the foot below it.
    const log = getByRole("log");
    expect(log.compareDocumentPosition(step) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(step).toHaveClass("text-body", "text-foreground");
    // With no step over the log, it is what the dialog says it is about.
    expect(getByRole("dialog").getAttribute("aria-describedby")?.split(" ")).toContain(step.id);
  });

  it("says it in Chinese, fitted to the cause the tool's words give", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      operations = [failedUpdate("Error: wget: something went wrong")];
      const generic = renderWithProviders(<LogDrawer />);
      said("Error: wget: something went wrong");
      await generic.findByText(
        "上面是Homebrew自己的报错。可以稍后点按“重试”；还是失败，就点按“拷贝日志”，发给懂的人看。",
      );
      generic.unmount();

      const cases: Array<[string, string]> = [
        ["curl: (6) Could not resolve host: ghcr.io", "上面是Homebrew自己的报错。网络恢复后可以点按“重试”；还是失败，就点按“拷贝日志”，发给懂的人看。"],
        ["Error: No space left on device @ rb_sysopen", "上面是Homebrew自己的报错。腾出磁盘空间后可以点按“重试”；还是失败，就点按“拷贝日志”，发给懂的人看。"],
        ["Error: Permission denied @ dir_s_mkdir - /opt/homebrew/Cellar", "上面是Homebrew自己的报错。要先改好它的文件权限，才能点按“重试”；可以点按“拷贝日志”，发给懂的人看。"],
        ["Error: Another active Homebrew process is already in progress.", "上面是Homebrew自己的报错。等另一个操作结束后可以点按“重试”；还是失败，就点按“拷贝日志”，发给懂的人看。"],
        ["sudo: a terminal is required to read the password; either use the -S option to read from standard input or configure an askpass helper", "上面是Homebrew自己的报错。在这里点按“重试”还会停在同一步；可以点按“拷贝日志”，发给懂的人看。"],
        ["sudo: 3 incorrect password attempts", "上面是Homebrew自己的报错。可以点按“重试”，在弹出的密码窗口中输入Mac的登录密码；还是失败，就点按“拷贝日志”，发给懂的人看。"],
      ];
      for (const [line, sentence] of cases) {
        useUiStore.setState({ logs: [] });
        operations = [failedUpdate(line)];
        const view = renderWithProviders(<LogDrawer />);
        said(line);
        expect(await view.findByText(sentence)).toBeInTheDocument();
        view.unmount();
      }
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("names the source that wrote the lines and how to try again for an uninstall, in both languages", async () => {
    operations = [
      failedUpdate("npm error code ETIMEDOUT", {
        kind: "Uninstall",
        instance_id: "npm:/opt/homebrew/bin/npm",
        argv_preview: ["/opt/homebrew/bin/npm", "uninstall", "-g", "prettier"],
      }),
    ];
    const en = renderWithProviders(<LogDrawer />);
    said("npm error code ETIMEDOUT");
    await en.findByText(
      "The lines above are npm's own error. Once you're back online, you can uninstall it again. If it still fails, click Copy Log and send the log to someone who can help.",
    );
    en.unmount();

    await i18n.changeLanguage("zh-CN");
    try {
      const zh = renderWithProviders(<LogDrawer />);
      await zh.findByText("上面是npm自己的报错。网络恢复后可以重新卸载；还是失败，就点按“拷贝日志”，发给懂的人看。");
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("says nothing of the kind where no tool's words are above it", async () => {
    // Banager's own failure: its words, not a tool's.
    operations = [{ ...runningOp, status: "Done", outcome: { BanagerFailed: "Panicked" } }];
    const own = renderWithProviders(<LogDrawer />);
    await own.findByText("Couldn't finish because of an internal error");
    expect(own.container.ownerDocument.querySelector("[data-failure-next-step]")).toBeNull();
    own.unmount();

    // A failure whose log this window does not have: nothing is above.
    operations = [failedUpdate("Error: wget: something went wrong")];
    const empty = renderWithProviders(<LogDrawer />);
    await empty.findByText("Couldn't finish");
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
    const under = await findByText(/^The lines above are Homebrew's own error\. Once you're back online/);
    const described = getByRole("dialog").getAttribute("aria-describedby")?.split(" ") ?? [];
    expect(described).toContain(over.id);
    expect(described).not.toContain(under.id);
  });
});
