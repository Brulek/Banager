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

/** Updates: an arrow up, in a circle. */
export function UpdatesIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 16.5V7.5M8.5 11L12 7.5L15.5 11" />
    </Icon>
  );
}

/** Installed: a storage box with its lid. */
export function InstalledIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <rect x="3.5" y="4.5" width="17" height="4.5" rx="1.25" />
      <path d="M5 9v8.5A1.5 1.5 0 0 0 6.5 19h11a1.5 1.5 0 0 0 1.5-1.5V9M10 12.5h4" />
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

/** Settings: an eight-toothed gear around its axle. */
export function SettingsIcon(props: IconProps) {
  return (
    <Icon {...props}>
      <path d="M10.33 5.3L10.59 3.11H13.41L13.67 5.3L15.55 6.09L17.29 4.72L19.28 6.71L17.91 8.45L18.7 10.33L20.89 10.59V13.41L18.7 13.67L17.91 15.55L19.28 17.29L17.29 19.28L15.55 17.91L13.67 18.7L13.41 20.89H10.59L10.33 18.7L8.45 17.91L6.71 19.28L4.72 17.29L6.09 15.55L5.3 13.67L3.11 13.41V10.59L5.3 10.33L6.09 8.45L4.72 6.71L6.71 4.72L8.45 6.09Z" />
      <circle cx="12" cy="12" r="2.75" />
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
