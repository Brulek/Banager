/**
 * Help's 「键盘快捷键」 (src-tauri/src/menu.rs): the keys Banager answers
 * to, for the sheet that lists them (src/components/ShortcutsSheet.tsx).
 * Only keys there is code for: the menu bar's own shortcuts, as menu.rs
 * gives them -- Banager's items' and the two of macOS's whose effect
 * Banager decides (⌘W only hides the window, ⌘Q asks first while an
 * operation runs) -- and the keys the lists, the sidebar, the menus and the
 * dialogs handle. src/lib/shortcuts.test.ts ties each row to its
 * accelerator in menu.rs or to the code that handles it.
 */
import { create } from "zustand";
import type { MenuCommand } from "./api";

/** The sheet's groups, in its order: the window's keys, the lists', the dialogs'. */
export type ShortcutGroupId = "window" | "list" | "dialog";

/** One row's name: its words are `shortcuts.rows.<id>` in src/i18n. */
export type ShortcutId =
  | "settings"
  | "overview"
  | "updates"
  | "installed"
  | "unknown"
  | "checkAgain"
  | "search"
  | "closeWindow"
  | "quit"
  | "move"
  | "page"
  | "ends"
  | "tick"
  | "details"
  | "closeDetails"
  | "tab"
  | "press"
  | "escape";

export interface Shortcut {
  id: ShortcutId;
  /**
   * The keys as a Mac's menus write them -- ⌘ ⇧ ⌥ ⌃ for the modifiers, ↩ ⎋
   * ⇥ ␣ ↑ ↓ for the keys with no letter -- modifiers first and glued to
   * the key; two keys that do the same in either direction, a space apart.
   */
  keys: string;
  /** The menu bar's item it is the shortcut of, where it is one of Banager's own. */
  menu?: MenuCommand;
  /** macOS's own item it is the shortcut of, where Banager decides what that item does. */
  macItem?: "CloseWindow" | "Quit";
}

export interface ShortcutGroup {
  id: ShortcutGroupId;
  shortcuts: readonly Shortcut[];
}

export const SHORTCUT_GROUPS: readonly ShortcutGroup[] = [
  {
    id: "window",
    shortcuts: [
      { id: "settings", keys: "⌘,", menu: "settings" },
      { id: "overview", keys: "⌘1", menu: "overview" },
      { id: "updates", keys: "⌘2", menu: "updates" },
      { id: "installed", keys: "⌘3", menu: "installed" },
      { id: "unknown", keys: "⌘4", menu: "unknown" },
      { id: "checkAgain", keys: "⌘R", menu: "checkAgain" },
      { id: "search", keys: "⌘F", menu: "search" },
      { id: "closeWindow", keys: "⌘W", macItem: "CloseWindow" },
      { id: "quit", keys: "⌘Q", macItem: "Quit" },
    ],
  },
  {
    id: "list",
    shortcuts: [
      { id: "move", keys: "↑ ↓" },
      { id: "page", keys: "⇞ ⇟" },
      { id: "ends", keys: "↖ ↘" },
      { id: "tick", keys: "␣" },
      { id: "details", keys: "↩" },
      { id: "closeDetails", keys: "⎋" },
      { id: "tab", keys: "⇥ ⇧⇥" },
    ],
  },
  {
    id: "dialog",
    shortcuts: [
      { id: "press", keys: "↩" },
      { id: "escape", keys: "⎋" },
    ],
  },
];

/** Whether the sheet is open: Help's item opens it, wherever the window is. */
export const useShortcutsSheet = create<{ open: boolean }>(() => ({ open: false }));

/** Opens the sheet over whatever page is showing. */
export function openShortcutsSheet(): void {
  useShortcutsSheet.setState({ open: true });
}
