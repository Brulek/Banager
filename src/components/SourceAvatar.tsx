import { glyphInk, type ToolIcon } from "../lib/toolIcons";
import { useToolIcons } from "../lib/toolIconsContext";

/**
 * Each source's colour and the colour of the initial on it, by adapter id
 * (docs/superpowers/2026-09-27-ui-redesign.md, 视觉: a source's colour is
 * used on its small avatar and nowhere else), for a source the logo pack
 * has no logo for. The Python tools share one colour, and so do Cargo and
 * rustup, the two Rust ones. Whole class names, so Tailwind finds every
 * one of them in this file.
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
 * `xs`, 16px: a source's mark inside a chip, such as the Installed page's
 * filters. `sm`, 24px: a tile's or a list's small mark. `md`, 32px: a
 * tool's row (`ToolRow`) and the Overview's tiles. The square and its
 * corners, whatever is drawn on it. Whole class names, for Tailwind.
 */
const SIZE_CLASSES = {
  xs: "h-4 w-4 rounded-[5px]",
  sm: "h-6 w-6 rounded-[7px]",
  md: "h-8 w-8 rounded-[9px]",
} as const;

export type SourceAvatarSize = keyof typeof SIZE_CLASSES;

/** The initial's type, on each size of square. */
const LETTER_CLASSES: Record<SourceAvatarSize, string> = {
  xs: "text-[10px] leading-none",
  sm: "text-small",
  md: "text-body",
};

/**
 * A glyph's box on each size of square: at a row's 32px, the 18px the
 * Unknown page's program avatar draws its mark at, and about as much room
 * around it at the smaller sizes.
 */
const GLYPH_CLASSES: Record<SourceAvatarSize, string> = {
  xs: "h-2.5 w-2.5",
  sm: "h-3.5 w-3.5",
  md: "h-[18px] w-[18px]",
};

interface PackLogoProps {
  icon: ToolIcon;
  size: SourceAvatarSize;
}

/**
 * A logo from the logo pack (src/lib/toolIcons.ts) on an avatar's square.
 * A glyph is drawn in white or near-black, whichever reads better on its
 * brand's colour (`glyphInk`), on a square of that colour, in dark mode as
 * in light. A raster fills a white square inside the border colour's
 * hairline: some are black on transparent, and would vanish on dark mode's
 * surfaces. `data-logo` says which it is. Decorative, as the initial is.
 */
function PackLogo({ icon, size }: PackLogoProps) {
  if (icon.kind === "raster") {
    return (
      <img
        src={icon.url}
        alt=""
        aria-hidden="true"
        draggable={false}
        data-logo="raster"
        className={`${SIZE_CLASSES[size]} shrink-0 border border-border bg-white object-contain`}
      />
    );
  }
  return (
    <span
      aria-hidden="true"
      data-logo="glyph"
      className={`inline-flex shrink-0 items-center justify-center ${SIZE_CLASSES[size]}`}
      style={{ backgroundColor: `#${icon.hex}` }}
    >
      <svg viewBox="0 0 24 24" fill={glyphInk(icon.hex)} className={GLYPH_CLASSES[size]}>
        <path d={icon.path} />
      </svg>
    </span>
  );
}

export interface SourceAvatarProps {
  adapterId: string;
  /** The source's name as the user reads it; its first letter goes on the avatar where it has no logo. */
  label: string;
  size?: SourceAvatarSize;
}

/**
 * A source's mark: its logo from the logo pack, or, for a source the pack
 * has none for, a small rounded square in the source's colour with the
 * first letter of its name. Decorative: the name itself is always beside
 * it.
 */
export function SourceAvatar({ adapterId, label, size = "sm" }: SourceAvatarProps) {
  const logo = useToolIcons().resolveSourceIcon(adapterId);
  if (logo !== null) return <PackLogo icon={logo} size={size} />;
  // `hasOwnProperty`, not a plain lookup: an id like "toString" would find
  // a function on the prototype.
  const colours = Object.prototype.hasOwnProperty.call(SOURCE_AVATAR_CLASSES, adapterId)
    ? SOURCE_AVATAR_CLASSES[adapterId]
    : UNKNOWN_SOURCE_CLASSES;
  return (
    <span
      aria-hidden="true"
      className={`inline-flex shrink-0 items-center justify-center font-semibold ${SIZE_CLASSES[size]} ${LETTER_CLASSES[size]} ${colours}`}
    >
      {label.slice(0, 1).toUpperCase()}
    </span>
  );
}
