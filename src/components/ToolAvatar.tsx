import { useArtifactIcon } from "../lib/queries";
import type { ArtifactKey } from "../lib/types";
import { SourceAvatar } from "./SourceAvatar";

/** The key the icon query is given when there is no tool to ask about: it is never asked. */
const NO_KEY: ArtifactKey = { instance_id: "", kind: "Formula", name: "" };

export interface ToolAvatarProps {
  /** The source's adapter id and name: the coloured initial, until an icon arrives or when there is none. */
  adapterId: string;
  sourceLabel: string;
  /**
   * The tool's key. A cask's is asked for its app's own icon
   * (`useArtifactIcon`, which asks for nothing else); left out, the
   * source's avatar is all there is.
   */
  iconKey?: ArtifactKey;
}

/**
 * The avatar at the start of a tool's row, sheet line or drawer: the app's
 * own icon -- the one Finder shows -- for an app Homebrew installed, once
 * it has arrived, drawn as macOS draws it, with no coloured square behind
 * it; the source's coloured initial while it is on its way and wherever
 * there is none. It is asked for only when an avatar is drawn, so a
 * virtualized list asks for the rows on screen and no others. Decorative,
 * like the initial: the name is always beside it.
 */
export function ToolAvatar({ adapterId, sourceLabel, iconKey }: ToolAvatarProps) {
  const { data: icon } = useArtifactIcon(iconKey ?? NO_KEY, iconKey !== undefined);
  if (typeof icon === "string") {
    return (
      <img
        src={icon}
        alt=""
        aria-hidden="true"
        draggable={false}
        data-app-icon=""
        // 32px, a row avatar's size, rounded as an app icon is; the icon's
        // own shape and margin do the rest.
        className="h-8 w-8 shrink-0 rounded-[7px] object-contain"
      />
    );
  }
  return <SourceAvatar adapterId={adapterId} label={sourceLabel} size="md" />;
}
