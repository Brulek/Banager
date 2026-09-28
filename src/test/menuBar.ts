import { act } from "@testing-library/react";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { vi } from "vitest";
import { MENU_EVENTS, type MenuCommand } from "../lib/api";

/**
 * The menu bar, for a test: from the call on, the page's listeners for its
 * events (`onMenuCommand`, through the mocked `listen` in ./setup.ts) are
 * kept here, and `choose` sends one the event Rust sends the window when
 * the user chooses that item (src-tauri/src/menu.rs).
 */
export function fakeMenuBar(): { choose(command: MenuCommand): void; listening(): string[] } {
  const listeners = new Map<string, EventCallback<unknown>>();
  vi.mocked(listen).mockImplementation(async (event, handler) => {
    listeners.set(event, handler as EventCallback<unknown>);
    return () => {
      listeners.delete(event);
    };
  });
  return {
    choose(command) {
      const event = MENU_EVENTS[command];
      const handler = listeners.get(event);
      if (handler === undefined) throw new Error(`nothing listens for ${event}`);
      act(() => handler({ event, id: 0, payload: null }));
    },
    listening: () => [...listeners.keys()].sort(),
  };
}
