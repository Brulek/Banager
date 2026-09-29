import { useCallback, useMemo, useRef, type ReactNode } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";

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
  /** What the list's box shows in place of the list while there are no items. */
  empty?: ReactNode;
}

/** What each slot showed, by its key. */
type Drawn = Map<string, ReactNode>;

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
 */
export function VirtualList<T>({ items, itemKey, estimateSize, renderItem, reusable, empty }: VirtualListProps<T>) {
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

  // 8 in from either side, and a row's own 12 inside that: its content 20
  // from the window's edges, where the toolbar's title and the page above
  // the list start.
  return (
    <div ref={listRef} className="min-h-0 flex-1 overflow-y-auto px-2 pb-4">
      {items.length === 0 && empty !== undefined ? (
        empty
      ) : (
        <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {virtualizer.getVirtualItems().map((virtualRow) => (
            // No fixed height on the slot: each reports its real height
            // back through `measureElement` instead.
            <div
              key={virtualRow.key}
              data-index={virtualRow.index}
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
              {contentOf(items[virtualRow.index], String(virtualRow.key))}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
