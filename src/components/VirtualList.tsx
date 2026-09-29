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

/**
 * How wide the list is that a row is drawn in, in CSS px: what a row
 * reads to decide which of its columns give way in a narrow window
 * (`ToolRow`'s `rowFitFor`). Null where nothing has measured it -- a row
 * outside a `VirtualList`, or before the list is laid out -- which a row
 * reads as room enough for everything.
 */
const ListWidthContext = createContext<number | null>(null);

/** The list width rows are drawn at, for a list that is not a `VirtualList` -- a test's, or the Unknown page's. */
export const ListWidthProvider = ListWidthContext.Provider;

/** The width of the list this is drawn in (`ListWidthContext`). */
export function useListWidth(): number | null {
  return useContext(ListWidthContext);
}

/**
 * `element`'s width, kept up to date as the window is resized; null until
 * it has one (jsdom lays nothing out, so there it stays null).
 */
function useWidthOf(element: RefObject<HTMLElement | null>): number | null {
  const [width, setWidth] = useState<number | null>(null);
  useLayoutEffect(() => {
    const node = element.current;
    if (node === null) return;
    const settle = (next: number) => setWidth(next > 0 ? Math.round(next) : null);
    settle(node.getBoundingClientRect().width);
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) settle(entry.contentRect.width);
    });
    observer.observe(node);
    return () => observer.disconnect();
  }, [element]);
  return width;
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
  /** What the list's box shows in place of the list while there are no items. */
  empty?: ReactNode;
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
  empty,
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

  const width = useWidthOf(listRef);

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
  };

  // Edge to edge: a row keeps its own 20 in from either side, where the
  // toolbar's title and the page above the list start, and its hairline
  // ends 20 from the right.
  return (
    <div ref={listRef} onKeyDown={onKeyDown} className="min-h-0 flex-1 overflow-y-auto pb-4">
      {items.length === 0 && empty !== undefined ? (
        empty
      ) : (
        <ListWidthContext.Provider value={width}>
          <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
            {virtualizer.getVirtualItems().map((virtualRow) => {
              const item = items[virtualRow.index];
              const key = String(virtualRow.key);
              const content = contentOf(item, key);
              return (
                // No fixed height on the slot: each reports its real height
                // back through `measureElement` instead.
                <div
                  key={virtualRow.key}
                  data-index={virtualRow.index}
                  data-key={key}
                  data-list-slot=""
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
        </ListWidthContext.Provider>
      )}
    </div>
  );
}
