import { useState } from "react";
import { describe, expect, it } from "vitest";
import { act, screen, waitFor } from "@testing-library/react";
import { renderWithProviders } from "../../test/setup";
import { focusLost, refocusWhenGone } from "./focus";

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
