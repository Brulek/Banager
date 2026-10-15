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
 * A group's container: the group fill, corners of 10, no edge. Everything
 * in it starts 10 in from its left, the one inset of a group. Between each
 * two of its rows a hairline in the group's separator colour, 10 in from
 * either side -- drawn over the top of every row but the first, so the rows
 * can be any element: a `<div>`, or the `<li>`s of a `<ul>` that is itself
 * the container.
 */
export const GROUP =
  "rounded-group bg-group [&>*+*]:relative [&>*+*]:before:absolute [&>*+*]:before:left-2.5 [&>*+*]:before:right-2.5 [&>*+*]:before:top-0 [&>*+*]:before:h-px [&>*+*]:before:bg-group-separator";

/**
 * A group whose rows start with a 16 symbol, 8 before their words (the
 * Overview's problems): its hairlines start where the words do, 34 in, as
 * a Mac list's separators start at the text rather than the icon.
 */
export const GROUP_WITH_ICONS =
  "rounded-group bg-group [&>*+*]:relative [&>*+*]:before:absolute [&>*+*]:before:left-8.5 [&>*+*]:before:right-2.5 [&>*+*]:before:top-0 [&>*+*]:before:h-px [&>*+*]:before:bg-group-separator";

/**
 * 11pt text that may run to a second line -- a row's subtitle, a notice's
 * explanation, a footnote: Chinese wrapped at 11/14 is cramped, so its
 * lines are 16 apart; a line box 1 short at either end keeps one line of
 * it 14 high, as the rest of the 11pt text is, so a row with it is still
 * 46.
 */
export const SMALL_WRAPPING = "text-small leading-4 -my-px";

/** A group's title: 13/16 bold, over the rows' text (10 in), 8 above the container. */
export const GROUP_TITLE = "mb-2 px-2.5 text-title text-foreground";

/**
 * What a group says as a whole, under it: 11 muted, 6 below the container,
 * in line with the rows' text; its lines 16 apart when it wraps.
 */
export const GROUP_FOOTNOTE = "mt-1.5 px-2.5 text-small leading-4 text-muted";

/**
 * One row of a group, on one line: 36 high, 10 in on either side, its words
 * on the left and its control on the right -- a switch 16 high, a popup 18,
 * a regular button 24 -- centred in it.
 */
export const GROUP_ROW = "flex min-h-9 items-center justify-between gap-4 px-2.5 py-1.5";

/** A row with a second line under its label (11/14 muted): 46 high. */
export const GROUP_ROW_TWO_LINES = "flex min-h-11.5 items-center justify-between gap-4 px-2.5 py-2";
