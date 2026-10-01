import { useEffect, useRef, type RefObject } from "react";

/**
 * What Tab reaches, in the page's order: what the engine's own Tab would
 * stop at, short of what only layout can tell (a box that is not drawn
 * cannot take the focus, and `focusFirst` below moves past it).
 */
const TABBABLE =
  'a[href], button, input:not([type="hidden"]), select, textarea, [tabindex], [contenteditable="true"]';

function tabbables(): HTMLElement[] {
  return Array.from(document.querySelectorAll<HTMLElement>(TABBABLE)).filter(
    (element) =>
      element.tabIndex >= 0 &&
      !(element as HTMLButtonElement).disabled &&
      element.closest("[inert], [hidden]") === null,
  );
}

/**
 * Whether Tab passes over `element`, in `list`: it is in a row of the list
 * that is not the one in the Tab order (its `data-row-focus` is at -1).
 * A slot of the list with no row of its own -- the notices over the
 * Updates and Installed lists -- is not passed over.
 */
function passedOver(element: HTMLElement, list: HTMLElement): boolean {
  if (!list.contains(element)) return false;
  const slot = element.closest<HTMLElement>("[data-list-slot]");
  const row = slot?.querySelector<HTMLElement>("[data-row-focus]");
  return row !== null && row !== undefined && row.tabIndex < 0;
}

/**
 * A list whose rows ↑ and ↓ move between (`VirtualList`'s `keyboardRows`)
 * is one stop for Tab, as a Mac's list is: the row in the Tab order and
 * the controls on it -- its checkbox, its Update, its ⋯ -- and then on
 * past the list. The controls of every other row drawn are left out of
 * Tab's way, which otherwise stopped at each of them in turn, two to four
 * a row, before anything after the list -- the details beside it, the
 * list's way out. They are still pressed with the pointer, and reached
 * from the keyboard by going to their row with ↑ ↓ first.
 *
 * Coming in from outside: on the row in the Tab order (Tab), or on the
 * last control of it (Shift-Tab), as the page's order has them. While that
 * row is scrolled out of the DOM, Tab onto the rows puts the focus on it,
 * scrolled back into sight (`focusActive`), rather than passing every row
 * by. Only where Tab would land on a control passed over: anything
 * else is left to the engine, and so is Tab inside an open dialog, which
 * keeps the focus to itself.
 */
export function useRowTabStop(
  listRef: RefObject<HTMLElement | null>,
  enabled: boolean,
  focusActive: () => void,
): void {
  const focusActiveRef = useRef(focusActive);
  focusActiveRef.current = focusActive;
  useEffect(() => {
    if (!enabled) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Tab" || event.defaultPrevented || event.altKey || event.metaKey || event.ctrlKey) return;
      const list = listRef.current;
      if (list === null) return;
      const from = document.activeElement instanceof HTMLElement ? document.activeElement : null;
      // A dialog over the page keeps Tab to itself.
      const dialog = from?.closest('[role="dialog"], [role="alertdialog"]');
      if (dialog !== null && dialog !== undefined && !dialog.contains(list)) return;
      const back = event.shiftKey;
      const all = tabbables();
      // Where the engine's own Tab goes from here: the next (or the
      // previous) in the page's order, whether or not `from` itself is a
      // stop -- a menu's item, a row at -1.
      const after = (element: HTMLElement) =>
        from === null || from === document.body
          ? true
          : back
            ? (element.compareDocumentPosition(from) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0
            : (from.compareDocumentPosition(element) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0;
      // In Tab's direction, and round from the page's other end past its
      // last stop, as the window goes round.
      const ordered = back ? [...all].reverse() : all;
      const ahead = [
        ...ordered.filter((element) => element !== from && after(element)),
        ...ordered.filter((element) => element !== from && !after(element)),
      ];
      const next = ahead[0];
      if (next === undefined || !passedOver(next, list)) return;
      event.preventDefault();
      // Onto the rows from outside them -- from before or after the list,
      // or from a slot of it with no row, its notices, 最近更新 -- with
      // the row in the Tab order not drawn: that row, brought back into
      // sight, rather than every row passed by.
      const fromRow =
        from !== null && list.contains(from) && from.closest("[data-list-slot]")?.querySelector("[data-row-focus]");
      if (!fromRow && list.querySelector('[data-row-focus][tabindex="0"]') === null) {
        focusActiveRef.current();
        return;
      }
      for (const target of ahead.filter((element) => !passedOver(element, list))) {
        target.focus();
        if (document.activeElement === target) return;
      }
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [enabled, listRef]);
}
