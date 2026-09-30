/**
 * The browser preview's stand-in for "@tauri-apps/api/window"
 * (docs/ui-preview.md), aliased by vite.config.ts in `vite --mode mock`
 * and in no other mode, as ./mockTauri.ts is for "@tauri-apps/api/core".
 * src/lib/api.ts badges Banager's icon in the Dock through it
 * (`setDockBadge`). The browser has no such icon, and `pnpm tauri:mock`'s
 * -- the app's own -- could only be badged by asking the real backend,
 * which the preview never does: this badges nothing.
 *
 * It has the one method api.ts calls, typed as Tauri's `Window` types it,
 * so a Tauri whose method changes shape fails `pnpm typecheck` here too;
 * a method api.ts starts calling has to be added here as well.
 */
import type { Window } from "@tauri-apps/api/window";

/** Tauri's `Window`, as far as api.ts uses it. */
type PreviewWindow = Pick<Window, "setBadgeCount">;

const previewWindow: PreviewWindow = {
  async setBadgeCount() {},
};

export function getCurrentWindow(): PreviewWindow {
  return previewWindow;
}
