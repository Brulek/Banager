import { useArtifactIcon } from "../lib/queries";
import type { ArtifactKey } from "../lib/types";
import { SourceAvatar } from "./SourceAvatar";

/** The key the icon query is given when there is no tool to ask about: it is never asked. */
const NO_KEY: ArtifactKey = { instance_id: "", kind: "Formula", name: "" };

/**
 * An app icon's size, as `SourceAvatar`'s of the same name: `md`, 32px, a
 * row's, a sheet line's and a drawer's; `sm`, 24px, a quiet line's, such as
 * the Updates page's "Just updated". Rounded as an app icon is at that
 * size; the icon's own shape and margin do the rest. Whole class names,
 * for Tailwind.
 */
const ICON_CLASSES = {
  sm: "h-6 w-6 rounded-[5px]",
  md: "h-8 w-8 rounded-[7px]",
} as const;

export interface ToolAvatarProps {
  /** The source's adapter id and name: its avatar, until an icon arrives or when there is none. */
  adapterId: string;
  sourceLabel: string;
  /**
   * The tool's key. A cask's is asked for its app's own icon
   * (`useArtifactIcon`, which asks for nothing else); left out, the
   * source's avatar is all there is.
   */
  iconKey?: ArtifactKey;
  /** `md` unless said: a row's. */
  size?: keyof typeof ICON_CLASSES;
}

/**
 * The avatar at the start of a tool's row, sheet line or drawer: the app's
 * own icon -- the one Finder shows -- for an app Homebrew installed, once
 * it has arrived, drawn as macOS draws it, with no coloured square behind
 * it; the source's avatar -- its logo, or its coloured initial -- while it
 * is on its way and wherever there is none. It is asked for only when an
 * avatar is drawn, so a virtualized list asks for the rows on screen and
 * no others. Decorative, like the source's: the name is always beside it.
 */
export function ToolAvatar({ adapterId, sourceLabel, iconKey, size = "md" }: ToolAvatarProps) {
  const { data: icon } = useArtifactIcon(iconKey ?? NO_KEY, iconKey !== undefined);
  if (typeof icon === "string") {
    return (
      <img
        src={icon}
        alt=""
        aria-hidden="true"
        draggable={false}
        data-app-icon=""
        className={`${ICON_CLASSES[size]} shrink-0 object-contain`}
      />
    );
  }
  return <SourceAvatar adapterId={adapterId} label={sourceLabel} size={size} />;
}
