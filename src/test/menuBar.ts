import { act } from "@testing-library/react";
import { listen, type EventCallback } from "@tauri-apps/api/event";
import { vi } from "vitest";
import { MENU_EVENTS, type MenuCommand } from "../lib/api";

/**
 * The menu bar, for a test: from the call on, the page's listeners for its
 * events (`onMenuCommand`, through the mocked `listen` in ./setup.ts) are
 * kept here, and `choose` sends one the event Rust sends the window when
 * the user chooses that item (src-tauri/src/menu.rs). Every other event
 * the page listens for is kept too, and `hear` sends one by its name: the
 * update notification's click (`OPEN_UPDATES_EVENT`), which Rust sends the
 * way it sends the menu bar's (src-tauri/src/notify.rs), and the question
 * before a quit (`QUIT_REQUESTED_EVENT`), with its number as `payload`
 * (src-tauri/src/quit.rs).
 */
export function fakeMenuBar(): {
  choose(command: MenuCommand): void;
  hear(event: string, payload?: unknown): void;
  listening(): string[];
} {
  const listeners = new Map<string, EventCallback<unknown>>();
  vi.mocked(listen).mockImplementation(async (event, handler) => {
    listeners.set(event, handler as EventCallback<unknown>);
    return () => {
      listeners.delete(event);
    };
  });
  const hear = (event: string, payload: unknown = null) => {
    const handler = listeners.get(event);
    if (handler === undefined) throw new Error(`nothing listens for ${event}`);
    act(() => handler({ event, id: 0, payload }));
  };
  return {
    choose(command) {
      hear(MENU_EVENTS[command]);
    },
    hear,
    listening: () => [...listeners.keys()].sort(),
  };
}
