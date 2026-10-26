import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { LOOK_EVERY_MS } from "../lib/homebrewSetup";
import { HomebrewSetupSheet } from "./HomebrewSetupSheet";

const mockInvoke = vi.mocked(invoke);

let looks = 0;

beforeEach(() => {
  vi.useFakeTimers();
  looks = 0;
  mockInvoke.mockReset();
  mockInvoke.mockImplementation(async (cmd: string) => {
    if (cmd === "look_for_homebrew") {
      looks += 1;
      return { there: true, updating: false };
    }
    return null;
  });
});

afterEach(() => {
  vi.useRealTimers();
});

/** Lets `ms` of the sheet's time pass, answering what it asks meanwhile. */
async function pass(ms: number) {
  await act(async () => {
    await vi.advanceTimersByTimeAsync(ms);
  });
}

describe("the Homebrew hand-off's looks", () => {
  it("counts two answers closer than half a look's interval as one look, and moves the focus to Continue once installed", async () => {
    // From the independent review of the branch: removing the guard passed
    // every test, the only one spacing its looks a real two seconds apart.
    const { queryClient } = renderWithProviders(<HomebrewSetupSheet open onClose={() => {}} />);
    await pass(10);
    expect(looks).toBe(1);
    // Another answer a moment later -- one the window's coming back adds:
    // not a second look, so not yet installed.
    await act(async () => {
      await queryClient.refetchQueries({ queryKey: ["lookForHomebrew"] });
    });
    await pass(100);
    expect(looks).toBe(2);
    expect(screen.queryByText("Homebrew is installed.")).toBeNull();
    expect(screen.getByRole("button", { name: "Open Terminal" })).toBeInTheDocument();
    // The interval's next look, two seconds after the first: installed.
    await pass(LOOK_EVERY_MS);
    expect(screen.getByRole("status")).toHaveTextContent("Homebrew is installed.");
    const next = screen.getByRole("button", { name: "Continue" });
    expect(document.activeElement).toBe(next);
  });

  it("has its status there and empty before Homebrew is installed, so that its words are read when they come", () => {
    renderWithProviders(<HomebrewSetupSheet open onClose={() => {}} />);
    const status = screen.getByRole("status");
    expect(status).toBeEmptyDOMElement();
  });
});
