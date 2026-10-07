/**
 * A text's width in CSS px, in the font it is drawn in: what `middleCut`
 * fits a name to.
 */
export type MeasureText = (text: string) => number;

/** The mark set where a cut name's middle was. */
export const ELLIPSIS = "…";

/**
 * `text` cut short in its middle to fit `room` px, as Finder cuts a long
 * file name: its start, "…", and its last `tailLength` characters, as one
 * string -- so the "…" stands right against both halves, with no gap
 * where a box cut at its end would leave one -- the start as long as
 * still fits. The whole text where it fits; where not even its first
 * character does beside the tail, that character, "…" and the tail, for
 * the box it is drawn in to cut at its end.
 */
export function middleCut(text: string, room: number, measure: MeasureText, tailLength: number): string {
  if (measure(text) <= room || text.length <= tailLength + 1) return text;
  const tail = text.slice(-tailLength);
  const withHead = (length: number) => `${text.slice(0, length)}${ELLIPSIS}${tail}`;
  // The longest start that fits, found by halving: widths only grow as
  // the start does.
  let fits = 1;
  let tooLong = text.length - tailLength;
  if (measure(withHead(fits)) > room) return withHead(fits);
  while (tooLong - fits > 1) {
    const middle = Math.floor((fits + tooLong) / 2);
    if (measure(withHead(middle)) <= room) fits = middle;
    else tooLong = middle;
  }
  return withHead(fits);
}

/**
 * `text`'s end alone, after "…", the longest that fits `room` px: for words
 * whose end is what tells them apart -- a source's place,
 * 「…python3.11）」 -- in a room too small for `middleCut`'s first
 * character, "…" and tail. The whole text where it fits; nothing at all
 * where not even "…" and one character do, rather than a lone "…".
 */
export function endCut(text: string, room: number, measure: MeasureText): string {
  if (measure(text) <= room) return text;
  const withTail = (length: number) => `${ELLIPSIS}${text.slice(text.length - length)}`;
  if (text.length === 0 || measure(withTail(1)) > room) return "";
  // The longest end that fits, found by halving: widths only grow as the
  // end does.
  let fits = 1;
  let tooLong = text.length;
  while (tooLong - fits > 1) {
    const middle = Math.floor((fits + tooLong) / 2);
    if (measure(withTail(middle)) <= room) fits = middle;
    else tooLong = middle;
  }
  return withTail(fits);
}

/** One canvas for every measurement: drawing into it is never shown. */
let canvas: HTMLCanvasElement | null = null;

/**
 * Measures text as `element` draws it -- its weight, size and family --
 * through a canvas's `measureText`; null where there is no canvas to
 * measure with (jsdom), where a caller leaves the text whole for its box
 * to cut.
 */
export function textMeasurer(element: HTMLElement): MeasureText | null {
  canvas ??= document.createElement("canvas");
  let context: CanvasRenderingContext2D | null = null;
  try {
    context = canvas.getContext("2d");
  } catch {
    return null;
  }
  if (context === null) return null;
  const style = getComputedStyle(element);
  context.font = `${style.fontStyle} ${style.fontWeight} ${style.fontSize} ${style.fontFamily}`;
  const drawing = context;
  return (text) => drawing.measureText(text).width;
}
