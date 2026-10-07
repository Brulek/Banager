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
 * A ref for something that leaves the page by itself, the focus perhaps
 * inside it: a source's notice once its problem is fixed, with the button
 * that fixed it -- Fix… once the link has put the program back, Open
 * Ollama once Ollama answers (r24 W2). Once it has gone with the focus in
 * it, and nothing else has taken the focus meanwhile, the focus goes to
 * the page's title (`focusOrFallback`) rather than staying with the
 * window's body. React lets go of the ref while the element is still in
 * the page, so it can tell whether the focus is inside; whether it has
 * gone, rather than been drawn again, is asked once the change is done.
 */
export function refocusWhenGone(node: HTMLElement | null): (() => void) | undefined {
  if (node === null) return undefined;
  return () => {
    if (!node.contains(document.activeElement)) return;
    queueMicrotask(() => {
      if (!node.isConnected && focusLost()) focusOrFallback(null);
    });
  };
}
