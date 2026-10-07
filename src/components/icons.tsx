import type { ReactNode } from "react";

/**
 * The app's icons, drawn here by hand: 24-unit squares, one stroke weight,
 * round ends, `currentColor` so each takes the colour of the text beside
 * it. Decorative -- every one sits next to words that say the same thing
 * -- so hidden from assistive technology.
 */
interface IconProps {
  /** Rendered width and height in pixels. */
  size?: number;
  className?: string;
}

function Icon({ size = 18, className, children }: IconProps & { children: ReactNode }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.75}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      className={className}
    >
      {children}
    </svg>
  );
}

/** Overview: four tiles, two by two. */
export function OverviewIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <rect x="4" y="4" width="6.5" height="6.5" rx="1.5" />
      <rect x="13.5" y="4" width="6.5" height="6.5" rx="1.5" />
      <rect x="4" y="13.5" width="6.5" height="6.5" rx="1.5" />
      <rect x="13.5" y="13.5" width="6.5" height="6.5" rx="1.5" />
    </Icon>
  );
}

/** Updates: an arrow down, in a circle -- SF Symbols' arrow.down.circle, as the App Store's Updates. */
export function UpdatesIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 7.5V16.5M8.5 13L12 16.5L15.5 13" />
    </Icon>
  );
}

/**
 * Installed: a shipping box seen from above one corner, with the tape
 * across its lid -- SF Symbols' shippingbox, drawn to the same proportions
 * (their own artwork is licensed for Apple's platforms only).
 */
export function InstalledIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M12 3L20 7.5V16.5L12 21L4 16.5V7.5Z" />
      <path d="M4 7.5L12 12L20 7.5M12 12V21M8 5.25L16 9.75" />
    </Icon>
  );
}

/** A command-line program: a prompt and its cursor. */
export function TerminalIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M5.5 8L9.5 12L5.5 16M12.5 16.5H18.5" />
    </Icon>
  );
}

/**
 * Settings: a gear of eight short, round-topped teeth around its axle --
 * SF Symbols' gearshape, drawn to its proportions: the teeth follow the
 * gear's circle, rather than stand off it as square blocks.
 */
export function SettingsIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M9.96 5.10L10.31 2.90A9.25 9.25 0 0 1 13.69 2.90L14.04 5.10A7.20 7.20 0 0 1 15.44 5.67L17.24 4.38A9.25 9.25 0 0 1 19.62 6.76L18.33 8.56A7.20 7.20 0 0 1 18.90 9.96L21.10 10.31A9.25 9.25 0 0 1 21.10 13.69L18.90 14.04A7.20 7.20 0 0 1 18.33 15.44L19.62 17.24A9.25 9.25 0 0 1 17.24 19.62L15.44 18.33A7.20 7.20 0 0 1 14.04 18.90L13.69 21.10A9.25 9.25 0 0 1 10.31 21.10L9.96 18.90A7.20 7.20 0 0 1 8.56 18.33L6.76 19.62A9.25 9.25 0 0 1 4.38 17.24L5.67 15.44A7.20 7.20 0 0 1 5.10 14.04L2.90 13.69A9.25 9.25 0 0 1 2.90 10.31L5.10 9.96A7.20 7.20 0 0 1 5.67 8.56L4.38 6.76A9.25 9.25 0 0 1 6.76 4.38L8.56 5.67A7.20 7.20 0 0 1 9.96 5.10Z" />
      <circle cx="12" cy="12" r="3" />
    </Icon>
  );
}

/** Check again: a circle that turns clockwise, with its arrowhead at the top. */
export function RefreshIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M17.36 7.5A7 7 0 1 1 10.78 5.11M8.75 3.17L11.38 5L9.54 7.62" />
    </Icon>
  );
}

/** Up to date: a check mark, in a circle. */
export function CheckCircleIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="12" cy="12" r="9" />
      <path d="M8 12.4L10.8 15.2L16.2 9.4" />
    </Icon>
  );
}

/**
 * Kept private: a closed padlock, a half-circle shackle over a rounded
 * body with its keyhole -- SF Symbols' lock, as tall as the circled icons.
 */
export function PrivacyIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <rect x="5" y="11" width="14" height="10" rx="2.5" />
      <path d="M8 11V7.5A4 4 0 0 1 16 7.5V11M12 15V17" />
    </Icon>
  );
}

