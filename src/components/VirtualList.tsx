import {
  createContext,
  useCallback,
  useContext,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
  type RefObject,
} from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { RovingRowProvider, type RovingRow } from "./rovingRows";
import { rowFitFor, type RowFit } from "./rowFit";

/**
 * How wide the list is that a row is drawn in, in CSS px: what a long
 * name is fitted to (`ToolRow`'s middle cut). Null where nothing has
 * measured it -- a row outside a `VirtualList`, or before the list is laid
 * out -- which a row reads as room enough for everything.
 */
const ListWidthContext = createContext<number | null>(null);

/**
 * Which of a row's columns fit that width (`rowFitFor`): what a row reads
 * to decide which give way in a narrow window. Said apart from the width,
 * so that a new width -- the list laid out as it mounts, a window being
 * resized -- draws the rows again only where it changes what fits.
 */
const RowFitContext = createContext<RowFit>("full");

/**
 * The list width rows are drawn at, and so what fits on them: a
 * `VirtualList`'s own, and that of a list that is not one -- a test's, or
 * the Unknown page's.
 */
export function ListWidthProvider({ value, children }: { value: number | null; children: ReactNode }) {
  return (
    <ListWidthContext.Provider value={value}>
      <RowFitContext.Provider value={rowFitFor(value)}>{children}</RowFitContext.Provider>
    </ListWidthContext.Provider>
  );
}

/**
 * Whether the rows of the list keep a status word's column (`ToolRow`):
 * where any row the list shows has a word, every row keeps the column,
 * empty or not, so the words line up down the list; where none has, the
 * rows give its 120 to their names and descriptions. Said by the list's
 * page, which knows every row's word -- a virtualized list draws only
 * the rows in sight. A list that says nothing keeps the column.
 */
const StatusColumnContext = createContext(true);

/** Whether the rows under it keep a status word's column (`StatusColumnContext`). */
export function StatusColumnProvider({ value, children }: { value: boolean; children: ReactNode }) {
  return <StatusColumnContext.Provider value={value}>{children}</StatusColumnContext.Provider>;
}

/** Whether the list this is drawn in keeps a status word's column (`StatusColumnContext`). */
export function useStatusColumn(): boolean {
  return useContext(StatusColumnContext);
}

/** The width of the list this is drawn in (`ListWidthContext`). */
export function useListWidth(): number | null {
  return useContext(ListWidthContext);
}

/** Which of a row's columns fit the list this is drawn in (`RowFitContext`). */
export function useRowFit(): RowFit {
  return useContext(RowFitContext);
}

/**
 * `node`'s width, kept up to date as the window is resized; null until it
 * has one (jsdom lays nothing out, so there it stays null), and measured
 * again whenever the element is a new one -- a page's box that mounts
 * only once its data is in, handed over through a callback ref.
 */
export function useElementWidth(node: HTMLElement | null): number | null {
  const [width, setWidth] = useState<number | null>(null);
  useLayoutEffect(() => {
    if (node === null) return;
    const settle = (next: number) => setWidth(next > 0 ? Math.round(next) : null);
    settle(node.getBoundingClientRect().width);
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) settle(entry.contentRect.width);
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, [node]);
  return width;
}

/**
 * Whether the element handed to the returned ref is narrower than
 * `limit`, kept up to date as the window is resized; false until it is
 * measured (jsdom lays nothing out). A state that changes only when the
 * answer does: where `useElementWidth` draws its user again as the
 * element is handed over and again for its width -- a page, and every row
 * of its list in sight with it, twice as it opens -- this draws nothing
 * again for a width on the same side of `limit`.
 */
export function useNarrowerThan(limit: number): [(node: HTMLElement | null) => (() => void) | undefined, boolean] {
  const [narrower, setNarrower] = useState(false);
  const known = useRef(false);
  const attach = useCallback(
    (node: HTMLElement | null) => {
      if (node === null) return undefined;
      const settle = (width: number) => {
        const next = width > 0 && width < limit;
        if (next === known.current) return;
        known.current = next;
        setNarrower(next);
      };
      settle(node.getBoundingClientRect().width);
      const observer = new ResizeObserver((entries) => {
        for (const entry of entries) settle(entry.contentRect.width);
      });
      observer.observe(node);
      return () => observer.disconnect();
    },
    [limit],
  );
  return [attach, narrower];
}

