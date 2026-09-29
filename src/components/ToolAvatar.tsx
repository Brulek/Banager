import type { ReactNode } from "react";
import { useArtifactIcon } from "../lib/queries";
import type { ToolIcon } from "../lib/toolIcons";
import { useToolIcons } from "../lib/toolIconsContext";
import type { ArtifactKey } from "../lib/types";
import { PackLogo, SourceAvatar } from "./SourceAvatar";

/** The key the icon query is given when there is no tool to ask about: it is never asked. */
const NO_KEY: ArtifactKey = { instance_id: "", kind: "Formula", name: "" };

/**
 * An app icon's size, as `SourceAvatar`'s of the same name: `md`, 32px, a
 * row's and a sheet line's; `sm`, 24px, a quiet line's, such as the
 * Updates page's "Just updated"; `lg`, 48px, the Installed page's
 * inspector's. Rounded as an app icon is at that
 * size; the icon's own shape and margin do the rest. Whole class names,
 * for Tailwind.
 */
const ICON_CLASSES = {
  sm: "h-6 w-6 rounded-[5px]",
  md: "h-8 w-8 rounded-[7px]",
  lg: "h-12 w-12 rounded-[11px]",
} as const;

export interface ToolAvatarProps {
  /**
   * The source's adapter id and name: its logo, or its coloured initial,
   * in place of the tool's own icon where it has none, and on its corner
   * where it has one.
   */
  adapterId: string;
  sourceLabel: string;
  /**
   * The tool's key, which finds its own icon: a cask's is asked for its
   * app's (`useArtifactIcon`, which asks for nothing else), and any tool's
   * logo is looked up in the logo pack (`resolveToolIcon`). Left out, the
   * source's avatar is all there is.
   */
  iconKey?: ArtifactKey;
  /** `md` unless said: a row's. */
  size?: keyof typeof ICON_CLASSES;
}

/**
 * The avatar at the start of a tool's row, sheet line or drawer, the first
 * of these there is:
 *
 * 1. the app's own icon -- the one Finder shows -- for an app Homebrew
 *    installed, once it has arrived, drawn as macOS draws it, with no
 *    coloured square behind it;
 * 2. the tool's logo, from the logo pack;
 * 3. its source's logo, from the pack;
 * 4. its source's coloured initial.
 *
 * The first two wear the source's mark, 14px, on their corner: at the
 * window's default 800px a row's source chip gives way to the name, and
 * the avatar is left to say where the tool comes from. The last two are
 * the source's own avatar, and wear none; nor does a tool whose logo is
 * its source's, such as one with its own installer, its own source.
 *
 * The app's icon is asked for only when an avatar is drawn, so a
 * virtualized list asks for the rows on screen and no others. Decorative,
 * like the initial: the name is always beside it.
 */
export function ToolAvatar({ adapterId, sourceLabel, iconKey, size = "md" }: ToolAvatarProps) {
  const { resolveToolIcon, resolveSourceIcon } = useToolIcons();
  const { data: appIcon } = useArtifactIcon(iconKey ?? NO_KEY, iconKey !== undefined);
  if (typeof appIcon === "string") {
    return (
      <WithSourceBadge adapterId={adapterId} sourceLabel={sourceLabel} size={size}>
        <img
          src={appIcon}
          alt=""
          draggable={false}
          data-app-icon=""
          className={`${ICON_CLASSES[size]} shrink-0 object-contain`}
        />
      </WithSourceBadge>
    );
  }
  const logo = iconKey === undefined ? null : resolveToolIcon(iconKey, adapterId);
  if (logo !== null && !sameLogo(logo, resolveSourceIcon(adapterId))) {
    return (
      <WithSourceBadge adapterId={adapterId} sourceLabel={sourceLabel} size={size}>
        <PackLogo icon={logo} size={size} />
      </WithSourceBadge>
    );
  }
  return <SourceAvatar adapterId={adapterId} label={sourceLabel} size={size} />;
}

/**
 * How far over the avatar's corner its badge sits: 2px on a row's 32px and the inspector's 48px;
 * 4px on a quiet line's 24px, where it would otherwise hide a good part of
 * the logo. Whole class names, for Tailwind.
 */
const BADGE_OFFSET_CLASSES = {
  sm: "-bottom-1 -right-1",
  md: "-bottom-0.5 -right-0.5",
  lg: "-bottom-0.5 -right-0.5",
} as const;

interface WithSourceBadgeProps {
  adapterId: string;
  sourceLabel: string;
  size: keyof typeof BADGE_OFFSET_CLASSES;
  /** The tool's own icon or logo. */
  children: ReactNode;
}

/**
 * A tool's own icon or logo with its source's avatar, 14px, on its
 * bottom-right corner, a little over the edge (`data-source-badge`). A
 * ring in the surface's colour cuts the badge out of what is under it, so
 * that a Homebrew badge on an amber logo still reads as a mark of its own.
 */
function WithSourceBadge({ adapterId, sourceLabel, size, children }: WithSourceBadgeProps) {
  return (
    <span aria-hidden="true" className="relative inline-flex shrink-0">
      {children}
      <span
        data-source-badge=""
        className={`absolute flex rounded-[4px] ring-[1.5px] ring-surface ${BADGE_OFFSET_CLASSES[size]}`}
      >
        <SourceAvatar adapterId={adapterId} label={sourceLabel} size="badge" />
      </span>
    </span>
  );
}

/**
 * Whether `logo` is `sourceLogo`: the one logo drawn for a tool and for
 * its source, such as Claude Code's, whose installer is its own.
 */
function sameLogo(logo: ToolIcon, sourceLogo: ToolIcon | null): boolean {
  if (sourceLogo === null) return false;
  return logo.kind === "raster"
    ? sourceLogo.kind === "raster" && sourceLogo.url === logo.url
    : sourceLogo.kind === "glyph" && sourceLogo.path === logo.path && sourceLogo.hex === logo.hex;
}
