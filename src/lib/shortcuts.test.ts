import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { MENU_EVENTS } from "./api";
import { SHORTCUT_GROUPS, type Shortcut, type ShortcutId } from "./shortcuts";

/**
 * The sheet lists only keys there is code for. A row of the menu bar's own
 * shortcuts is checked against src-tauri/src/menu.rs, the accelerator
 * itself; every other row against the code that handles its keys and the
 * test that presses them there. A row added without either fails here.
 */

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");
const read = (file: string) => readFileSync(path.join(ROOT, file), "utf-8");
const MENU_RS = read("src-tauri/src/menu.rs");

const ALL: Shortcut[] = SHORTCUT_GROUPS.flatMap((group) => group.shortcuts);

/** `fn <name>`'s body in menu.rs: what follows its signature, up to the next `pub fn`. */
function body(name: string): string {
  const start = MENU_RS.indexOf(`pub fn ${name}(`);
  expect(start, `no fn ${name} in menu.rs`).toBeGreaterThanOrEqual(0);
  const end = MENU_RS.indexOf("pub fn ", start + 1);
  return MENU_RS.slice(start, end);
}

/** `PageCommand` variant → what `fn <name>` gives it, from its `PageCommand::X => "…"` arms. */
function arms(name: string, value: RegExp): Map<string, string> {
  const found = new Map<string, string>();
  const arm = new RegExp(`PageCommand::(\\w+) => ${value.source}`, "g");
  for (const match of body(name).matchAll(arm)) found.set(match[1], match[2]);
  return found;
}

/** A tauri accelerator as a Mac's menu writes it: "CmdOrCtrl+Comma" → "⌘,". */
function asMacKeys(accelerator: string): string {
  const [modifier, key] = accelerator.split("+");
  expect(modifier).toBe("CmdOrCtrl");
  const named: Record<string, string> = { Comma: "," };
  return `⌘${named[key] ?? key}`;
}

describe("the shortcuts sheet's rows of the menu bar", () => {
  const ids = arms("id", /"([a-z-]+)"/);
  const accelerators = arms("shortcut", /Some\("([^"]+)"\)/);

  it("read menu.rs: eleven items, seven with a shortcut", () => {
    expect(ids.size).toBe(11);
    expect(accelerators.size).toBe(7);
  });

  it("give each of Banager's items the keys menu.rs gives it", () => {
    const rows = ALL.filter((shortcut) => shortcut.menu !== undefined);
    for (const row of rows) {
      const event = MENU_EVENTS[row.menu!];
      const variant = [...ids].find(([, id]) => `menu://${id}` === event)?.[0];
      expect(variant, `${row.id}: no item in menu.rs sends ${event}`).toBeDefined();
      const accelerator = accelerators.get(variant!);
      expect(accelerator, `${row.id}: ${variant} has no shortcut in menu.rs`).toBeDefined();
      expect(row.keys, row.id).toBe(asMacKeys(accelerator!));
    }
  });

  it("list every shortcut menu.rs gives an item of Banager's, once", () => {
    const listed = ALL.filter((shortcut) => shortcut.menu !== undefined).map((shortcut) => MENU_EVENTS[shortcut.menu!]);
    const given = [...accelerators.keys()].map((variant) => `menu://${ids.get(variant)}`);
    expect([...listed].sort()).toEqual([...given].sort());
  });

  it("name macOS's own Close Window and Quit, which are in the menu bar, and AppKit's keys for them", () => {
    // tauri's predefined items carry AppKit's own shortcuts (menu.rs's
    // `MacItem` says which): ⌘W for Close Window, ⌘Q for Quit.
    const appKit = { CloseWindow: "⌘W", Quit: "⌘Q" } as const;
    const rows = ALL.filter((shortcut) => shortcut.macItem !== undefined);
    expect(rows.map((row) => row.macItem)).toEqual(["CloseWindow", "Quit"]);
    for (const row of rows) {
      expect(row.keys).toBe(appKit[row.macItem!]);
      expect(MENU_RS).toContain(`mac(MacItem::${row.macItem}, `);
    }
    expect(MENU_RS).toContain("MacItem::CloseWindow => PredefinedMenuItem::close_window(app, label)");
    expect(MENU_RS).toContain("MacItem::Quit => PredefinedMenuItem::quit(app, label)");
    // What Banager makes of them: ⌘W hides the window, and Banager runs on
    // (window.rs); ⌘Q asks first while an operation is under way (quit.rs).
    expect(read("src-tauri/src/window.rs")).toMatch(/Close::HideWindow => window\.hide\(\)/);
    expect(read("src-tauri/src/quit.rs")).toMatch(/pub fn should_quit<R: Runtime>/);
  });
});

/** Where a row's keys are handled: a file and what in it handles them. */
interface Handler {
  file: string;
  handles: RegExp[];
}

/** A test that presses the keys there: its file and its title. */
interface Pressed {
  file: string;
  title: string;
}

/**
 * Every row that is not one of the menu bar's: the code that handles its
 * keys, and a test that presses them. Typed over every such row, so a new
 * one does not compile until it is here.
 */
type KeyRowId = Exclude<
  ShortcutId,
  "settings" | "overview" | "updates" | "installed" | "unknown" | "checkAgain" | "search" | "closeWindow" | "quit"
