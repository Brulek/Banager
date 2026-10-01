import { useEffect, useRef } from "react";
import { focusOrFallback } from "../components/ui/focus";
import type { Page } from "../store/ui";

/**
 * Where the focus goes when the window opens another page and nothing on
 * the new page has taken it: the page's title (`focusOrFallback`), which
 * a screen reader reads as the new page's name, and from where the next
 * Tab goes on into the page. A button on the page before that opened it
 * -- the Overview's Review Updates, its check-for-updates row that opens
 * Settings, a notice's Show -- went with that page, and the focus with it,
 * to the window's body: from there the next Tab started over at the
 * sidebar, and nothing said which page was open.
 *
 * Only where the focus is lost: the sidebar's rows and the menu bar keep
 * it where it was, and a page that puts it somewhere itself (the Installed
 * page's search after ⌘F, a list's first row) does so first -- a parent's
 * effect runs after its children's. Not as the window first opens, which
 * has no page before it.
 */
export function useFocusOnPageChange(page: Page): void {
  const shown = useRef(page);
  useEffect(() => {
    if (shown.current === page) return;
    shown.current = page;
    const focus = document.activeElement;
    if (focus === null || focus === document.body || !focus.isConnected) focusOrFallback(null);
  }, [page]);
}
