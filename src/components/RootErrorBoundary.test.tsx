import { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { fakeMenuBar } from "../test/menuBar";
import { dragsWindow } from "../test/dragRegion";
import i18n from "../i18n";
import { QuitQuestion } from "./QuitQuestion";
import { RootErrorBoundary } from "./RootErrorBoundary";

const mockInvoke = vi.mocked(invoke);

/** A part of the page that draws, until a click has it throw. */
function Breaks() {
  const [broken, setBroken] = useState(false);
  if (broken) throw new Error("a row could not be drawn");
  return (
    <button type="button" onClick={() => setBroken(true)}>
      {"break"}
    </button>
  );
}

let error: ReturnType<typeof vi.spyOn>;

beforeEach(() => {
  mockInvoke.mockReset();
  mockInvoke.mockImplementation((cmd: string) => Promise.resolve(cmd === "list_operations" ? [] : undefined));
  // React reports the error it caught; the test only wants the page.
  error = vi.spyOn(console, "error").mockImplementation(() => {});
});

afterEach(() => {
  error.mockRestore();
});

describe("an error in drawing the page", () => {
  it("leaves the page as it is while nothing throws", () => {
    renderWithProviders(
      <RootErrorBoundary reload={() => {}}>
        <Breaks />
      </RootErrorBoundary>,
    );

    expect(screen.getByRole("button", { name: "break" })).toBeInTheDocument();
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("shows one line saying so, and Reload, which loads the page afresh", () => {
    const reload = vi.fn();
    renderWithProviders(
      <RootErrorBoundary reload={reload}>
        <Breaks />
      </RootErrorBoundary>,
    );

    fireEvent.click(screen.getByRole("button", { name: "break" }));

    expect(screen.getByRole("alert")).toHaveTextContent(
      "Couldn't show this page.",
    );
    expect(screen.queryByRole("button", { name: "break" })).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Reload" }));
    expect(reload).toHaveBeenCalledTimes(1);
  });

  it("lets the window be dragged by anything but Reload", () => {
    renderWithProviders(
      <RootErrorBoundary reload={() => {}}>
        <Breaks />
      </RootErrorBoundary>,
    );
    fireEvent.click(screen.getByRole("button", { name: "break" }));

    expect(dragsWindow(screen.getByRole("alert"))).toBe(true);
    expect(dragsWindow(screen.getByRole("alert").parentElement as HTMLElement)).toBe(true);
    expect(dragsWindow(screen.getByRole("button", { name: "Reload" }))).toBe(false);
  });

  it("says so in Chinese", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      renderWithProviders(
        <RootErrorBoundary reload={() => {}}>
          <Breaks />
        </RootErrorBoundary>,
      );
      fireEvent.click(screen.getByRole("button", { name: "break" }));

      expect(screen.getByRole("alert")).toHaveTextContent("无法显示此页面。");
      expect(screen.getByRole("button", { name: "重新载入" })).toBeInTheDocument();
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("tells Rust to stop asking before a quit: the question has gone with the page", async () => {
    fakeMenuBar();
    renderWithProviders(
      <RootErrorBoundary reload={() => {}}>
        <QuitQuestion />
        <Breaks />
      </RootErrorBoundary>,
    );
    await waitFor(() =>
      expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "ask_before_quit")).toEqual([
        ["ask_before_quit", { ask: true }],
      ]),
    );

    fireEvent.click(screen.getByRole("button", { name: "break" }));

    expect(screen.getByRole("alert")).toBeInTheDocument();
    expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "ask_before_quit")).toEqual([
      ["ask_before_quit", { ask: true }],
      ["ask_before_quit", { ask: false }],
    ]);
  });
});
