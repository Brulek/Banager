/**
 * The browser preview's stand-in for "@tauri-apps/api/event"
 * (docs/ui-preview.md), aliased by vite.config.ts in `vite --mode mock`
 * and in no other mode, as ./mockTauri.ts is for "@tauri-apps/api/core".
 * src/lib/api.ts listens through it for the menu bar's items that act in
 * the page (`onMenuCommand`), and for Rust's question before a quit
 * (`onQuitRequested`). The browser has no menu bar of Canager's, and
 * `pnpm tauri:mock`'s -- the app's own, which Rust puts up -- could only
 * be heard by asking the real backend to listen, which the preview never
 * does: this listens to nothing, and nothing is ever heard.
 *
 * Typed against the real module, as ./mockTauri.ts is, so a change to how
 * api.ts listens fails `pnpm typecheck` here too.
 */
import type { listen as tauriListen, UnlistenFn } from "@tauri-apps/api/event";

export async function listen(): Promise<UnlistenFn> {
  return () => {};
}

// Callable exactly as the real `listen` is.
listen satisfies typeof tauriListen;
