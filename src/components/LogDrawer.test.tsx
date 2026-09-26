import { describe, expect, it, beforeEach, vi } from "vitest";
import { act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { LogDrawer } from "./LogDrawer";
import { useUiStore } from "../store/ui";
import i18n from "../i18n";

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

const runningOp = {
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

  it("renders Canager's own notes in the user's language, in place among the tool's lines", async () => {
    // A note is Canager speaking, so it goes through the locale files: a
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
      const lines = Array.from(getByRole("log").querySelectorAll("p")).map((p) => p.textContent);
      expect(lines).toEqual([
        "Homebrew 还在下载最新的软件目录，Canager 要等它下载完再开始。通常要几分钟，最多等 10 分钟。不想等的话可以点“取消”，现在还什么都没有改动。",
        "==> Pouring jq",
        "Canager 无法继续读取该命令的错误信息（Input/output error (os error 5)），错误信息到此为止。",
      ]);
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

    await findByText(
      "Canager couldn't read any more of this command's output (EIO), so its output ends here.",
    );
  });

  it("words each path a path-list uninstall moved, and the one macOS refused", async () => {
    // Canager's own two lines in an uninstall that runs no command: where
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

    await findByText("Moved ~/.local/share/claude to the Trash (now at ~/.Trash/claude).");
    await findByText(
      "Couldn't move ~/.local/bin/claude to the Trash, so the uninstall stopped here. Your Mac gave this reason: Operation not permitted",
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
      "The 120 seconds Canager allows this uninstall ran out, so it stopped before moving ~/.local/bin/claude. Anything it already moved is in the Trash; it hasn't touched anything else. You can uninstall again to finish the rest.",
    );
  });

  it("names what came back after a path-list uninstall moved everything, under the outcome that says so", async () => {
    // The run's own last look, once the pause after its last move is
    // over: one line per path it found there, and the outcome pointing at
    // those lines -- with the launcher gone, nothing else on screen shows
    // them.
    mockInvoke.mockResolvedValue([
      { ...runningOp, status: "Done", outcome: { NeedsAttention: "BackAfterUninstall" } },
    ]);
    const { findByText } = renderWithProviders(<LogDrawer />);

    act(() => {
      useUiStore.getState().appendLog({
        opId: 1,
        note: { BackAfterUninstall: { path: "~/.local/share/claude" } },
      });
    });

    await findByText(
      "~/.local/share/claude came back after everything on the list had gone to the Trash. Canager left it where it is.",
    );
    await findByText(
      "Needs attention: everything on the list went to the Trash, but part of it came back afterwards — a copy of the tool that was still running can do that. The operation log names what came back. Quit the tool, then uninstall it again if it's still listed, or move what came back to the Trash yourself.",
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

  it("shows the terminal outcome once the operation finishes", async () => {
    mockInvoke.mockResolvedValue([{ ...runningOp, status: "Done", outcome: "Succeeded" }]);

    const { findByText } = renderWithProviders(<LogDrawer />);

    await findByText("Succeeded");
  });

  it("words Canager's own failure in the user's language, quoting only the path", async () => {
    // This used to arrive as `Failed` with Rust's English in its summary
    // ("runner: program not found: /opt/homebrew/bin/brew"), printed inside
    // the translated "失败：" frame.
    mockInvoke.mockResolvedValue([
      {
        ...runningOp,
        status: "Done",
        outcome: { CanagerFailed: { ProgramMissing: { program: "/opt/homebrew/bin/brew" } } },
      },
    ]);
    await i18n.changeLanguage("zh-CN");
    try {
      const { findByText, queryByText } = renderWithProviders(<LogDrawer />);
      await findByText(
        "失败：Canager 找不到 /opt/homebrew/bin/brew，它可能在 Canager 上次检查之后被删掉了。什么都没有改动。",
      );
      expect(queryByText(/program not found/)).not.toBeInTheDocument();
    } finally {
      await i18n.changeLanguage("en");
    }
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
    expect(getByRole("dialog")).toHaveFocus();

    await user.keyboard("{Escape}");
    expect(useUiStore.getState().drawerOpen).toBe(false);
    expect(opener).toHaveFocus();
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
    // controls the user can neither see nor tell they are on.
    const user = userEvent.setup();
    const { getByRole } = renderWithProviders(<DrawerInPage />);
    const dialog = getByRole("dialog");
    const behind = getByRole("button", { name: "behind the drawer" });
    const opener = getByRole("button", { name: "show the log" });

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
