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

export type Placement = "below" | "above";

/**
 * Below the button, unless the panel would run past the bottom of the
 * list it is in (or of the window) and there is more room above: a row at
 * the foot of the Updates page opens its menu upwards instead of into the
 * operation bar. Measured once each time the panel opens.
 */
export function usePlacement(
  open: boolean,
  trigger: RefObject<HTMLElement | null>,
  panel: RefObject<HTMLElement | null>,
): Placement {
  const [placement, setPlacement] = useState<Placement>("below");
  useLayoutEffect(() => {
    if (!open) {
      setPlacement("below");
      return;
    }
    const button = trigger.current;
    const content = panel.current;
    if (button === null || content === null) return;
    const bounds = scrollingAncestor(button)?.getBoundingClientRect() ?? {
      top: 0,
      bottom: window.innerHeight,
    };
    const rect = button.getBoundingClientRect();
    const needed = content.offsetHeight + 8;
    const below = bounds.bottom - rect.bottom;
    const above = rect.top - bounds.top;
    setPlacement(below < needed && above > below ? "above" : "below");
  }, [open, trigger, panel]);
  return placement;
}
