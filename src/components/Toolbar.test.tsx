import { useRef, useState, type ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { dragsWindow } from "../test/dragRegion";
import { PageHeader } from "./PageHeader";
import { ToolbarItems, ToolbarSlotProvider, useScrollEdge } from "./Toolbar";

/** The toolbar and a page under it, wired as `App` wires them. */
function Window({ actions, children }: { actions?: ReactNode; children: ReactNode }) {
  const [slot, setSlot] = useState<HTMLDivElement | null>(null);
  return (
    <>
      <PageHeader title="Updates" actions={actions} slotRef={setSlot} />
      <ToolbarSlotProvider value={slot}>
        <main>{children}</main>
      </ToolbarSlotProvider>
    </>
  );
}

describe("the toolbar's slot", () => {
  it("draws a page's actions in the toolbar, on the right after Check again", () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const onPress = vi.fn();
    const { getByRole } = renderWithProviders(
      <Window>
        <p>The page</p>
        <ToolbarItems>
          <button type="button" onClick={onPress}>
            Update all
          </button>
        </ToolbarItems>
      </Window>,
    );

    const header = getByRole("banner");
    const updateAll = within(header).getByRole("button", { name: "Update all" });
    // In the toolbar, not in the page it came from...
    expect(within(getByRole("main")).queryByRole("button", { name: "Update all" })).toBeNull();
    // ...after Check again, as a Mac toolbar's items run to its edge.
    expect(within(header).getAllByRole("button").map((button) => button.getAttribute("aria-label") ?? button.textContent)).toEqual([
      "Check Again",
      "Update all",
    ]);
    // Still the page's own button, and a button of the toolbar: pressing
    // it runs the page's handler and drags nothing.
    fireEvent.click(updateAll);
    expect(onPress).toHaveBeenCalledTimes(1);
    expect(dragsWindow(updateAll)).toBe(false);
  });

  it("takes no room while no page puts anything in it", () => {
    const { container } = renderWithProviders(
      <Window actions={null}>
        <p>Settings</p>
      </Window>,
    );
    const slot = container.querySelector("[data-toolbar-slot]") as HTMLElement;
    expect(slot.childElementCount).toBe(0);
    // `:empty` hides it: no gap after Check again for nothing.
    expect(slot.className).toContain("empty:hidden");
  });

  it("goes away with the page that put it there", () => {
    function Switcher() {
      const [page, setPage] = useState<"updates" | "settings">("updates");
      return (
        <Window actions={null}>
          <button type="button" onClick={() => setPage("settings")}>
            Open Settings
          </button>
          {page === "updates" ? (
            <ToolbarItems>
              <button type="button">Update all</button>
            </ToolbarItems>
          ) : null}
        </Window>
      );
    }
    const { getByRole, queryByRole } = renderWithProviders(<Switcher />);
    expect(getByRole("button", { name: "Update all" })).toBeInTheDocument();

    fireEvent.click(getByRole("button", { name: "Open Settings" }));
    expect(queryByRole("button", { name: "Update all" })).toBeNull();
  });

  it("draws nothing where there is no toolbar", () => {
    const { container } = render(
      <ToolbarItems>
        <button type="button">Update all</button>
      </ToolbarItems>,
    );
    expect(container.innerHTML).toBe("");
  });
});

/**
 * A scroller jsdom can scroll: it lays nothing out, so each one says how
 * tall its content and its box are.
 */
function scroller(element: HTMLElement, content: number, box: number) {
  Object.defineProperty(element, "scrollHeight", { configurable: true, value: content });
  Object.defineProperty(element, "clientHeight", { configurable: true, value: box });
}

function scrollTo(element: HTMLElement, top: number) {
  element.scrollTop = top;
  fireEvent.scroll(element);
}

/** A page box with a list that scrolls inside it and a row of chips that scrolls sideways, as the Installed page's. */
function PageBox({ page }: { page: string }) {
  const box = useRef<HTMLDivElement>(null);
  const scrolled = useScrollEdge(box, page);
  return (
    <>
      <p data-testid="edge">{scrolled ? "scrolled" : "at the top"}</p>
      <div ref={box} data-testid="box">
        <div data-testid="chips" />
        <div data-testid="list" />
      </div>
    </>
  );
}

describe("useScrollEdge", () => {
  it("says the page has scrolled once a list inside it has, and not once it is back at the top", () => {
    const { getByTestId } = render(<PageBox page="installed" />);
    const list = getByTestId("list");
    scroller(list, 2000, 500);
    expect(getByTestId("edge")).toHaveTextContent("at the top");

    scrollTo(list, 120);
    expect(getByTestId("edge")).toHaveTextContent("scrolled");

    scrollTo(list, 0);
    expect(getByTestId("edge")).toHaveTextContent("at the top");
  });

  it("counts the page's own box as much as a list in it", () => {
    const { getByTestId } = render(<PageBox page="settings" />);
    const box = getByTestId("box");
    scroller(box, 1500, 600);

    scrollTo(box, 40);
    expect(getByTestId("edge")).toHaveTextContent("scrolled");
  });

  it("pays no heed to a row of chips scrolled sideways", () => {
    const { getByTestId } = render(<PageBox page="installed" />);
    const list = getByTestId("list");
    const chips = getByTestId("chips");
    scroller(list, 2000, 500);
    scroller(chips, 28, 28);

    scrollTo(list, 300);
    fireEvent.scroll(chips);
    expect(getByTestId("edge")).toHaveTextContent("scrolled");
  });

  it("starts a new page at its top, with no hairline", () => {
    const { getByTestId, rerender } = render(<PageBox page="installed" />);
    const list = getByTestId("list");
    scroller(list, 2000, 500);
    scrollTo(list, 300);
    expect(getByTestId("edge")).toHaveTextContent("scrolled");

    rerender(<PageBox page="updates" />);
    expect(getByTestId("edge")).toHaveTextContent("at the top");
  });
});
