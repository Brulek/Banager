import { describe, expect, it, beforeEach, vi } from "vitest";
import { act, fireEvent, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { LogDrawer } from "./LogDrawer";
import { useUiStore } from "../store/ui";
import i18n from "../i18n";
import type { OpSummary } from "../lib/types";

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
      const lines = Array.from(getByRole("log").querySelectorAll("p"));
      expect(lines.map((p) => p.textContent)).toEqual([
        "Homebrew 正在更新软件清单，完成后开始，最多等 10 分钟。现在取消不会有任何改动。",
        "==> Pouring jq",
        "读不到后续错误信息了：Input/output error (os error 5)",
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
      "Time ran out after 120 seconds, so Canager stopped before moving ~/.local/bin/claude. What it moved is in the Trash; uninstall again to move the rest.",
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

    await findByText("Succeeded");
    expect(queryByText("Running")).toBeNull();
  });

  it("offers Cancel while the operation runs, named for what it stops, and it reaches cancel_operation", async () => {
    // The page under the drawer, the operation bar's Cancel with it, is
    // out of reach while it is open.
    const { findByRole } = renderWithProviders(<LogDrawer />);

    fireEvent.click(await findByRole("button", { name: "Cancel install" }));

    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("cancel_operation", { opId: 1 }));
  });

  it("offers no Cancel once the operation is done, nor for a running one that cannot be stopped", async () => {
    operations = [{ ...runningOp, status: "Done", outcome: "Succeeded" }];
    const done = renderWithProviders(<LogDrawer />);
    await done.findByText("Succeeded");
    expect(done.queryByRole("button", { name: "Cancel install" })).toBeNull();
    done.unmount();

    operations = [{ ...runningOp, kind: "Upgrade", name: "rustup", cancel_policy: "NoCancel" }];
    const noCancel = renderWithProviders(<LogDrawer />);
    await noCancel.findByRole("dialog", { name: "Update rustup" });
    expect(noCancel.queryByRole("button", { name: "Cancel update" })).toBeNull();
  });

  it("says what to do next under an outcome that leaves the user a step", async () => {
    // A crash may have run the command (T9): the next step is to look,
    // never "nothing changed".
    operations = [{ ...runningOp, status: "Done", outcome: { CanagerFailed: "Panicked" } }];

    const { findByText } = renderWithProviders(<LogDrawer />);

    await findByText("Failed: something went wrong inside Canager");
    await findByText("Check the list to see whether anything changed.");
  });

  it("words Canager's own failure in the user's language, quoting only the path", async () => {
    // This used to arrive as `Failed` with Rust's English in its summary
    // ("runner: program not found: /opt/homebrew/bin/brew"), printed inside
    // the translated "失败：" frame.
    operations = [
      {
        ...runningOp,
        status: "Done",
        outcome: { CanagerFailed: { ProgramMissing: { program: "/opt/homebrew/bin/brew" } } },
      },
    ];
    await i18n.changeLanguage("zh-CN");
    try {
      const { findByText, queryByText, findByRole } = renderWithProviders(<LogDrawer />);
      await findByRole("dialog", { name: "安装 jq" });
      await findByText("失败：找不到 /opt/homebrew/bin/brew，没有改动");
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
