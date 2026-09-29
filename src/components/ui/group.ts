/**
 * A grouped form, as System Settings draws one (measured on macOS 27:
 * native-sui-settings; spec §3.7 and §2.6): the Settings page and the
 * Overview are made of these.
 *
 * Whole class strings, each a literal, so Tailwind finds every class here.
 */

/**
 * The column the groups stand in: as wide as 560, or as the page less 20
 * on each side where that is narrower, centred, 20 under the toolbar, and
 * its groups 24 apart.
 */
export const FORM_COLUMN = "mx-auto flex w-[min(560px,calc(100%-40px))] flex-col gap-6 pb-8 pt-5";

/**
 * A group's container: the group fill, corners of 10, no edge. Between each
 * two of its rows a hairline in the group's separator colour, 10 in from
 * either side -- drawn over the top of every row but the first, so the rows
 * can be any element: a `<div>`, or the `<li>`s of a `<ul>` that is itself
 * the container.
 */
export const GROUP =
  "rounded-group bg-group [&>*+*]:relative [&>*+*]:before:absolute [&>*+*]:before:inset-x-2.5 [&>*+*]:before:top-0 [&>*+*]:before:h-px [&>*+*]:before:bg-group-separator";

/** A group's title: 13/16 bold, over the rows' text (10 in), 8 above the container. */
export const GROUP_TITLE = "mb-2 px-2.5 text-title text-foreground";

/** What a group says as a whole, under it: 11/14 muted, 6 below the container, in line with the rows' text. */
export const GROUP_FOOTNOTE = "mt-1.5 px-2.5 text-small text-muted";

/**
 * One row of a group, on one line: 36 high, 10 in on either side, its words
 * on the left and its control on the right -- a switch 16 high, a popup 18,
 * a regular button 24 -- centred in it.
 */
export const GROUP_ROW = "flex min-h-9 items-center justify-between gap-4 px-2.5 py-1.5";

/** A row with a second line under its label (11/14 muted): 46 high. */
export const GROUP_ROW_TWO_LINES = "flex min-h-11.5 items-center justify-between gap-4 px-2.5 py-2";
