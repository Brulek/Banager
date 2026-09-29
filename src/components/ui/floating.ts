/**
 * What the app's two small floating panels share: the detail popover
 * behind a status chip (`Popover`) and a row's ⋯ menu (`Menu`). Both are
 * drawn in the DOM right after the button that opens them, not in a
 * portal, so a screen reader reaches their text in reading order and Tab
 * moves through them where they belong.
 */
import { useEffect, useLayoutEffect, useState, type RefObject } from "react";

/**
 * Closes an open panel the way every macOS popover closes: a click or tap
 * anywhere outside `wrapper` (the button and the panel together), Escape
 * -- which also hands focus back to `trigger`, where the user was -- and
 * focus moving to something outside it, so two panels opened from the
 * keyboard are never open at once. Focus that goes nowhere (a click on
 * the panel's own text, to select a command in it) is not a move.
 */
export function useDismiss(
  open: boolean,
  close: () => void,
  wrapper: RefObject<HTMLElement | null>,
  trigger: RefObject<HTMLElement | null>,
): void {
  useEffect(() => {
    if (!open) return;
    function onPointerDown(event: Event) {
      const target = event.target as Node | null;
      if (target !== null && wrapper.current?.contains(target)) return;
      close();
    }
    function onKeyDown(event: KeyboardEvent) {
      if (event.key !== "Escape") return;
      event.preventDefault();
      close();
      trigger.current?.focus();
    }
    function onFocusIn(event: FocusEvent) {
      const target = event.target as Node | null;
      if (target !== null && !wrapper.current?.contains(target)) close();
    }
    // Capture, so a click that something else stops on its way down
    // still closes the panel.
    document.addEventListener("pointerdown", onPointerDown, true);
    document.addEventListener("mousedown", onPointerDown, true);
    document.addEventListener("keydown", onKeyDown);
    document.addEventListener("focusin", onFocusIn);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown, true);
      document.removeEventListener("mousedown", onPointerDown, true);
      document.removeEventListener("keydown", onKeyDown);
      document.removeEventListener("focusin", onFocusIn);
    };
  }, [open, close, wrapper, trigger]);
}

/** The nearest ancestor that scrolls, whose edges would cut a panel off. */
function scrollingAncestor(element: HTMLElement): HTMLElement | null {
  for (let node = element.parentElement; node !== null; node = node.parentElement) {
    const { overflowY } = getComputedStyle(node);
    if (overflowY === "auto" || overflowY === "scroll") return node;
  }
  return null;
}

/** The box a panel opened from `element` must stay inside: the list it is in, or else the window. */
export function panelBounds(element: HTMLElement): { top: number; bottom: number; left: number; right: number } {
  return (
    scrollingAncestor(element)?.getBoundingClientRect() ?? {
      top: 0,
      bottom: window.innerHeight,
      left: 0,
      right: window.innerWidth,
    }
  );
}

/** Which side of its button a panel opens on, and which of the button's edges it lines up with. */
export interface Placement {
  side: "below" | "above";
  align: "start" | "end";
}

/**
 * Below the button, unless the panel would run past the bottom of the
 * list it is in (or of the window) and there is more room above: a row at
 * the foot of the Updates page opens its menu upwards instead of into the
 * operation bar. Lined up with the button's `preferred` edge, unless the
 * panel would then run past the list's or the window's side and fits the
 * other way: a notice's "Details" near the right of a narrow window opens
 * leftwards instead of past the window's edge. Measured once each time the
 * panel opens, from the button, so the answer does not depend on where
 * the panel happened to be drawn first.
 */
export function usePlacement(
  open: boolean,
  trigger: RefObject<HTMLElement | null>,
  panel: RefObject<HTMLElement | null>,
  preferred: "start" | "end" = "start",
): Placement {
  const [placement, setPlacement] = useState<Placement>({ side: "below", align: preferred });
  useLayoutEffect(() => {
    // Only a change is set: every closed chip and menu on a list runs this
    // when it mounts, and an equal new object would draw each one twice.
    const settle = (next: Placement) =>
      setPlacement((was) => (was.side === next.side && was.align === next.align ? was : next));
    if (!open) {
      settle({ side: "below", align: preferred });
      return;
    }
    const button = trigger.current;
    const content = panel.current;
    if (button === null || content === null) return;
    const bounds = panelBounds(button);
    const rect = button.getBoundingClientRect();
    const needed = content.offsetHeight + 8;
    const below = bounds.bottom - rect.bottom;
    const above = rect.top - bounds.top;
    const width = content.offsetWidth;
    const fitsFromStart = rect.left + width <= bounds.right;
    const fitsFromEnd = rect.right - width >= bounds.left;
    let align = preferred;
    if (preferred === "start" && !fitsFromStart && fitsFromEnd) align = "end";
    if (preferred === "end" && !fitsFromEnd && fitsFromStart) align = "start";
    settle({ side: below < needed && above > below ? "above" : "below", align });
  }, [open, trigger, panel, preferred]);
  return placement;
}