export interface VirtualListProps<T> {
  /** The slots, in order. */
  items: readonly T[];
  /**
   * A slot's identity: its React key, and the key the virtualizer files
   * its measured height under, so that a height stays with the slot it was
   * measured from when a slot above it goes (the Updates page's
   * `listItemKey` has the story). A function of the item alone, the same
   * one from one render to the next.
   */
  itemKey: (item: T) => string;
  /** The first guess at a slot's height, before it measures itself. */
  estimateSize: (item: T) => number;
  /** What a slot shows. */
  renderItem: (item: T) => ReactNode;
  /**
   * Whether a slot, once drawn, may be shown again as it was while `items`
   * and `renderItem` are still the ones it was drawn from -- every slot,
   * unless said. Not one whose content reads anything else as it is
   * drawn, such as the clock.
   */
  reusable?: (item: T) => boolean;
  /**
   * The slots ↑ and ↓ move the focus between (spec R11), as in a Mac
   * list: a tool's row, and a line that discloses more of them. Each takes
   * part through `useRovingRow` (./rovingRows.ts). Left out, the arrow
   * keys do nothing here.
   */
  keyboardRows?: (item: T) => boolean;
  /**
   * Whether a line's hairline (`data-row-separator`: a row's, the notices
   * line's) shows over `next`, the item after it: a Mac list parts a row
   * from the next row and from nothing else -- no hairline over a heading
   * or a disclosure, nor over the selected row, whose fill is its own
   * edge. A slot with none under it -- this says no, or it holds the
   * list's last item -- is marked `data-run-end`, which index.css hides
   * the hairline in. Left out, every slot but the last keeps its hairline.
   *
   * Said here, where the items are, rather than read off the next slot by
   * the stylesheet (`:has(+ …)`): a rule about a slot's next sibling has
   * the browser restyle every slot in sight at every step of a scroll.
   */
  hairlineBefore?: (next: T) => boolean;
  /** What the list's box shows in place of the list while there are no items. */
  empty?: ReactNode;
  /**
   * Called with the slot ↑ or ↓ has just moved the focus to: the
   * Installed page's selection follows the keyboard, as a Mac list's does.
   */
  onKeyboardMove?: (item: T) => void;
  /** Handed a way to put the focus back on a slot by its key (`VirtualListHandle`). */
  handleRef?: RefObject<VirtualListHandle | null>;
  /**
   * The slot to hold still when `items` changes -- the selected row: a
   * check that adds or takes away slots above it moves the list by as
   * much, so the row stays where it was on screen (spec R11), rather than
   * the rows sliding under a still scrollbar.
   */
  anchorKey?: string | null;
  /**
   * Whether any row among `items` has a status word, and so whether the
   * rows keep that word's column (`StatusColumnContext`): kept, unless
   * said.
   */
  statusColumn?: boolean;
}

/** What a list's owner can ask of it (`handleRef`). */
export interface VirtualListHandle {
  /**
   * Puts the focus on the slot `key` names -- its row's own focus
   * (`data-row-focus`), drawn first and scrolled into sight if it is not
   * -- as Escape hands it back from the inspector to its row.
   */
  focusKey(key: string): void;
}

/** What each slot showed, by its key. */
type Drawn = Map<string, ReactNode>;

/** Where ↑ or ↓ goes: the next slot either way that takes part in the arrow keys, or null past the end. */
function nextKeyboardRow<T>(
  items: readonly T[],
  from: number,
  step: 1 | -1,
  keyboardRows: (item: T) => boolean,
): number | null {
  for (let index = from + step; index >= 0 && index < items.length; index += step) {
    if (keyboardRows(items[index])) return index;
  }
  return null;
}

