/**
 * Which of a list row's columns fit (`ToolRow`), by the width of the list
 * it is drawn in: a module of its own, so that the list (`VirtualList`),
 * which measures that width once for all of its rows, can say the fit to
 * them as well as the width.
 */

/**
 * How much of a row's columns fit in the list's width (spec R9), as the
 * window narrows: `full`, everything; `compact`, the version column says
 * only the version an update brings, after its arrow; `narrow`, that, and
 * the status word moves to the start of the description's line, a dot
 * between them; `minimal` -- a list the Installed page's inspector has
 * narrowed -- the version column goes as well, and an update's versions
 * move to that line after the status word: the inspector says the
 * selected row's version, and the others' are one click away; `slim` --
 * the inspector's list in the narrowest window -- the versions leave that
 * line too, and it says the status word and the description alone: a
 * version is one click away in the inspector, but a word such as
 * 「已关闭提醒」 is what the list is scanned for, and a window made
 * narrower must not hide it; `tiny` -- a list too narrow for the avatar,
 * a name's first few characters, the button and the ⋯, which no window
 * this app opens has -- the row's button goes as well, and only the ⋯
 * stays at its end: what the button did is in the inspector. The status
 * word stays at every width. Past that, the description is cut short --
 * never the name, nor the word -- and, left room for no more than a few
 * characters after the words before it, dropped from the line
 * (`ToolRow`'s `DESCRIPTION_MIN_CHARACTERS`).
 */
export type RowFit = "full" | "compact" | "narrow" | "minimal" | "slim" | "tiny";

/**
 * The widths of the list a row is drawn in -- measured, not the window's
 * -- at which its columns give way (`RowFit`): the list is 752 wide in a
 * window at its default 960, with room for everything, and 592 at its
 * narrowest, 800; beside the inspector, 452 at 960 and 332 at 800 (the
 * inspector 260 there). `slim` from 324: what a row with a button needs --
 * 20 in, the avatar 32, 12, a name's first 96, 16, the button ("Uninstall…"
 * 87 wide; 「卸载…」 60, in its column of 80), 16, the ⋯ 24, and 20 -- is
 * 323 in English and 316 in Chinese, so the narrowest window's 332 keeps
 * every row's button.
 */
export const ROW_FIT_WIDTHS = { full: 700, compact: 640, narrow: 520, minimal: 340, slim: 324 } as const;

/** Which of a row's columns fit a list `width` wide: everything, where nothing measured it. */
export function rowFitFor(width: number | null): RowFit {
  if (width === null || width >= ROW_FIT_WIDTHS.full) return "full";
  if (width >= ROW_FIT_WIDTHS.compact) return "compact";
  if (width >= ROW_FIT_WIDTHS.narrow) return "narrow";
  if (width >= ROW_FIT_WIDTHS.minimal) return "minimal";
  return width >= ROW_FIT_WIDTHS.slim ? "slim" : "tiny";
}
