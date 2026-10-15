/**
 * A list's order by a comparison, worked out once for the whole list, so
 * that sorting any part of it again -- a search's matches, a 「显示」
 * choice, another sort that falls back to the name -- compares two
 * numbers rather than two names through `Intl.Collator`. A Mac with a few
 * thousand tools sorts them by name at every key typed and every choice
 * made; this sorts them once per check.
 *
 * Exact: two items `compare` calls equal get the same place, so a sort by
 * the result keeps them in the order it was handed them, as a sort by
 * `compare` itself does. An item that is not in `all` is compared with
 * `compare`.
 */
export function rankedComparator<T extends object>(
  all: readonly T[],
  compare: (a: T, b: T) => number,
): (a: T, b: T) => number {
  const sorted = [...all].sort(compare);
  const rank = new Map<T, number>();
  for (let index = 0; index < sorted.length; index += 1) {
    const item = sorted[index];
    const before = index > 0 ? sorted[index - 1] : undefined;
    rank.set(item, before !== undefined && compare(before, item) === 0 ? (rank.get(before) ?? index) : index);
  }
  return (a, b) => {
    const left = rank.get(a);
    const right = rank.get(b);
    return left !== undefined && right !== undefined ? left - right : compare(a, b);
  };
}

/** The comparators already worked out, by the list they were worked out for and what else they depend on. */
const RANKED = new WeakMap<object, { key: string; compare: (a: never, b: never) => number }>();

/**
 * `rankedComparator(all, compare)`, kept for as long as `all` is the same
 * list and `key` -- what else `compare` reads, such as the language its
 * collator is for -- the same: a page that is shown again after another
 * finds its list's order already worked out.
 */
export function cachedRankedComparator<T extends object>(
  all: readonly T[],
  key: string,
  compare: (a: T, b: T) => number,
): (a: T, b: T) => number {
  const cached = RANKED.get(all);
  if (cached !== undefined && cached.key === key) return cached.compare as (a: T, b: T) => number;
  const ranked = rankedComparator(all, compare);
  RANKED.set(all, { key, compare: ranked as (a: never, b: never) => number });
  return ranked;
}
