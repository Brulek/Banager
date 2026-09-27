import { useCallback, useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";

/**
 * How far in from an edge a fade reaches, and how far in from it a chip
 * is scrolled to (`scroll-px-8`, the same 2rem): a chip brought into view
 * lands clear of the fade, not half under it.
 */
const FADE = "2rem";

/** Whether chips are hidden before the row's left edge, and after its right. */
interface Hidden {
  before: boolean;
  after: boolean;
}

function hiddenIn(row: HTMLElement): Hidden {
  const end = row.scrollWidth - row.clientWidth;
  // A pixel either way: a row scrolled to a fraction of one is at its end.
  return { before: row.scrollLeft > 1, after: row.scrollLeft < end - 1 };
}

/** The soft edge on each side that has chips hidden past it, as a mask: it fades the chips, whatever is behind them. */
function fadeFor({ before, after }: Hidden): string | undefined {
  if (before && after) {
    return `linear-gradient(to right, transparent, #000 ${FADE}, #000 calc(100% - ${FADE}), transparent)`;
  }
  if (after) return `linear-gradient(to right, #000 calc(100% - ${FADE}), transparent)`;
  if (before) return `linear-gradient(to right, transparent, #000 ${FADE})`;
  return undefined;
}

export interface ChipRowProps {
  /** The group's accessible name: 「按来源筛选」, "Filter by source". */
  label: string;
  /** The chips: buttons, one per choice, whichever is chosen `aria-pressed`. */
  children: ReactNode;
}

/**
 * A row of chips that stays one line however many there are -- the
 * Installed page's filters, at the window's 800px -- and scrolls sideways
 * when they do not fit, with no scrollbar and a soft fade at an edge that
 * has chips hidden past it (`data-more-before`, `data-more-after`).
 *
 * Every chip stays a button in the tab order, and one that takes the
 * focus is scrolled into view, clear of the fade; so is the chosen one
 * whenever the choice changes -- a filter chosen on the Overview, a chip
 * pressed half under the fade, since a click in WebKit does not focus a
 * button. Over a row that overflows, a mouse's up-and-down wheel moves
 * it sideways, as a trackpad does.
 */
export function ChipRow({ label, children }: ChipRowProps) {
  const rowRef = useRef<HTMLDivElement>(null);
  const [hidden, setHidden] = useState<Hidden>({ before: false, after: false });
  const measure = useCallback(() => {
    const row = rowRef.current;
    if (!row) return;
    const next = hiddenIn(row);
    setHidden((last) => (last.before === next.before && last.after === next.after ? last : next));
  }, []);

  // After every render: a chip's count or name can change its width, and
  // the chosen chip can change.
  const chosenRef = useRef<Element | null>(null);
  useLayoutEffect(() => {
    const row = rowRef.current;
    if (!row) return;
    const chosen = row.querySelector('[aria-pressed="true"]');
    if (chosen !== chosenRef.current) {
      chosenRef.current = chosen;
      chosen?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
    }
    measure();
  });

  useEffect(() => {
    const row = rowRef.current;
    if (!row) return;
    const resize = new ResizeObserver(measure);
    resize.observe(row);
    const wheel = (event: WheelEvent) => {
      if (row.scrollWidth <= row.clientWidth || Math.abs(event.deltaY) <= Math.abs(event.deltaX)) return;
      row.scrollLeft += event.deltaY;
      event.preventDefault();
    };
    // Not passive: the wheel moves the row instead of whatever is under it.
    row.addEventListener("wheel", wheel, { passive: false });
    return () => {
      resize.disconnect();
      row.removeEventListener("wheel", wheel);
    };
  }, [measure]);

  const fade = fadeFor(hidden);
  return (
    <div
      ref={rowRef}
      role="group"
      aria-label={label}
      data-more-before={hidden.before ? "" : undefined}
      data-more-after={hidden.after ? "" : undefined}
      onScroll={measure}
      onFocus={(event) => {
        if (event.target !== event.currentTarget) {
          event.target.scrollIntoView?.({ block: "nearest", inline: "nearest" });
        }
      }}
      style={fade === undefined ? undefined : { maskImage: fade, WebkitMaskImage: fade }}
      // Room inside the row, which clips whatever overflows it, for a
      // chip's focus ring: above and below the chips, and at either end.
      className="-mx-1 -my-1 flex flex-nowrap gap-1 overflow-x-auto scroll-px-8 px-1 py-1 [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
    >
      {children}
    </div>
  );
}
