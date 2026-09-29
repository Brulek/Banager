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

/** Unknown: a question mark, in a circle. */
export function UnknownIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="12" cy="12" r="9" />
      <path d="M9.6 9.4a2.5 2.5 0 0 1 4.9.7c0 1.7-2.5 2.2-2.5 3.9M12 17h.01" />
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

/** The row's other actions: three dots in a row. */
export function MoreIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M6 12h.01M12 12h.01M18 12h.01" strokeWidth={2.75} />
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
 * Something is happening: a quarter of a circle that turns, over a faint
 * whole one. Turns only for someone who has not asked for less motion.
 */
export function SpinnerIcon({ size = 18, className }: IconProps) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      strokeWidth={2.25}
      strokeLinecap="round"
      aria-hidden="true"
      className={`motion-safe:animate-spin ${className ?? ""}`}
    >
      <circle cx="12" cy="12" r="8.5" stroke="currentColor" opacity={0.25} />
      <path d="M12 3.5A8.5 8.5 0 0 1 20.5 12" stroke="currentColor" />
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
