import { createContext, useContext, useEffect, useState, type ReactNode, type RefObject } from "react";
import { createPortal } from "react-dom";

/**
 * Where a page's own actions go: a box at the right of the window's
 * toolbar (`PageHeader`), before its Check again, which stays at the
 * right end -- Update all on the Updates page, the sort and the search on
 * the Installed page, as a Mac app's toolbar holds them (spec §3.2). `App` hands the box, once it is
 * drawn, to the page below the toolbar; null until then, and wherever no
 * toolbar is.
 */
const ToolbarSlotContext = createContext<HTMLElement | null>(null);

export const ToolbarSlotProvider = ToolbarSlotContext.Provider;

/**
 * A page's actions, drawn in the toolbar (`ToolbarSlotProvider`) while
 * the page is: through a portal, so they are the page's own components --
 * its state, its handlers -- in the toolbar's place. Nothing where there
 * is no toolbar to draw them in.
 */
export function ToolbarItems({ children }: { children: ReactNode }) {
  const slot = useContext(ToolbarSlotContext);
  return slot === null ? null : createPortal(children, slot);
}

/**
 * Whether what is under the toolbar has scrolled from its top: the
 * toolbar's hairline, which a Mac window draws only once there is content
 * under its edge (the scroll edge), and takes away at the top again.
 *
 * Listens in `box` for any of its scrollers -- the page's own box, or a
 * list that scrolls inside it (`VirtualList`) -- as the events pass it on
 * their way down; scroll events do not bubble. Only a scroller that
 * scrolls up and down counts: a row of chips scrolled sideways says
 * nothing about the page's top. A new `page` starts at its top, with no
 * hairline, whatever the last one had.
 */
export function useScrollEdge(box: RefObject<HTMLElement | null>, page: string): boolean {
  const [scrolled, setScrolled] = useState<{ page: string; past: boolean }>({ page, past: false });
  useEffect(() => {
    const element = box.current;
    if (element === null) return;
    const onScroll = (event: Event) => {
      const target = event.target;
      if (!(target instanceof Element) || target.scrollHeight <= target.clientHeight) return;
      const past = target.scrollTop > 0;
      setScrolled((was) => (was.page === page && was.past === past ? was : { page, past }));
    };
    element.addEventListener("scroll", onScroll, { capture: true, passive: true });
    return () => element.removeEventListener("scroll", onScroll, { capture: true });
  }, [box, page]);
  return scrolled.page === page && scrolled.past;
}
