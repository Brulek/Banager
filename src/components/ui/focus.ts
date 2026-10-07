/**
 * Gives `element` the focus -- or, when it is gone from the page or cannot
 * take it, the page's title (`PageHeader`), which every page has and which
 * takes the focus by script: the last resort, where nothing better is
 * known -- an update that starts puts the focus on its row or on the list
 * instead (`useUpdateConfirm`'s `onStarted`) -- rather than the window's
 * body, from where the next Tab would start over at the sidebar.
 */
export function focusOrFallback(element: HTMLElement | null | undefined): void {
  if (element?.isConnected) {
    element.focus();
    if (document.activeElement === element) return;
  }
  document.querySelector<HTMLElement>("[data-focus-fallback]")?.focus();
}

/**
 * Whether nothing has the focus: the window's body has it, or what had it
 * has gone from the page.
 */
export function focusLost(): boolean {
  const focus = document.activeElement;
  return focus === null || focus === document.body || !focus.isConnected;
}

/**
 * Whether `node` shows, at least in part, in the box of the list it is in
 * (`VirtualList`'s, `[data-list]`), as the box is scrolled now -- measured
 * by its row (`[data-list-slot]`), which has a box of its own where what
 * is in it may not. A row the list has been scrolled away from is not
 * in sight, though the list still draws it (one either side). Anything
 * in no list is; so is everything where nothing is laid out (a box with
 * no height).
 */
export function inSightOfList(node: Element): boolean {
  const box = node.closest("[data-list]");
  if (box === null) return true;
  const outer = box.getBoundingClientRect();
  if (outer.height === 0) return true;
  const inner = (node.closest("[data-list-slot]") ?? node).getBoundingClientRect();
  return inner.bottom > outer.top && inner.top < outer.bottom;
}

/**
 * A ref for something that leaves the page by itself, the focus perhaps
 * inside it: a source's notice once its problem is fixed, with the button
 * that fixed it -- Fix… once the link has put the program back, Open
 * Ollama once Ollama answers (r24 W2). Once it has gone with the focus in
 * it, in sight (`inSightOfList`), and nothing else has taken the focus
 * meanwhile, the focus goes to the page's title (`focusOrFallback`)
 * rather than staying with the window's body. Not where it went because
 * the user scrolled its list away from it: that moves no focus, as it
 * did not before. React lets go of the ref while the element is still in
 * the page, so it can tell whether the focus is inside; whether it has
 * gone, rather than been drawn again, is asked once the change is done.
 */
export function refocusWhenGone(node: HTMLElement | null): (() => void) | undefined {
  if (node === null) return undefined;
  return () => {
    if (!node.contains(document.activeElement) || !inSightOfList(node)) return;
    queueMicrotask(() => {
      if (!node.isConnected && focusLost()) focusOrFallback(null);
    });
  };
}
