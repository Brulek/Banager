import { createContext, useContext } from "react";

/**
 * What a row of a list that moves between its rows with ↑ and ↓
 * (`VirtualList`'s `keyboardRows`) needs to take part: a roving tabindex.
 * One row at a time is in the Tab order (`tabIndex` 0) -- the one the
 * focus was last in, the first until then -- and the rest are reached with
 * the arrow keys (-1). `onFocus` makes the row the one Tab comes back to,
 * whatever inside it took the focus: the row itself, its checkbox, its
 * button. The element that takes the focus for the row carries
 * `data-row-focus`, which the list finds it by.
 */
export interface RovingRow {
  tabIndex: 0 | -1;
  onFocus: () => void;
}

const RovingRowContext = createContext<RovingRow | null>(null);

export const RovingRowProvider = RovingRowContext.Provider;

/** This row's part in its list's arrow keys, or null in a list without them. */
export function useRovingRow(): RovingRow | null {
  return useContext(RovingRowContext);
}