>;
const KEY_ROWS: Record<KeyRowId, { handlers: Handler[]; pressed: Pressed[] }> = {
  move: {
    handlers: [
      { file: "src/components/VirtualList.tsx", handles: [/const LIST_KEYS[^\n]*"ArrowDown", "ArrowUp"/] },
      { file: "src/components/Sidebar.tsx", handles: [/ArrowDown: \(at, count\)/, /ArrowUp: \(at\)/] },
      { file: "src/components/ui/Menu.tsx", handles: [/case "ArrowDown":/, /case "ArrowUp":/] },
    ],
    pressed: [
      { file: "src/components/VirtualList.test.tsx", title: "moves the focus to the next row with ↓ and back with ↑, passing over what is not a row" },
      { file: "src/components/Sidebar.test.tsx", title: "moves with ↑ and ↓ through the pages and the sources as one list, and Home and End to its ends" },
      { file: "src/components/ui/Menu.test.tsx", title: "moves between the items with the arrow keys, Home and End, and wraps round" },
    ],
  },
  page: {
    handlers: [{ file: "src/components/VirtualList.tsx", handles: [/"PageDown", "PageUp"/, /event\.key === "PageDown" \|\| event\.key === "PageUp"/] }],
    pressed: [{ file: "src/components/VirtualList.test.tsx", title: "moves a box's height with Page Down and Page Up, to the last row and the first past either end" }],
  },
  ends: {
    handlers: [
      { file: "src/components/VirtualList.tsx", handles: [/"Home", "End"\]/] },
      { file: "src/components/Sidebar.tsx", handles: [/Home: \(\) => 0/, /End: \(_, count\) => count - 1/] },
      { file: "src/components/ui/Menu.tsx", handles: [/case "Home":/, /case "End":/] },
    ],
    pressed: [
      { file: "src/components/VirtualList.test.tsx", title: "goes to the first row with Home and to the last with End, passing over what is not a row" },
      { file: "src/components/Sidebar.test.tsx", title: "moves with ↑ and ↓ through the pages and the sources as one list, and Home and End to its ends" },
    ],
  },
  tick: {
    handlers: [
      { file: "src/components/ToolRow.tsx", handles: [/event\.key === " "/, /selectable\.onToggle\(\);\s*else onOpen\?\.\(\);/] },
      { file: "src/pages/InstalledPage.tsx", handles: [/onOpen=\{\(\) => pressRow\(artifact\)\}/] },
    ],
    pressed: [
      { file: "src/components/ToolRow.test.tsx", title: "takes the focus itself in a list with arrow keys: Space ticks its box, Enter does nothing" },
      { file: "src/components/ToolRow.test.tsx", title: "opens with Space a row with no checkbox among rows that have one, the Installed page's" },
    ],
  },
  details: {
    handlers: [
      { file: "src/components/ToolRow.tsx", handles: [/event\.key === "Enter"/, /onEnter\?\.\(\);/] },
      { file: "src/pages/InstalledPage.tsx", handles: [/onEnter=\{\(\) => enterRow\(artifact\)\}/] },
    ],
    pressed: [
      { file: "src/pages/InstalledPage.test.tsx", title: "opens on Enter from a row and puts the focus on its heading, and Escape hands it back to the row" },
    ],
  },
  closeDetails: {
    handlers: [{ file: "src/pages/InstalledPage.tsx", handles: [/event\.key !== "Escape"/, /closeInspector\(\);/] }],
    pressed: [
      { file: "src/pages/InstalledPage.test.tsx", title: "closes with Escape, from the row or from inside it, and with its close button, the focus back on the row" },
    ],
  },
  tab: {
    handlers: [{ file: "src/components/rowTabStop.ts", handles: [/event\.key !== "Tab"/, /const back = event\.shiftKey;/] }],
    pressed: [
      { file: "src/components/VirtualList.test.tsx", title: "goes on from the last control of the row in the Tab order past every other row's, and out of the list" },
      { file: "src/components/VirtualList.test.tsx", title: "comes in on the row in the Tab order, and on its last control from below" },
    ],
  },
  press: {
    // Return presses the focused button, as WebKit has a button do; what
    // Banager decides is which button has the focus as a dialog opens.
    handlers: [
      { file: "src/components/ui/Dialog.tsx", handles: [/onOpenAutoFocus=/, /target\.focus\(\);/] },
      { file: "src/components/UninstallDialog.tsx", handles: [/initialFocus=\{cancelRef\}/] },
    ],
    pressed: [
      { file: "src/components/ui/Dialog.test.tsx", title: "puts the focus where it is told to as it opens" },
      { file: "src/components/UninstallDialog.test.tsx", title: "puts the focus on Cancel as it opens, and makes Uninstall the default button, not a red one" },
    ],
  },
  escape: {
    handlers: [
      { file: "src/components/ui/Dialog.tsx", handles: [/onEscapeKeyDown=/] },
      { file: "src/components/ui/floating.ts", handles: [/if \(event\.key !== "Escape"\) return;/] },
    ],
    pressed: [
      { file: "src/components/ui/Dialog.test.tsx", title: "closes an open ⓘ first on Escape, and only that" },
      { file: "src/components/ui/Menu.test.tsx", title: "closes on Escape, with focus back on the button, and on a click outside" },
    ],
  },
};

describe("the shortcuts sheet's rows of keys the page handles", () => {
  it("are every row that is not the menu bar's", () => {
    const keyRows = ALL.filter((shortcut) => shortcut.menu === undefined && shortcut.macItem === undefined);
    expect(keyRows.map((row) => row.id).sort()).toEqual(Object.keys(KEY_ROWS).sort());
  });

  it.each(Object.entries(KEY_ROWS))("%s: handled in the code, and pressed in a test", (_, { handlers, pressed }) => {
    for (const { file, handles } of handlers) {
      const source = read(file);
      for (const handle of handles) expect(source, `${file} has no ${handle}`).toMatch(handle);
    }
    for (const { file, title } of pressed) {
      expect(read(file), `${file} has no test "${title}"`).toContain(`it("${title}"`);
    }
  });
});