/** Done: a check mark on its own. */
export function CheckIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M5.5 12.5L10 17L18.5 7.5" />
    </Icon>
  );
}

/** More about this: an "i", in a circle. */
export function InfoIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 11v5.5M12 7.75h.01" />
    </Icon>
  );
}

/** Look at this: an exclamation mark, in a triangle. */
export function WarningIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M10.3 4.4L2.9 17.3A2 2 0 0 0 4.6 20.3H19.4A2 2 0 0 0 21.1 17.3L13.7 4.4A2 2 0 0 0 10.3 4.4Z" />
      <path d="M12 9.5v4.5M12 17h.01" />
    </Icon>
  );
}

/**
 * A warning, as macOS marks one beside a word (exclamationmark.triangle.fill):
 * the triangle filled in the colour it is given -- systemOrange -- and the
 * exclamation mark cut out of it in white, whatever the appearance.
 */
export function WarningFilledIcon({ size = 18, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true" className={className}>
      <path
        d="M10.27 3.5 1.9 18a2 2 0 0 0 1.73 3h16.74a2 2 0 0 0 1.73-3L13.73 3.5a2 2 0 0 0-3.46 0Z"
        fill="currentColor"
      />
      <path d="M12 8.75v5.5" stroke="#fff" strokeWidth={2.25} strokeLinecap="round" />
      <circle cx="12" cy="17.4" r="1.3" fill="#fff" />
    </svg>
  );
}

/**
 * A disclosure triangle, as a Mac list's (NSDisclosureButton): filled,
 * pointing right while what it discloses is hidden; turned down by its
 * owner (`rotate-90`) once it shows.
 */
export function DisclosureIcon({ size = 10, className }: IconProps) {
  return (
    <svg width={size} height={size} viewBox="0 0 10 10" aria-hidden="true" className={className}>
      <path d="M3 1.5 7.5 5 3 8.5Z" fill="currentColor" />
    </svg>
  );
}

/**
 * The row's other actions: three dots in a row, each 2.5 across at 16
 * (3.75 of 24) with about 2 between them -- big enough to be seen as the way to
 * a menu, as the ellipsis symbol's are.
 */
export function MoreIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M5 12h.01M12 12h.01M19 12h.01" strokeWidth={3.75} />
    </Icon>
  );
}

/** A section that opens: a chevron pointing right, turned down when open. */
export function ChevronIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M9.5 6.5L15 12L9.5 17.5" />
    </Icon>
  );
}

/** Nothing to do: a dash. */
export function DashIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M7 12h10" />
    </Icon>
  );
}

/**
 * Each spoke of the spinner, clockwise from twelve o'clock, and how faint
 * it is: the one at the top in the full colour, then an eighth fainter for
 * each spoke behind it, anticlockwise -- so the one just after the top, at
 * half past one, is the faintest, and the next to light up.
 */
const SPINNER_SPOKES = Array.from({ length: 8 }, (_, index) => ({
  angle: index * 45,
  opacity: 1 - ((8 - index) % 8) / 8,
}));

/**
 * Something is happening: macOS's spinning indicator, not a web page's
 * ring with a gap -- eight short spokes with round ends around the centre,
 * fading round the circle, as measured in native-controls (at 16: 2 wide,
 * from 3 to 7.5 out). It turns an eighth at a time (index.css), the lit
 * spoke stepping to the next, as the Mac's does. `currentColor`,
 * the top spoke in the caller's colour -- the muted grey, as the Mac draws
 * its own -- the others fainter still. For someone who has asked for less
 * motion it stands still, and still reads as the one it is.
 */
export function SpinnerIcon({ size = 18, className }: IconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={3}
      strokeLinecap="round"
      aria-hidden="true"
      className={`motion-safe:animate-spinner ${className ?? ""}`}
    >
      {SPINNER_SPOKES.map(({ angle, opacity }) => (
        <line key={angle} x1="12" y1="2.25" x2="12" y2="6" opacity={opacity} transform={`rotate(${angle} 12 12)`} />
      ))}
    </svg>
  );
}

/** Close: a cross. */
export function CloseIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M6.5 6.5L17.5 17.5M17.5 6.5L6.5 17.5" />
    </Icon>
  );
}

/** Search: a magnifying glass. */
export function SearchIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="11" cy="11" r="6.5" />
      <path d="M16 16L20 20" />
    </Icon>
  );
}
