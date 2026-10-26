/**
 * Installing Homebrew itself (phase 5 step C): Banager does not run its
 * installer. The person runs Homebrew's own command in Terminal, which
 * asks for the Mac's password there; Banager puts the command on the
 * clipboard, opens Terminal (`open_terminal`), and looks for Homebrew to
 * appear (`look_for_homebrew`). And the empty Mac's front door: opened
 * on a Mac with nothing to manage, Banager starts on Discover.
 */
import { useEffect, useRef } from "react";
import { useSnapshot } from "./queries";
import { useUiStore } from "../store/ui";
import type { Snapshot } from "./types";

/**
 * Homebrew's install command, exactly as brew.sh and its installer's
 * README give it: what Open Terminal puts on the clipboard, and what Show
 * Command shows. docs/what-we-run.md quotes it, and a test holds the two
 * the same.
 */
export const HOMEBREW_INSTALL_COMMAND =
  '/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"';

/**
 * The oldest macOS Homebrew supports (`MACOS_OLDEST_SUPPORTED` in its
 * installer): on an older one it warns that the install may not succeed
 * and goes on.
 */
export const HOMEBREW_OLDEST_MACOS = 15;

/** How often the hand-off looks for Homebrew while it waits, in ms. */
export const LOOK_EVERY_MS = 2_000;

/**
 * The major version of `version` ("14.5") where it is older than Homebrew
 * supports, or null: newer, or not known.
 */
export function olderThanHomebrewSupports(version: string | null | undefined): number | null {
  const major = Number.parseInt(version?.split(".")[0] ?? "", 10);
  return Number.isFinite(major) && major < HOMEBREW_OLDEST_MACOS ? major : null;
}

/**
 * Whether a finished check found a Mac with nothing to manage: no
 * Homebrew, and nothing listed but by pip -- macOS's own Python has one
 * (phase 5 proposal §1.1). Not before a check has finished: the startup
 * snapshot lists nothing because nothing was asked yet.
 */
export function isEmptyMac(snapshot: Snapshot): boolean {
  if (snapshot.refreshed_at === null) return false;
  if (snapshot.instances.some((instance) => instance.adapter_id === "brew")) return false;
  const pip = new Set(snapshot.instances.filter((instance) => instance.adapter_id === "pip").map((instance) => instance.id));
  return snapshot.artifacts.every((artifact) => pip.has(artifact.key.instance_id));
}

/**
 * The front door (phase 5 step C): the first check of this launch to
 * finish finds an empty Mac (`isEmptyMac`), and the page is still the one
 * Banager opened on, the Overview -- then it opens Discover, once. A page
 * the person chose meanwhile stays, and later checks change nothing.
 */
export function useFrontDoor(): void {
  const { data: snapshot } = useSnapshot();
  const decided = useRef(false);
  useEffect(() => {
    if (decided.current || snapshot === undefined || snapshot.refreshed_at === null) return;
    decided.current = true;
    const { page, setPage } = useUiStore.getState();
    if (page === "overview" && isEmptyMac(snapshot)) setPage("discover");
  }, [snapshot]);
}
