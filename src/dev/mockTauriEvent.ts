/**
 * The browser preview's stand-in for "@tauri-apps/api/event"
 * (docs/ui-preview.md), aliased by vite.config.ts in `vite --mode mock`
 * and in no other mode, as ./mockTauri.ts is for "@tauri-apps/api/core".
 * src/lib/api.ts listens through it for the menu bar's items that act in
 * the page (`onMenuCommand`), and for Rust's question before a quit
 * (`onQuitRequested`). The browser has no menu bar of Banager's, and
 * `pnpm tauri:mock`'s -- the app's own, which Rust puts up -- could only
 * be heard by asking the real backend to listen, which the preview never
 * does. So nothing is heard by itself: this keeps the page's listeners,
 * and `window.mockMenu("copy-diagnostics")` -- typed in the browser's
 * console, or sent by a screenshot script -- sends the one a menu item's
 * id names (`PageCommand::id` in src-tauri/src/menu.rs), as Rust sends it
 * when that item is chosen.
 *
 * Typed against the real module, as ./mockTauri.ts is, so a change to how
 * api.ts listens fails `pnpm typecheck` here too.
 */
import type { EventCallback, listen as tauriListen, UnlistenFn } from "@tauri-apps/api/event";

const listeners = new Map<string, Set<EventCallback<unknown>>>();

export async function listen<T>(event: string, handler: EventCallback<T>): Promise<UnlistenFn> {
  const callback = handler as EventCallback<unknown>;
  const set = listeners.get(event) ?? new Set();
  set.add(callback);
  listeners.set(event, set);
  return () => {
    set.delete(callback);
  };
}

// Callable exactly as the real `listen` is.
listen satisfies typeof tauriListen;

/** Sends the page `menu://<id>`, as choosing that menu item does in the app; false when nothing listens for it. */
export function chooseMenuItem(id: string): boolean {
  const event = `menu://${id}`;
  const heard = [...(listeners.get(event) ?? [])];
  for (const handler of heard) handler({ event, id: 0, payload: null });
  return heard.length > 0;
}

if (typeof window !== "undefined") {
  (window as unknown as { mockMenu: typeof chooseMenuItem }).mockMenu = chooseMenuItem;
}
