import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "../App";
import i18n from "../i18n";
import { openShortcutsSheet, SHORTCUT_GROUPS, useShortcutsSheet } from "../lib/shortcuts";
import { fakeMenuBar } from "../test/menuBar";
import { renderWithProviders } from "../test/setup";
import { ShortcutsSheet } from "./ShortcutsSheet";

const mockInvoke = vi.mocked(invoke);

beforeEach(() => {
  useShortcutsSheet.setState({ open: false });
});

/** Each row of the open sheet as "words | keys", group by group. */
function rows(dialog: HTMLElement): string[][] {
  return [...dialog.querySelectorAll("[data-shortcut-group]")].map((group) =>
    [...group.querySelectorAll("[data-shortcut]")].map(
      (row) => `${row.querySelector("span")?.textContent} | ${row.querySelector("kbd")?.textContent}`,
    ),
  );
}

describe("ShortcutsSheet", () => {
  it("lists the window's, the lists' and the dialogs' keys under their titles, Done focused", async () => {
    renderWithProviders(<ShortcutsSheet />);
    expect(screen.queryByRole("dialog")).toBeNull();

    act(() => openShortcutsSheet());

    const dialog = await screen.findByRole("dialog", { name: "Keyboard Shortcuts" });
    expect(within(dialog).getAllByRole("heading", { level: 3 }).map((heading) => heading.textContent)).toEqual([
      "Window",
      "Lists",
      "Dialogs",
    ]);
    expect(rows(dialog)).toEqual([
      [
        "Open Settings | ⌘,",
        "Go to Overview | ⌘1",
        "Go to Updates | ⌘2",
        "Go to Installed | ⌘3",
        "Go to Other Programs | ⌘4",
        "Check installed tools and updates again | ⌘R",
        "Search Installed | ⌘F",
        "Close the window. Anything under way carries on; click the Dock icon to open it again. | ⌘W",
        "Quit. If an operation hasn't finished, you're asked first. | ⌘Q",
      ],
      [
        "Move to the previous or next item in a list, the sidebar or a menu | ↑ ↓",
        "Move up or down a page in a list. On a MacBook, press fn with ↑ or ↓. | ⇞ ⇟",
        "Move to the first or last item. On a MacBook, press fn with ← or →. | ↖ ↘",
        "Tick or untick the row. On Installed, a row without a checkbox shows or hides its details instead. | ␣",
        "On Installed, show the row's details and move to them | ↩",
        "On Installed, close the details and go back to the row | ⎋",
        "Move to the next or previous control. In a list, only the current row is a stop. | ⇥ ⇧⇥",
      ],
      [
        "Press the focused button. An uninstall confirmation opens with the focus on Cancel. | ↩",
        "Close a dialog or menu. An open ⓘ closes first. | ⎋",
      ],
    ]);
    // As many rows as SHORTCUT_GROUPS has, and nothing else in the groups.
    expect(dialog.querySelectorAll("[data-shortcut]")).toHaveLength(
      SHORTCUT_GROUPS.reduce((count, group) => count + group.shortcuts.length, 0),
    );
    expect(within(dialog).getByRole("button", { name: "Done" })).toHaveFocus();
  });

  it("closes with Done and with Escape", async () => {
    renderWithProviders(<ShortcutsSheet />);
    act(() => openShortcutsSheet());
    fireEvent.click(within(await screen.findByRole("dialog")).getByRole("button", { name: "Done" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(useShortcutsSheet.getState().open).toBe(false);

    act(() => openShortcutsSheet());
    fireEvent.keyDown(await screen.findByRole("dialog"), { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(useShortcutsSheet.getState().open).toBe(false);
  });

  it("says it in Chinese, with the same keys", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      renderWithProviders(<ShortcutsSheet />);
      act(() => openShortcutsSheet());
      const dialog = await screen.findByRole("dialog", { name: "键盘快捷键" });
      expect(within(dialog).getAllByRole("heading", { level: 3 }).map((heading) => heading.textContent)).toEqual([
        "窗口",
        "列表",
        "对话框",
      ]);
      const shown = rows(dialog);
      expect(shown[0][0]).toBe("打开设置 | ⌘,");
      expect(shown[0][8]).toBe("退出；有操作没有完成时会先询问 | ⌘Q");
      expect(shown[1][3]).toBe("勾选或取消勾选这一行；在“已安装”中，没有复选框的行会显示或隐藏详细信息 | ␣");
      expect(shown[2]).toEqual([
        "按下有焦点的按钮；卸载前的确认打开时，焦点在“取消”上 | ↩",
        "关闭对话框或菜单；打开着的ⓘ说明会先关闭 | ⎋",
      ]);
      expect(within(dialog).getByRole("button", { name: "完成" })).toHaveFocus();
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("opens from Help's Keyboard Shortcuts over the page that is showing", async () => {
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "list_operations") return [];
      return undefined;
    });
    const menu = fakeMenuBar();
    renderWithProviders(<App />);
    await screen.findByRole("heading", { level: 1, name: "Overview" });
    expect(screen.queryByRole("dialog", { name: "Keyboard Shortcuts" })).toBeNull();

    menu.choose("keyboardShortcuts");

    expect(await screen.findByRole("dialog", { name: "Keyboard Shortcuts" })).toBeInTheDocument();
    // Still behind it, out of reach while the sheet is up.
    expect(screen.getByRole("heading", { level: 1, name: "Overview", hidden: true })).toBeInTheDocument();
  });
});
