/**
 * The colour of the words on the accent -- a default button such as
 * 「全部更新」, a menu's highlighted item, the focused list's selection --
 * worked out from the accent the user picked in System Settings (decision
 * I21b). index.css draws them white (`--color-accent-foreground`), as
 * macOS does; on a light accent -- yellow, green, orange, graphite, pink --
 * white reads at 1.5:1 to 3.4:1, and the words are black there instead,
 * which reads at 6:1 or more.
 *
 * Where it keeps white: wherever white reads at least as well as on the
 * Mac's own blue (#007AFF, 4.0:1; an older macOS's dark #0A84FF, 3.6:1),
 * which decision I21a keeps as Finder draws it -- `KEEP_WHITE`. Under
 * Increase Contrast, whose accent index.css darkens by 15%, only where it
 * reads at 4.5:1 or more (`MINIMUM`). Black is the other: white reads at
 * 4.5:1 or more on an accent whose luminance is 0.183 or less, black on
 * one of 0.175 or more, so one of the two always does.
 *
 * The page knows the accent only as the colour `AccentColor` computes to
 * (WebKit's, the user's choice); it changes while Banager runs, with no
 * event to say so. So the words are worked out again whenever the window
 * comes back to the front -- the user picked a colour in System Settings,
 * then came back -- and whenever the appearance or Increase Contrast
 * changes. Without a colour it can read, the words stay white.
 */

/** sRGB channels, 0 to 255. */
export interface Rgb {
  r: number;
  g: number;
  b: number;
}

/** WCAG's contrast for body text: 4.5:1. */
export const MINIMUM = 4.5;

/**
 * White words stay on an accent they read on at this or better: the
 * Mac's own blue reads at 4.0:1, and 3.6:1 in an older macOS's dark
 * appearance (decision I21a keeps both); macOS 27's graphite, the
 * nearest of the others, 3.4:1.
 */
export const KEEP_WHITE = 3.5;

/** Set on `<html>` to `dark` for black words on the accent; absent for white (index.css). */
export const ACCENT_INK_ATTRIBUTE = "data-accent-ink";

function linear(channel: number): number {
  const c = channel / 255;
  return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
}

/** WCAG 2's relative luminance. */
function luminance({ r, g, b }: Rgb): number {
  return 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b);
}

/** WCAG 2's contrast ratio of two colours, 1 to 21, whichever is lighter. */
export function contrastRatio(a: Rgb, b: Rgb): number {
  const [high, low] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (high + 0.05) / (low + 0.05);
}

const WHITE: Rgb = { r: 255, g: 255, b: 255 };

/**
 * The words for `accent`: `light`, white, where they read at `KEEP_WHITE`
 * or better -- at `MINIMUM` or better under Increase Contrast
 * (`moreContrast`) -- and `dark`, black, everywhere else.
 */
export function accentInk(accent: Rgb, moreContrast: boolean): "light" | "dark" {
  return contrastRatio(WHITE, accent) >= (moreContrast ? MINIMUM : KEEP_WHITE) ? "light" : "dark";
}

const NUMBER = String.raw`(-?\d*\.?\d+(?:e[-+]?\d+)?)`;
const RGB_FORM = new RegExp(
  String.raw`^rgba?\(\s*${NUMBER}[\s,]+${NUMBER}[\s,]+${NUMBER}(?:\s*[,/]\s*${NUMBER}%?)?\s*\)$`,
  "i",
);
const SRGB_FORM = new RegExp(
  String.raw`^color\(\s*srgb\s+${NUMBER}\s+${NUMBER}\s+${NUMBER}(?:\s*/\s*${NUMBER}%?)?\s*\)$`,
  "i",
);

/**
 * A computed colour's channels: `rgb(0, 122, 255)` as `AccentColor`
 * computes, or `color(srgb 0 0.41 0.85)` as a `color-mix()` does (the
 * accent Increase Contrast darkens). Alpha is left out: the accent is
 * opaque. Anything else, null.
 */
export function parseComputedColor(value: string): Rgb | null {
  const rgb = RGB_FORM.exec(value.trim());
  if (rgb !== null) return { r: Number(rgb[1]), g: Number(rgb[2]), b: Number(rgb[3]) };
  const srgb = SRGB_FORM.exec(value.trim());
  if (srgb !== null) return { r: Number(srgb[1]) * 255, g: Number(srgb[2]) * 255, b: Number(srgb[3]) * 255 };
  return null;
}

/** The accent as the page draws it now: a hidden probe's computed colour. */
function readAccentFromPage(): string {
  const probe = document.createElement("span");
  probe.hidden = true;
  probe.style.color = "var(--color-accent)";
  document.body.append(probe);
  const colour = getComputedStyle(probe).color;
  probe.remove();
  return colour;
}

/** Whether Increase Contrast is on (System Settings > Accessibility > Display). */
function moreContrastNow(): boolean {
  return typeof window.matchMedia === "function" && window.matchMedia("(prefers-contrast: more)").matches;
}

export interface AccentInkSources {
  /** The accent's computed colour, as `parseComputedColor` reads it. */
  readAccent?: () => string;
  /** Whether Increase Contrast is on. */
  moreContrast?: () => boolean;
}

/**
 * Marks `<html>` for black words on the accent where `accentInk` says so,
 * now and each time the accent may have changed; returns what stops it.
 */
export function watchAccentInk({
  readAccent = readAccentFromPage,
  moreContrast = moreContrastNow,
}: AccentInkSources = {}): () => void {
  const root = document.documentElement;
  const update = () => {
    const accent = parseComputedColor(readAccent());
    if (accent !== null && accentInk(accent, moreContrast()) === "dark") {
      root.setAttribute(ACCENT_INK_ATTRIBUTE, "dark");
    } else {
      root.removeAttribute(ACCENT_INK_ATTRIBUTE);
    }
  };
  update();
  const onVisibility = () => {
    if (document.visibilityState !== "hidden") update();
  };
  window.addEventListener("focus", update);
  document.addEventListener("visibilitychange", onVisibility);
  const queries =
    typeof window.matchMedia === "function"
      ? ["(prefers-contrast: more)", "(prefers-color-scheme: dark)"].map((query) => window.matchMedia(query))
      : [];
  for (const query of queries) query.addEventListener("change", update);
  return () => {
    window.removeEventListener("focus", update);
    document.removeEventListener("visibilitychange", onVisibility);
    for (const query of queries) query.removeEventListener("change", update);
  };
}
