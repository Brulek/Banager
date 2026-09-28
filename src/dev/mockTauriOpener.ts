/**
 * The browser preview's stand-in for "@tauri-apps/plugin-opener"
 * (docs/ui-preview.md), aliased by vite.config.ts in `vite --mode mock`
 * and in no other mode, as ./mockTauri.ts is for "@tauri-apps/api/core".
 * src/lib/api.ts asks Finder through it to show one of the Unknown page's
 * programs (`revealInFinder`). The browser has no Finder, and in `pnpm
 * tauri:mock`'s window the real module would ask the app's own backend --
 * that is, this Mac's Finder -- which the preview never does: this shows
 * nothing, and says in the console what it was asked to show.
 *
 * It has the one function api.ts calls, typed against the real module, as
 * ./mockTauri.ts is, so a change to how api.ts calls it fails `pnpm
 * typecheck` here too.
 */
import type { revealItemInDir as tauriRevealItemInDir } from "@tauri-apps/plugin-opener";
import { MOCK_MARKER } from "./mockTauri";

export async function revealItemInDir(path: string | string[]): Promise<void> {
  console.info(`[${MOCK_MARKER}] Show in Finder, not done in the preview: ${[path].flat().join(", ")}`);
}

// Callable exactly as the real `revealItemInDir` is.
revealItemInDir satisfies typeof tauriRevealItemInDir;
