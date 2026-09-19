import "@testing-library/jest-dom/vitest";
import type { ReactElement, ReactNode } from "react";
import React from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, cleanup } from "@testing-library/react";
import { afterEach, vi } from "vitest";
import { I18nextProvider } from "react-i18next";
import i18n from "../i18n";

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

export function renderWithProviders(ui: ReactElement) {
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
      React.createElement(I18nextProvider, { i18n }, children),
    );
  }

  return {
    queryClient,
    ...render(ui, { wrapper: Wrapper }),
  };
}
