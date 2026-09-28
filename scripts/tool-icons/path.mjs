/**
 * Rounding a Simple Icons glyph's path data to fewer decimals, for
 * build.mjs. Not a regular expression over the numbers: minified path data
 * writes an arc's two flags with no separator (`a1 1 0 01.5.5` is flags 0
 * and 1, then .5 and .5), which a number-by-number rewrite would read as
 * `01.5` and turn into a different shape. So the data is parsed into
 * segments, rounded, and written back.
 *
 * Relative commands are rounded against where the *rounded* path already
 * is, so the error does not pile up along a long relative path: every
 * point the data writes stays within half a unit of the last decimal of
 * where it was. build.mjs still renders each glyph before and after and
 * keeps the original when they differ visibly.
 */

/** How many numbers each command takes. */
const ARITY = { m: 2, l: 2, h: 1, v: 1, c: 6, s: 4, q: 4, t: 2, a: 7, z: 0 };

const NUMBER = /[+-]?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?/y;
const SEPARATORS = /[\s,]*/y;

/**
 * `d` as a list of segments, `[command, ...numbers]`, one per command
 * including the implicit ones (a moveto's further pairs are linetos).
 * Throws on anything that is not path data.
 */
export function parsePath(d) {
  const segments = [];
  let i = 0;
  let command = null;
  const skip = () => {
    SEPARATORS.lastIndex = i;
    SEPARATORS.exec(d);
    i = SEPARATORS.lastIndex;
  };
  const number = () => {
    NUMBER.lastIndex = i;
    const match = NUMBER.exec(d);
    if (match === null) throw new Error(`not a number at ${i}: "${d.slice(i, i + 16)}"`);
    i = NUMBER.lastIndex;
    return Number(match[0]);
  };
  // An arc flag is one character, "0" or "1", with or without a separator after it.
  const flag = () => {
    const c = d[i];
    if (c !== "0" && c !== "1") throw new Error(`not an arc flag at ${i}: "${d.slice(i, i + 16)}"`);
    i += 1;
    return Number(c);
  };

  skip();
  while (i < d.length) {
    const c = d[i];
    if (/[A-Za-z]/.test(c)) {
      if (!Object.prototype.hasOwnProperty.call(ARITY, c.toLowerCase())) {
        throw new Error(`unknown path command "${c}" at ${i}`);
      }
      command = c;
      i += 1;
    } else if (command === null || command.toLowerCase() === "z") {
      throw new Error(`numbers without a command at ${i}: "${d.slice(i, i + 16)}"`);
    }
    const lower = command.toLowerCase();
    const segment = [command];
    for (let k = 0; k < ARITY[lower]; k++) {
      skip();
      segment.push(lower === "a" && (k === 3 || k === 4) ? flag() : number());
    }
    segments.push(segment);
    if (command === "M") command = "L";
    else if (command === "m") command = "l";
    skip();
  }
  return segments;
}

/**
 * `segments` with every number rounded to `digits` decimals, except an
 * arc's. `err` is how far the rounded path's current point is from the
 * original's; a relative command's coordinates are measured from the
 * rounded point, so it is taken off them before they are rounded.
 *
 * An arc is kept exactly as drawn -- its radii and its chord, the step
 * from where it starts to where it ends -- and only moved along with the
 * point it starts from. Its shape turns on how its radius compares with
 * half its chord: a circle drawn as two half circles has them equal, and
 * a radius 0.005 longer than half the chord moves the centre a quarter of
 * a unit off the chord's middle (Simple Icons' FusionAuth, Cloudsmith),
 * while a near-full circle whose chord rounds to nothing is not drawn at
 * all (Trino). An absolute arc is written as a relative one, whose chord
 * is exact. Such a segment is marked `exact` for `serializePath`.
 */
