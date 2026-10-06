import type { ReactNode } from "react";
import { useArtifactIcon } from "../lib/queries";
import type { ToolIcon } from "../lib/toolIcons";
import { useToolIcons } from "../lib/toolIconsContext";
import type { ArtifactKey } from "../lib/types";
import { TerminalIcon } from "./icons";
import { PackLogo, SourceAvatar } from "./SourceAvatar";

/** The key the icon query is given when there is no tool to ask about: it is never asked. */
const NO_KEY: ArtifactKey = { instance_id: "", kind: "Formula", name: "" };

/**
 * An app icon's size, as `SourceAvatar`'s of the same name: `md`, 32px, a
 * row's and a sheet line's; `sm`, 24px, a quiet line's, such as a
 * dialog's list of tools; `compact`, 20px, a line of the Updates page's
 * "Update History", which wears no source mark (at 20 a 14 mark would hide
 * the icon); `lg`, 48px, over a dialog's question about one tool, as
 * NSAlert puts an app's icon, and atop the Installed page's inspector.
 * Rounded as an app icon is at that size; the icon's own shape and margin
 * do the rest. Whole class names, for Tailwind.
 */
const ICON_CLASSES = {
  sm: "h-6 w-6 rounded-[5px]",
  md: "h-8 w-8 rounded-[7px]",
  lg: "h-12 w-12 rounded-[11px]",
  compact: "h-5 w-5 rounded-[4px]",
} as const;

export interface ToolAvatarProps {
  /**
   * The source's adapter id and name: its logo, or its coloured initial,
   * on the corner of the tool's icon -- the tool's own, or the neutral
   * program tile where it has none.
   */
  adapterId: string;
  sourceLabel: string;
  /**
   * The tool's key, which finds its own icon: a cask's is asked for its
   * app's (`useArtifactIcon`, which asks for nothing else), and any tool's
   * logo is looked up in the logo pack (`resolveToolIcon`). Left out,
   * there is nothing of its own to find: the program tile.
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
 * 3. the neutral program tile, a prompt on grey (`ProgramTile`): never
 *    its source's logo, which made a tool without one look like another
 *    -- tokei like rustup, both Rust's -- nor a letter (decision I8).
 *
 * Each wears the source's mark, 14px (16 on a dialog's 48), on its corner:
 * at the window's default 800px a row's source chip gives way to the
 * name, and the avatar is left to say where the tool comes from. A tool
 * whose logo is its source's -- one with its own installer, its own
 * source, such as rustup or Claude Code -- is its logo alone.
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
  if (logo !== null && sameLogo(logo, resolveSourceIcon(adapterId))) {
    return <SourceAvatar adapterId={adapterId} label={sourceLabel} size={size} />;
  }
  return (
    <WithSourceBadge adapterId={adapterId} sourceLabel={sourceLabel} size={size}>
      {logo !== null ? <PackLogo icon={logo} size={size} /> : <ProgramTile size={size} />}
    </WithSourceBadge>
  );
}

/**
 * The prompt on the program tile at each size: the 18 the Other Programs
 * page draws its tile's at 32 (`ProgramAvatar` in
 * src/pages/UnknownPage.tsx), and about as much room around it at the
 * others -- a little less at 24, where the 14 badge takes over half the
 * tile. Where a badge sits on the corner the prompt is set a little up
 * and to the left, as Terminal's own icon has its prompt top left: at 32
 * a centred prompt's cursor ran under the badge. At 20, with no badge,
 * it is centred. Whole class names, for Tailwind.
 */
const PROMPTS = {
  sm: { size: 12, shift: "-translate-x-px -translate-y-0.5" },
  md: { size: 18, shift: "-translate-x-px -translate-y-0.5" },
  lg: { size: 27, shift: "-translate-x-0.5 -translate-y-0.75" },
  compact: { size: 12, shift: undefined },
} as const;

/**
 * The icon of a command-line tool that has none of its own -- no app
 * icon, no logo in the pack: a prompt, white on the neutral grey, as
 * Finder draws a Unix executable and as the Other Programs page draws a
 * program no source accounts for (`data-program-tile`). Never its
 * source's logo, which made a tool look like another, and never a letter
 * (decision I8). Its source's mark goes on its corner.
 *
 * The grey is `neutral-avatar`: systemGray, in the dark systemGray3, the
 * Other Programs page's (index.css). In dark mode a 1px edge of 12% white just inside it, as a logo's square
 * has (`PackLogo`), so the dark grey keeps its outline on the dark
 * content and a selected row.
 */
function ProgramTile({ size }: { size: keyof typeof ICON_CLASSES }) {
  return (
    <span
      aria-hidden="true"
      data-program-tile=""
      className={`inline-flex shrink-0 items-center justify-center bg-neutral-avatar text-white dark:inset-ring dark:inset-ring-white/12 ${ICON_CLASSES[size]}`}
    >
      <TerminalIcon size={PROMPTS[size].size} className={PROMPTS[size].shift} />
    </span>
  );
}

/**
 * How far over the avatar's corner its badge sits: 2px on a row's 32px;
 * 4px on a quiet line's 24px, where it would otherwise hide a good part of
 * the logo, and on a 48px one, a dialog's or the inspector's. Whole class
 * names, for Tailwind.
 */
const BADGE_OFFSET_CLASSES = {
  sm: "-bottom-1 -right-1",
  md: "-bottom-0.5 -right-0.5",
  lg: "-bottom-1 -right-1",
} as const;

/** The badge's own size: 14px, or 16px on a dialog's 48px icon. */
const BADGE_SIZES = { sm: "badge", md: "badge", lg: "xs" } as const;

interface WithSourceBadgeProps {
  adapterId: string;
  sourceLabel: string;
  size: keyof typeof ICON_CLASSES;
  /** The tool's own icon or logo, or the program tile. */
  children: ReactNode;
}

/**
 * A tool's own icon or logo, or the program tile, with its source's
 * avatar, 14px, on its bottom-right corner, a little over the edge
 * (`data-source-badge`). A ring in the surface's colour cuts the badge out
 * of what is under it, so that a Homebrew badge on an amber logo still
 * reads as a mark of its own.
 * At 20 (`compact`) there is no room for one: the icon alone.
 */
function WithSourceBadge({ adapterId, sourceLabel, size, children }: WithSourceBadgeProps) {
  if (size === "compact") {
    return (
      <span aria-hidden="true" className="relative inline-flex shrink-0">
        {children}
      </span>
    );
  }
  return (
    <span aria-hidden="true" className="relative inline-flex shrink-0">
      {children}
      <span
        data-source-badge=""
        className={`absolute flex rounded-[4px] ring-[1.5px] ring-surface ${BADGE_OFFSET_CLASSES[size]}`}
      >
        <SourceAvatar adapterId={adapterId} label={sourceLabel} size={BADGE_SIZES[size]} />
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