/**
 * The Installed and Updates pages' list: a box that scrolls, with only
 * the slots in sight (and one either side) in the DOM, each measuring its
 * own height (`measureElement`) -- a Mac that has used Homebrew for a
 * while lists 300 to 800 tools.
 *
 * The virtualizer draws the list again at every step of a scroll, and it
 * is this component that it draws again, not the page around it: the
 * header, the filters and the notices stay as they are. Nor are the slots
 * already in sight drawn again: each is shown as it was last drawn
 * (`reusable`), while the page hands the same `items` and the same
 * `renderItem` -- a page that draws again, because anything it shows has
 * changed, hands a new `renderItem` and every slot is drawn afresh from
 * it. A scroll draws only the slots that come into sight.
 *
 * With `keyboardRows`, ↑ and ↓ move the focus from row to row, scrolling
 * the next one into sight -- drawing it first, if it was out of the DOM --
 * and a roving tabindex keeps one row in the Tab order: the one last
 * focused. Not while a menu or a field inside the list has the keys: a
 * menu's arrows are its own (it marks them handled), and so are a text
 * field's.
 */
export function VirtualList<T>({
  items,
  itemKey,
  estimateSize,
  renderItem,
  reusable,
  keyboardRows,
  hairlineBefore,
  empty,
  onKeyboardMove,
  handleRef,
  anchorKey = null,
  statusColumn = true,
}: VirtualListProps<T>) {
  const listRef = useRef<HTMLDivElement>(null);
  // Changes with `items`, which is what tells the virtualizer to lay the
  // list out again from its measured heights under the new keys.
  const getItemKey = useCallback((index: number) => itemKey(items[index]), [items, itemKey]);
  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => listRef.current,
    estimateSize: (index) => estimateSize(items[index]),
    getItemKey,
  });

  // What each slot showed, by its key, drawn from these `items` and this
  // `renderItem`; a new list of either starts it over. The same element,
  // handed back, is one React does not draw again.
  const drawn = useMemo((): Drawn => new Map(), [items, renderItem]);
  const contentOf = (item: T, key: string): ReactNode => {
    if (reusable !== undefined && !reusable(item)) return renderItem(item);
    if (!drawn.has(key)) drawn.set(key, renderItem(item));
    return drawn.get(key);
  };

  // The box, as state too: what its width is measured from.
  const [listBox, setListBox] = useState<HTMLDivElement | null>(null);
  const attachList = useCallback((node: HTMLDivElement | null) => {
    listRef.current = node;
    setListBox(node);
  }, []);
  const width = useElementWidth(listBox);

  // The row in the Tab order: the one last focused, while it is listed,
  // and the first row until then.
  const [activeKey, setActiveKey] = useState<string | null>(null);
  const firstRow = keyboardRows === undefined ? -1 : items.findIndex(keyboardRows);
  const activeIndex =
    keyboardRows === undefined
      ? -1
      : (() => {
          const at = activeKey === null ? -1 : items.findIndex((item) => itemKey(item) === activeKey);
          return at >= 0 && keyboardRows(items[at]) ? at : firstRow;
        })();
  const active = activeIndex >= 0 ? itemKey(items[activeIndex]) : null;
  // One value per row, the same object from one draw to the next while the
  // row in the Tab order is the same: a new one would draw every row in
  // sight again at every step of a scroll.
  const roving = useMemo((): Map<string, RovingRow> => new Map(), [active]);
  const rovingOf = (key: string): RovingRow => {
    let value = roving.get(key);
    if (value === undefined) {
      value = { tabIndex: key === active ? 0 : -1, onFocus: () => setActiveKey(key) };
      roving.set(key, value);
    }
    return value;
  };

  // The row ↑ or ↓ moved to, to focus once it is drawn.
  const [pendingFocus, setPendingFocus] = useState<string | null>(null);
  useLayoutEffect(() => {
    if (pendingFocus === null) return;
    const slot = [...(listRef.current?.querySelectorAll<HTMLElement>("[data-list-slot]") ?? [])].find(
      (element) => element.dataset.key === pendingFocus,
    );
    const target = slot?.querySelector<HTMLElement>("[data-row-focus]");
    if (target === null || target === undefined) return;
    target.focus();
    setPendingFocus(null);
  });

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (keyboardRows === undefined || event.defaultPrevented) return;
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    const target = event.target as HTMLElement;
    if (target.closest('[role="menu"], input:not([type="checkbox"]), textarea') !== null) return;
    const slot = target.closest<HTMLElement>("[data-list-slot]");
    if (slot === null) return;
    const next = nextKeyboardRow(items, Number(slot.dataset.index), event.key === "ArrowDown" ? 1 : -1, keyboardRows);
    event.preventDefault();
    if (next === null) return;
    const key = itemKey(items[next]);
    setActiveKey(key);
    setPendingFocus(key);
    virtualizer.scrollToIndex(next, { align: "auto" });
    onKeyboardMove?.(items[next]);
  };

  // Escape's way back from the inspector to its row (`handleRef`).
  const focusKey = useCallback(
    (key: string) => {
      const index = items.findIndex((item) => itemKey(item) === key);
      if (index < 0) return;
      setActiveKey(key);
      setPendingFocus(key);
      virtualizer.scrollToIndex(index, { align: "auto" });
    },
    [items, itemKey, virtualizer],
  );
  useLayoutEffect(() => {
    if (handleRef === undefined) return;
    handleRef.current = { focusKey };
    return () => {
      if (handleRef.current?.focusKey === focusKey) handleRef.current = null;
    };
  }, [handleRef, focusKey]);

  // Where the anchor started when the list was last laid out; a new list
  // of items that moves it moves the scroll position by as much.
  const anchorAt = useRef<{ key: string; start: number } | null>(null);
  useLayoutEffect(() => {
    const index = anchorKey === null ? -1 : items.findIndex((item) => itemKey(item) === anchorKey);
    const measured = index < 0 ? undefined : virtualizer.measurementsCache[index];
    const was = anchorAt.current;
    anchorAt.current = anchorKey === null || measured === undefined ? null : { key: anchorKey, start: measured.start };
    const scroller = listRef.current;
    if (was === null || measured === undefined || was.key !== anchorKey || scroller === null) return;
    if (measured.start !== was.start) scroller.scrollTop += measured.start - was.start;
    // Only a new list moves it: a new anchor is taken where it is.
  }, [items, anchorKey]);

  // Edge to edge: a row keeps its own 20 in from either side, where the
  // toolbar's title and the page above the list start, and its hairline
  // ends 20 from the right.
  return (
    <div ref={attachList} data-list="" onKeyDown={onKeyDown} className="min-h-0 flex-1 overflow-y-auto pb-4">
      {items.length === 0 && empty !== undefined ? (
        empty
      ) : (
        <ListWidthProvider value={width}>
          <StatusColumnProvider value={statusColumn}>
            <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
              {virtualizer.getVirtualItems().map((virtualRow) => {
                const item = items[virtualRow.index];
                const key = String(virtualRow.key);
                const content = contentOf(item, key);
                const next = items[virtualRow.index + 1];
                const runEnd = next === undefined || (hairlineBefore !== undefined && !hairlineBefore(next));
                return (
                  // No fixed height on the slot: each reports its real height
                  // back through `measureElement` instead.
                  <div
                    key={virtualRow.key}
                    data-index={virtualRow.index}
                    data-key={key}
                    data-list-slot=""
                    data-run-end={runEnd ? "" : undefined}
                    ref={virtualizer.measureElement}
                    style={{
                      position: "absolute",
                      top: 0,
                      left: 0,
                      width: "100%",
                      transform: `translateY(${virtualRow.start}px)`,
                    }}
                  >
                    {keyboardRows !== undefined && keyboardRows(item) ? (
                      <RovingRowProvider value={rovingOf(key)}>{content}</RovingRowProvider>
                    ) : (
                      content
                    )}
                  </div>
                );
              })}
            </div>
          </StatusColumnProvider>
        </ListWidthProvider>
      )}
    </div>
  );
}
