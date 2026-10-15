import { useEffect, useState } from "react";

/**
 * How long the search field's text has to stay the same before the
 * Installed page takes it as what the user searched for, not a step on the
 * way to it (`useSettled`): long enough for a typo and its Backspace, short
 * enough that a search that hides the selected tool closes its details
 * soon after the typing stops.
 */
export const SEARCH_SETTLE_MS = 800;

/**
 * `value` once it has stayed the same for `delayMs`; until then, the value
 * that last did. Starts as `value`, so a page opened with a search already
 * in its field takes that search as settled.
 */
export function useSettled<T>(value: T, delayMs: number): T {
  const [settled, setSettled] = useState(value);
  useEffect(() => {
    if (Object.is(settled, value)) return;
    const timer = setTimeout(() => setSettled(value), delayMs);
    return () => clearTimeout(timer);
  }, [value, settled, delayMs]);
  return settled;
}