export function roundSegments(segments, digits) {
  const round = (value) => Number(value.toFixed(digits));
  let errX = 0;
  let errY = 0;
  let startErrX = 0;
  let startErrY = 0;
  // Where the original path is: the current point and its subpath's start.
  let x = 0;
  let y = 0;
  let startX = 0;
  let startY = 0;
  return segments.map(([command, ...p]) => {
    const lower = command.toLowerCase();
    const relative = command === lower;
    const dx = relative ? -errX : 0;
    const dy = relative ? -errY : 0;
    // An end point: rounded, and the error it leaves behind remembered.
    const endX = (value) => {
      const exact = value + dx;
      const rounded = round(exact);
      errX = rounded - exact;
      x = relative ? x + value : value;
      return rounded;
    };
    const endY = (value) => {
      const exact = value + dy;
      const rounded = round(exact);
      errY = rounded - exact;
      y = relative ? y + value : value;
      return rounded;
    };
    switch (lower) {
      case "z":
        errX = startErrX;
        errY = startErrY;
        x = startX;
        y = startY;
        return [command];
      case "h":
        return [command, endX(p[0])];
      case "v":
        return [command, endY(p[0])];
      case "a": {
        const chordX = relative ? p[5] : p[5] - x;
        const chordY = relative ? p[6] : p[6] - y;
        x += chordX;
        y += chordY;
        // The error is carried over unchanged: the arc moves with its start.
        return Object.assign(["a", p[0], p[1], p[2], p[3], p[4], chordX, chordY], { exact: true });
      }
      default: {
        // Control points, then the end point: pairs of x and y.
        const out = [command];
        for (let k = 0; k < p.length - 2; k += 2) {
          out.push(round(p[k] + dx), round(p[k + 1] + dy));
        }
        out.push(endX(p[p.length - 2]), endY(p[p.length - 1]));
        if (lower === "m") {
          startErrX = errX;
          startErrY = errY;
          startX = x;
          startY = y;
        }
        return out;
      }
    }
  });
}

/** `value` in as few characters as path data allows: no trailing zeros, no leading `0.`. */
function formatNumber(value, digits) {
  let text = value.toFixed(digits);
  if (text.includes(".")) text = text.replace(/0+$/, "").replace(/\.$/, "");
  if (text === "-0") text = "0";
  return text.replace(/^(-?)0\./, "$1.");
}

/** Decimals enough to write an `exact` segment's numbers as they were. */
const EXACT_DIGITS = 6;

/**
 * `segments` written back as path data: a command's letter only when it
 * changes (a lineto after a moveto needs none), and a space only where
 * two numbers would otherwise run together. An arc flag always gets its
 * spaces, so no reader can take it for part of a number.
 */
export function serializePath(segments, digits) {
  let out = "";
  let previous = null;
  // What was written last: a command letter, a number (and its text) or a flag.
  let lastKind = null;
  let lastText = "";
  for (const segment of segments) {
    const [command, ...params] = segment;
    const places = segment.exact === true ? EXACT_DIGITS : digits;
    const implicit =
      params.length > 0 &&
      ((command === previous && command !== "M" && command !== "m") ||
        (previous === "M" && command === "L") ||
        (previous === "m" && command === "l"));
    if (!implicit) {
      out += command;
      lastKind = "command";
    }
    params.forEach((value, k) => {
      const isFlag = command.toLowerCase() === "a" && (k === 3 || k === 4);
      const text = isFlag ? String(value) : formatNumber(value, places);
      const joins =
        lastKind === "command" ||
        (!isFlag &&
          lastKind === "number" &&
          (text.startsWith("-") || (text.startsWith(".") && lastText.includes("."))));
      out += (joins ? "" : " ") + text;
      lastKind = isFlag ? "flag" : "number";
      lastText = text;
    });
    previous = command;
  }
  return out;
}

/** `d` with every number rounded to `digits` decimals. */
export function roundPath(d, digits) {
  return serializePath(roundSegments(parsePath(d), digits), digits);
}
