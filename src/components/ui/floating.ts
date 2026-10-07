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

/**
 * The box a panel opened from `element` must stay inside: the list it is
 * in, or else the window. Of the list, the part that shows its rows -- its
 * client box, inside any border and any scroll bar it draws: a Mac set to
 * show scroll bars always, or with a mouse plugged in, draws a long list's
 * down its right side, 15 wide, and a panel moved only inside the list's
 * outer edge would sit under it, partly hidden, and make the list scroll
 * sideways (r30 Z2's skeptic).
 */
export function panelBounds(element: HTMLElement): { top: number; bottom: number; left: number; right: number } {
  const list = scrollingAncestor(element);
  if (list === null) return { top: 0, bottom: window.innerHeight, left: 0, right: window.innerWidth };
  const box = list.getBoundingClientRect();
  const left = box.left + list.clientLeft;
  const top = box.top + list.clientTop;
  return { top, bottom: top + list.clientHeight, left, right: left + list.clientWidth };
}

/** Which side of its button a panel opens on, and which of the button's edges it lines up with. */
export interface Placement {
  side: "below" | "above";
  align: "start" | "end";
  /**
   * How far the panel is moved sideways from where `align` puts it, in
   * pixels, leftwards when negative: 0 wherever one of the two edges fits.
   */
  shift: number;
}

/**
 * Below the button, unless the panel would run past the bottom of the
 * list it is in (or of the window) and there is more room above: a row at
 * the foot of the Updates page opens its menu upwards instead of into the
 * operation bar. Lined up with the button's `preferred` edge, unless the
 * panel would then run past the list's or the window's side and fits the
 * other way: a notice's "Details" near the right of a narrow window opens
 * leftwards instead of past the window's edge. Where it fits neither way
 * -- an ⓘ near the middle of a list the details panel has narrowed to a
 * third of an 800-wide window (r30 Z2) -- it keeps the preferred edge and
 * is moved sideways (`shift`) until it is inside the list, so nothing in
 * it is cut off at the list's edge; a popover's arrow, `fromMiddle` from
 * its side, moves the other way and still points at the button, and it is
 * never moved so far that the arrow would come nearer its side than that.
 * Measured each time the panel opens, and again while it is open whenever
 * the window or the list it is in changes size -- dragging the window's
 * edge happens outside the page, so it does not close the panel (r30 Z2's
 * skeptic) -- always from the button, so the answer does not depend on
 * where the panel happened to be drawn before.
 *
 * `fromMiddle`: where the panel's lined-up edge is -- that far to the
 * side of the button's middle, as a popover stands from its arrow
 * (`Popover`), or, `null`, on the button's own edge (`Menu`).
 */
export function usePlacement(
  open: boolean,
  trigger: RefObject<HTMLElement | null>,
  panel: RefObject<HTMLElement | null>,
  preferred: "start" | "end" = "start",
  fromMiddle: number | null = null,
): Placement {
  const [placement, setPlacement] = useState<Placement>({ side: "below", align: preferred, shift: 0 });
  useLayoutEffect(() => {
    // Only a change is set: every closed chip and menu on a list runs this
    // when it mounts, and an equal new object would draw each one twice.
    const settle = (next: Placement) =>
      setPlacement((was) =>
        was.side === next.side && was.align === next.align && was.shift === next.shift ? was : next,
      );
    if (!open) {
      settle({ side: "below", align: preferred, shift: 0 });
      return;
    }
    const button = trigger.current;
    if (button === null || panel.current === null) return;
    const measure = () => {
      const content = panel.current;
      if (content === null) return;
      const bounds = panelBounds(button);
      const rect = button.getBoundingClientRect();
      const needed = content.offsetHeight + 8;
      const below = bounds.bottom - rect.bottom;
      const above = rect.top - bounds.top;
      const width = content.offsetWidth;
      const middle = rect.left + rect.width / 2;
      // The panel's left side lined up from the start, its right from the end.
      const startLeft = fromMiddle === null ? rect.left : middle - fromMiddle;
      const endRight = fromMiddle === null ? rect.right : middle + fromMiddle;
      const fitsFromStart = startLeft + width <= bounds.right;
      const fitsFromEnd = endRight - width >= bounds.left;
      let align = preferred;
      if (preferred === "start" && !fitsFromStart && fitsFromEnd) align = "end";
      if (preferred === "end" && !fitsFromEnd && fitsFromStart) align = "start";
      settle({
        side: below < needed && above > below ? "above" : "below",
        align,
        shift: shiftInside(align === "start" ? startLeft : endRight - width, width, bounds, middle, fromMiddle),
      });
    };
    measure();
    // The window, for a panel kept inside it; the list, which can also be
    // narrowed without the window changing. Each answer that is the same
    // as the last is not set again (`settle`).
    window.addEventListener("resize", measure);
    const list = scrollingAncestor(button);
    let watcher: ResizeObserver | null = null;
    if (list !== null) {
      watcher = new ResizeObserver(measure);
      watcher.observe(list);
    }
    return () => {
      window.removeEventListener("resize", measure);
      watcher?.disconnect();
    };
  }, [open, trigger, panel, preferred, fromMiddle]);
  return placement;
}

/**
 * How far to move a panel whose left side is at `left` so that it is
 * inside `bounds`: back from the right side first, then off the left one,
 * which wins in a list narrower than the panel, where its start is what
 * is read first. With an arrow (`fromMiddle`), never so far that the
 * arrow, pointing at `middle`, would come nearer either of the panel's
 * sides than `fromMiddle`. In whole pixels, rounded inwards, so its text
 * is not drawn between two and no part of a pixel is left over the side.
 */
function shiftInside(
  left: number,
  width: number,
  bounds: { left: number; right: number },
  middle: number,
  fromMiddle: number | null,
): number {
  let shift = 0;
  if (left + width > bounds.right) shift = bounds.right - (left + width);
  if (left + shift < bounds.left) shift = bounds.left - left;
  if (fromMiddle !== null) {
    shift = Math.min(shift, middle - fromMiddle - left);
    shift = Math.max(shift, middle + fromMiddle - width - left);
  }
  return shift < 0 ? Math.floor(shift) : Math.ceil(shift);
}
