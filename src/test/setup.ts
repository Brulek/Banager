import "@testing-library/jest-dom/vitest";
import type { ReactElement, ReactNode } from "react";
import React from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, cleanup } from "@testing-library/react";
import { afterEach, beforeEach, vi } from "vitest";
import { I18nextProvider } from "react-i18next";
import i18n from "../i18n";
import { loadToolIcons, type ToolIcons } from "../lib/toolIcons";
import { ToolIconsContext } from "../lib/toolIconsContext";
import { DescriptionTableContext, lazyDescriptionTable, type DescriptionTable } from "../lib/toolDescriptions";
import { useUiStore } from "../store/ui";

beforeEach(() => {
  // zustand 5 remembers the state the store was created with; merging it
  // back restores every field and leaves the action functions unchanged.
  useUiStore.setState(useUiStore.getInitialState());
});

afterEach(() => {
  cleanup();
});

// jsdom has no ResizeObserver. @tanstack/react-virtual (Task 11) and Radix
// ScrollArea (Task 13) both use it to measure their container; a no-op stub
// is enough because neither needs a *real* resize callback to run in tests —
// react-virtual takes its first measurement synchronously on mount.
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}

if (!("ResizeObserver" in globalThis)) {
  (globalThis as unknown as { ResizeObserver: typeof ResizeObserverStub }).ResizeObserver =
    ResizeObserverStub;
}

vi.mock("@tauri-apps/api/core", () => {
  class Channel<T = unknown> {
    onmessage: (response: T) => void = () => {};
  }
  return {
    invoke: vi.fn(),
    Channel,
  };
});

// The menu bar's events (`onMenuCommand` in src/lib/api.ts): listened for
// and never heard, unless a test fakes the menu bar (./menuBar.ts).
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

// The Dock's badge (`setDockBadge` in src/lib/api.ts): one window, whose
// badge is set on nothing -- jsdom has no Tauri for the real one to reach
// -- and read back by a test that watches the Dock (./dock.ts).
vi.mock("@tauri-apps/api/window", () => {
  const currentWindow = { setBadgeCount: vi.fn(async () => {}) };
  return { getCurrentWindow: () => currentWindow };
});

// Show in Finder (`revealInFinder` in src/lib/api.ts): shown nowhere --
// jsdom has no Tauri, and no Finder -- and read back by a test that asks
// what it was handed.
vi.mock("@tauri-apps/plugin-opener", () => ({
  revealItemInDir: vi.fn(async () => {}),
}));

/**
 * A logo pack with no logos: what the avatars draw from under
 * `renderWithProviders` unless a test hands it a pack of its own, so that
 * every avatar is its source's coloured initial whatever the built-in
 * pack lists: the reviewed mapping's hundreds of logos, which the next
 * review may change.
 */
const NO_TOOL_ICONS: ToolIcons = loadToolIcons(
  { version: 1, generated: "", glyphs: {}, rasters: {}, tools: {}, sources: {} },
  new Map(),
);

/**
 * A Chinese table with no lines: what the rows read under
 * `renderWithProviders` unless a test hands it a table of its own, so
 * that a row in Chinese says what its source says, whatever lines the
 * built-in table holds.
 */
const NO_DESCRIPTIONS: DescriptionTable = lazyDescriptionTable(async () => ({}));

export interface RenderOptions {
  /** The logos the avatars draw (`loadToolIcons` over a test's own pack); none unless given. */
  toolIcons?: ToolIcons;
  /** The Chinese lines the rows read (`lazyDescriptionTable` over a test's own); none unless given. */
  toolDescriptions?: DescriptionTable;
}

export function renderWithProviders(
  ui: ReactElement,
  { toolIcons = NO_TOOL_ICONS, toolDescriptions = NO_DESCRIPTIONS }: RenderOptions = {},
) {
  const queryClient = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });

  function Wrapper({ children }: { children: ReactNode }) {
    return React.createElement(
      QueryClientProvider,
      { client: queryClient },
      React.createElement(
        I18nextProvider,
        { i18n },
        React.createElement(
          ToolIconsContext.Provider,
          { value: toolIcons },
          React.createElement(DescriptionTableContext.Provider, { value: toolDescriptions }, children),
        ),
      ),
    );
  }

  return {
    queryClient,
    ...render(ui, { wrapper: Wrapper }),
  };
}
