/**
 * The app's buttons, as macOS draws its own (polish-3 spec §2.7 and §3.5,
 * measured off AppKit on macOS 27: refs/apple-native/native-controls-*.png).
 * Three sizes -- small 20, regular 24, large 28 -- and two kinds:
 *
 * - grey: the fill, the text in the label colour, a darker fill while it
 *   is pressed. Nothing changes under the pointer, as nothing does on a
 *   Mac's push button.
 * - default: the accent, white text in the regular weight, 15% darker while
 *   pressed. The one thing a screen asks for -- a dialog's confirmation,
 *   the Overview's Review updates -- and never red: an action the user
 *   chose, removing something included, is not tinted as a warning
 *   (HIG, Alerts).
 *
 * Off, either kind is the grey one with its text in the tertiary colour,
 * never a faded accent. The focus ring is the page's own (`:focus-visible`
 * in index.css), which follows each button's corners.
 *
 * Whole class strings, each a literal, so Tailwind finds every class here.
 */

/** What every button shares: its content centred on one line, and the regular weight whatever its parent's. */
const BUTTON_BASE = "inline-flex shrink-0 items-center justify-center gap-1.5 whitespace-nowrap font-normal";

const SMALL = "h-5 rounded-control px-2 text-small";
const REGULAR = "h-6 min-w-14 rounded-control px-3 text-body";
const LARGE = "h-7 min-w-20 rounded-full px-4 text-body";

const GREY = "bg-fill text-foreground enabled:active:bg-fill-pressed disabled:text-tertiary";
const DEFAULT = "bg-accent text-accent-foreground enabled:active:bg-accent-pressed disabled:bg-fill disabled:text-tertiary";

export type ButtonSize = "small" | "regular" | "large";
export type ButtonKind = "grey" | "default";

/**
 * `BUTTON[size][kind]`. Where each goes (§3.5): small for a notice's
 * button and the operation bar's; regular for a row's Update, Retry and
 * Uninstall…, a setting's row and an empty state; large for a dialog's
 * footer and the Overview's one button.
 */
export const BUTTON: Record<ButtonSize, Record<ButtonKind, string>> = {
  small: {
    grey: `${BUTTON_BASE} ${SMALL} ${GREY}`,
    default: `${BUTTON_BASE} ${SMALL} ${DEFAULT}`,
  },
  regular: {
    grey: `${BUTTON_BASE} ${REGULAR} ${GREY}`,
    default: `${BUTTON_BASE} ${REGULAR} ${DEFAULT}`,
  },
  large: {
    grey: `${BUTTON_BASE} ${LARGE} ${GREY}`,
    default: `${BUTTON_BASE} ${LARGE} ${DEFAULT}`,
  },
};

/**
 * A toolbar's button with a glyph and no words -- Check again's ⟳, a ⋯
 * menu, a close ×: 28 by 28, no fill at rest, the glyph at 16 in the muted
 * colour. The one button with a change under the pointer, the quietest
 * fill, as a toolbar item's; pressed, the grey button's fill. Its words
 * are its accessible name, and its tooltip where it has one.
 */
export const ICON_BUTTON =
  "inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-control text-muted enabled:hover:bg-fill-subtle enabled:active:bg-fill disabled:text-tertiary [&>svg]:size-4";

/**
 * Words in a line that do something -- a notice's Details, the Overview's
 * 「2个已隐藏」: the accent as text (linkColor), the regular weight, and
 * the size of the line they are in. No underline, under the pointer or
 * not: a Mac's links in a window's text have none.
 */
export const LINK = "rounded-sm font-normal text-accent-text";
