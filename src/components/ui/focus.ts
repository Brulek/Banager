/**
 * Gives `element` the focus -- or, when it is gone from the page or cannot
 * take it, the page's title (`PageHeader`), which every page has and which
 * takes the focus by script. A row's Update gives way to its progress as
 * the update starts, and Update selected turns off with nothing left
 * ticked: the focus goes to the title rather than to the window's body,
 * from where the next Tab would start over at the sidebar.
 */
export function focusOrFallback(element: HTMLElement | null | undefined): void {
  if (element?.isConnected) {
    element.focus();
    if (document.activeElement === element) return;
  }
  document.querySelector<HTMLElement>("[data-focus-fallback]")?.focus();
}
