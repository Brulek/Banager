/**
 * Each source's colour and the colour of the initial on it, by adapter id
 * (docs/superpowers/2026-09-27-ui-redesign.md, 视觉: a source's colour is
 * used on its small avatar and nowhere else). The Python tools share one
 * colour, and so do Cargo and rustup, the two Rust ones. Whole class
 * names, so Tailwind finds every one of them in this file.
 */
export const SOURCE_AVATAR_CLASSES: Record<string, string> = {
  brew: "bg-source-homebrew text-source-homebrew-ink",
  npm: "bg-source-npm text-white",
  pipx: "bg-source-python text-white",
  uv: "bg-source-python text-white",
  pip: "bg-source-python text-white",
  cargo: "bg-source-rust text-white",
  "standalone-rustup": "bg-source-rust text-white",
  ollama: "bg-source-ollama text-source-ollama-ink",
  "standalone-claude": "bg-source-claude text-white",
  "standalone-grok": "bg-source-grok text-white",
  "standalone-agy": "bg-source-agy text-white",
};

/** A source this build has no colour for: the muted grey, never a guess. */
const UNKNOWN_SOURCE_CLASSES = "bg-muted text-white";

/**
 * `sm`, 24px: a tile's or a list's small mark. `md`, 32px: a tool's row
 * (`ToolRow`) and the Overview's tiles. Whole class names, for Tailwind.
 */
const SIZE_CLASSES = {
  sm: "h-6 w-6 rounded-[7px] text-small",
  md: "h-8 w-8 rounded-[9px] text-body",
} as const;

export type SourceAvatarSize = keyof typeof SIZE_CLASSES;

export interface SourceAvatarProps {
  adapterId: string;
  /** The source's name as the user reads it; its first letter goes on the avatar. */
  label: string;
  size?: SourceAvatarSize;
}

/**
 * A small rounded square in the source's colour with the first letter of
 * its name. Decorative: the name itself is always beside it.
 */
export function SourceAvatar({ adapterId, label, size = "sm" }: SourceAvatarProps) {
  // `hasOwnProperty`, not a plain lookup: an id like "toString" would find
  // a function on the prototype.
  const colours = Object.prototype.hasOwnProperty.call(SOURCE_AVATAR_CLASSES, adapterId)
    ? SOURCE_AVATAR_CLASSES[adapterId]
    : UNKNOWN_SOURCE_CLASSES;
  return (
    <span
      aria-hidden="true"
      className={`inline-flex shrink-0 items-center justify-center font-semibold ${SIZE_CLASSES[size]} ${colours}`}
    >
      {label.slice(0, 1).toUpperCase()}
    </span>
  );
}
