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
