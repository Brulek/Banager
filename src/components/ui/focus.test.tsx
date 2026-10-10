import { useState } from "react";
import { describe, expect, it, onTestFinished, vi } from "vitest";
import { act, screen, waitFor } from "@testing-library/react";
import { renderWithProviders } from "../../test/setup";
import { focusLost, inSightOfList, refocusWhenGone } from "./focus";

/**
 * A page with a title, a notice that can go, and a button that stays. The
 * ref is a new function at each draw, as an inline one is: React lets go
 * of the old one while the notice stays, which must move no focus.
 */
function Page() {
  const [shown, setShown] = useState(true);
  const [count, setCount] = useState(0);
  return (
    <>
      <h1 data-focus-fallback="" tabIndex={-1}>
        Overview
      </h1>
      {shown ? (
        <div ref={(node) => refocusWhenGone(node)} data-count={count}>
          <button type="button" onClick={() => setCount(count + 1)}>
            Open Ollama
          </button>
        </div>
      ) : null}
      <button type="button" onClick={() => setShown(false)}>
        Elsewhere
      </button>
      <button type="button" data-testid="go" onClick={() => setShown(false)} hidden />
    </>
  );
}

describe("refocusWhenGone", () => {
  it("gives the focus to the page's title once it has gone with the focus in it, not to the body", async () => {
    renderWithProviders(<Page />);
    const open = screen.getByRole("button", { name: "Open Ollama" });
    act(() => open.focus());

    act(() => screen.getByTestId("go").click());

    await waitFor(() => expect(open.isConnected).toBe(false));
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("heading", { name: "Overview" })));
    expect(focusLost()).toBe(false);
  });

  it("moves no focus when it is drawn again, rather than gone", async () => {
    renderWithProviders(<Page />);
    const open = screen.getByRole("button", { name: "Open Ollama" });
    act(() => open.focus());

    act(() => open.click());
    await act(async () => {
      await Promise.resolve();
    });

    expect(document.activeElement).toBe(open);
  });

  it("moves no focus when the focus was elsewhere as it went", async () => {
    renderWithProviders(<Page />);
    const elsewhere = screen.getByRole("button", { name: "Elsewhere" });
    act(() => elsewhere.focus());

    act(() => elsewhere.click());
    await waitFor(() => expect(screen.queryByRole("button", { name: "Open Ollama" })).toBeNull());
    await act(async () => {
      await Promise.resolve();
    });

    expect(document.activeElement).toBe(elsewhere);
  });
});

/**
 * The same notice as the first slot of a list's box (`VirtualList`), as
 * the Updates and Installed pages draw their notices.
 */
function ListPage() {
  const [shown, setShown] = useState(true);
  return (
    <>
      <h1 data-focus-fallback="" tabIndex={-1}>
        Updates
      </h1>
      <div data-list="">
        {shown ? (
          <div data-list-slot="">
            <div ref={refocusWhenGone}>
              <button type="button">Check Again</button>
            </div>
          </div>
        ) : null}
      </div>
      <button type="button" data-testid="go" onClick={() => setShown(false)} hidden />
    </>
  );
}

/** Lays the list out as a browser would: the box 600 high, the slot `slotTop` from its top as it is scrolled now. */
function layOut(slotTop: number) {
  const layout = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
    const rect = (top: number, height: number) =>
      ({ top, bottom: top + height, left: 0, right: 800, width: 800, height, x: 0, y: top }) as DOMRect;
    if (this.hasAttribute("data-list")) return rect(0, 600);
    if (this.hasAttribute("data-list-slot")) return rect(slotTop, 32);
    return rect(0, 0);
  });
  onTestFinished(() => layout.mockRestore());
}

describe("refocusWhenGone, in a list", () => {
  it("gives the focus to the page's title once it has gone in sight", async () => {
    layOut(0);
    renderWithProviders(<ListPage />);
    const again = screen.getByRole("button", { name: "Check Again" });
    act(() => again.focus());

    act(() => screen.getByTestId("go").click());

    await waitFor(() => expect(again.isConnected).toBe(false));
    await waitFor(() => expect(document.activeElement).toBe(screen.getByRole("heading", { name: "Updates" })));
  });

  it("moves no focus once it has gone out of sight, the list scrolled away from it", async () => {
    layOut(-40);
    renderWithProviders(<ListPage />);
    const again = screen.getByRole("button", { name: "Check Again" });
    act(() => again.focus());

    act(() => screen.getByTestId("go").click());
    await waitFor(() => expect(again.isConnected).toBe(false));
    await act(async () => {
      await Promise.resolve();
    });

    expect(document.activeElement).toBe(document.body);
  });
});

describe("inSightOfList", () => {
  it("counts what is in no list as in sight, and everything where nothing is laid out", () => {
    renderWithProviders(
      <>
        <button type="button">Alone</button>
        <div data-list="">
          <div data-list-slot="">
            <button type="button">Listed</button>
          </div>
        </div>
      </>,
    );
    expect(inSightOfList(screen.getByRole("button", { name: "Alone" }))).toBe(true);
    // jsdom lays nothing out: the box has no height.
    expect(inSightOfList(screen.getByRole("button", { name: "Listed" }))).toBe(true);
  });
});

describe("focusLost", () => {
  it("is true while the body has the focus, and not while anything else has it", () => {
    renderWithProviders(<Page />);
    act(() => (document.activeElement as HTMLElement | null)?.blur());
    expect(document.activeElement).toBe(document.body);
    expect(focusLost()).toBe(true);
    act(() => screen.getByRole("button", { name: "Elsewhere" }).focus());
    expect(focusLost()).toBe(false);
  });
});
