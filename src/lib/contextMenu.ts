/**
 * Right-click, as a Mac app has it. The window is a web view, whose own
 * menu on a page is a browser's -- Reload, and in a debug build Inspect
 * Element -- which no Mac app shows. In a build, then, a right-click shows
 * a menu only where a Mac app would: on a text field, and on text the
 * user has selected.
 */
import { useEffect } from "react";

/**
 * The input types a user types into. Their menu is a Mac text field's
 * -- Cut, Copy, Paste -- where a checkbox's would be the page's.
 */
const TEXT_INPUT_TYPES = new Set(["text", "search", "email", "url", "tel", "password", "number"]);

function isTextField(target: EventTarget | null): boolean {
  return (
    target instanceof HTMLTextAreaElement ||
    (target instanceof HTMLInputElement && TEXT_INPUT_TYPES.has(target.type))
  );
}

/**
 * Whether the point is on selected text: text the user dragged across,
 * or the word under the pointer, which WebKit on macOS selects as a
 * right-click opens the menu, before the page hears of the click --
 * where text selects at all (`select-text`, index.css). The menu WebKit
 * then shows is the selection's: Copy, Look Up, no Reload. A point beside
 * the selected text is not on it: there WebKit would show the page's.
 */
function onSelectedText(selection: Selection | null, x: number, y: number): boolean {
  if (selection === null || selection.isCollapsed) return false;
  for (let i = 0; i < selection.rangeCount; i += 1) {
    const rects = Array.from(selection.getRangeAt(i).getClientRects());
    if (rects.some((rect) => x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom)) {
      return true;
    }
  }
  return false;
}

/** Whether a right-click keeps the web view's own menu: on a text field, or on selected text. */
function keepsOwnMenu(event: MouseEvent, selection: Selection | null): boolean {
  return isTextField(event.target) || onSelectedText(selection, event.clientX, event.clientY);
}

/**
 * In a build (`import.meta.env.PROD`), a right-click shows no menu but
 * where `keepsOwnMenu` keeps the web view's. Returns what undoes it. In
 * development -- `pnpm tauri dev`, `pnpm tauri:mock`, the browser
 * preview -- it does nothing, so Inspect Element stays a right-click away.
 */
export function suppressBrowserContextMenu(win: Window): () => void {
  if (!import.meta.env.PROD) return () => {};
  const onContextMenu = (event: MouseEvent) => {
    if (!keepsOwnMenu(event, win.getSelection())) event.preventDefault();
  };
  win.addEventListener("contextmenu", onContextMenu);
  return () => win.removeEventListener("contextmenu", onContextMenu);
}

/** `suppressBrowserContextMenu` on the window, for as long as the app is up. */
export function useNoBrowserContextMenu(): void {
  useEffect(() => suppressBrowserContextMenu(window), []);
}
