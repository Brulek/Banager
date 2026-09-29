import { beforeEach, describe, expect, it, onTestFinished, vi } from "vitest";
import { act, fireEvent, render, waitFor } from "@testing-library/react";
import { useListWidth, VirtualList } from "./VirtualList";
import { useRovingRow } from "./rovingRows";

const ROW = 60;
const VIEWPORT = 600;
const TOOLS = Array.from({ length: 100 }, (_, index) => `tool-${index}`);

const keyOf = (item: string) => item;
const estimate = () => ROW;

beforeEach(() => {
  // @tanstack/react-virtual measures its box and each slot through
  // offsetHeight / offsetWidth, which jsdom hardcodes to 0: the slots carry
  // `data-index`, and the box is the viewport's height.
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (this: HTMLElement) {
    return this.getAttribute("data-index") === null ? VIEWPORT : ROW;
  });
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
});

/** The list's box, scrolled to `top` as a wheel would. */
function scrollTo(box: HTMLElement, top: number) {
  Object.defineProperty(box, "scrollTop", { configurable: true, value: top });
  fireEvent.scroll(box);
}

/** The items handed to `renderItem`, in order, since the mock was last cleared. */
function drawnItems(renderItem: ReturnType<typeof vi.fn>): string[] {
  return renderItem.mock.calls.map(([item]) => item as string);
}

describe("VirtualList", () => {
  it("draws the slots in sight, and on a scroll only those that come into sight", () => {
    const renderItem = vi.fn((item: string) => <p>{item}</p>);
    const { container, getByText, queryByText } = render(
      <VirtualList items={TOOLS} itemKey={keyOf} estimateSize={estimate} renderItem={renderItem} />,
    );
    const box = container.firstElementChild as HTMLElement;

    // Ten rows fill the box, and one more is drawn below it.
    expect(drawnItems(renderItem)).toEqual(TOOLS.slice(0, 11));
    expect(queryByText("tool-11")).not.toBeInTheDocument();

    renderItem.mockClear();
    scrollTo(box, 2 * ROW);

    // Two rows came into sight; the nine still in it are shown as they were.
    expect(drawnItems(renderItem)).toEqual(["tool-11", "tool-12"]);
    expect(getByText("tool-12")).toBeInTheDocument();
    expect(getByText("tool-5")).toBeInTheDocument();
    expect((getByText("tool-12").closest("[data-list-slot]") as HTMLElement).style.transform).toBe(
      `translateY(${12 * ROW}px)`,
    );
  });

  it("draws every slot afresh from a new renderItem, and from new items", () => {
    const first = vi.fn((item: string) => <p>{item}</p>);
    const { rerender, getByText } = render(
      <VirtualList items={TOOLS} itemKey={keyOf} estimateSize={estimate} renderItem={first} />,
    );

    // The page drew again, because something it shows changed.
    const second = vi.fn((item: string) => <p>{`${item} (updated)`}</p>);
    rerender(<VirtualList items={TOOLS} itemKey={keyOf} estimateSize={estimate} renderItem={second} />);
    expect(drawnItems(second)).toEqual(TOOLS.slice(0, 11));
    expect(getByText("tool-3 (updated)")).toBeInTheDocument();

    // The same renderItem over a new list: a search, a filter.
    second.mockClear();
    const found = TOOLS.filter((item) => item.endsWith("7"));
    rerender(<VirtualList items={found} itemKey={keyOf} estimateSize={estimate} renderItem={second} />);
    expect(drawnItems(second)).toEqual(found);
    expect(getByText("tool-97 (updated)")).toBeInTheDocument();
  });

  it("draws a slot that is not reusable every time the list is drawn", () => {
    const renderItem = vi.fn((item: string) => <p>{item}</p>);
    const { container } = render(
      <VirtualList
        items={TOOLS}
        itemKey={keyOf}
        estimateSize={estimate}
        renderItem={renderItem}
        reusable={(item) => item !== "tool-1"}
      />,
    );

    renderItem.mockClear();
    scrollTo(container.firstElementChild as HTMLElement, ROW);

    // tool-1 is still in sight, and drawn again; tool-11 came into sight.
    expect(drawnItems(renderItem).sort()).toEqual(["tool-1", "tool-11"]);
  });

  it("shows what it is handed for an empty list, in the list's box", () => {
    const renderItem = vi.fn((item: string) => <p>{item}</p>);
    const { container, getByText } = render(
      <VirtualList
        items={[]}
        itemKey={keyOf}
        estimateSize={estimate}
        renderItem={renderItem}
        empty={<p>Nothing installed</p>}
      />,
    );

    expect(getByText("Nothing installed").parentElement).toBe(container.firstElementChild);
    expect(container.querySelector("[data-list-slot]")).toBeNull();
    expect(renderItem).not.toHaveBeenCalled();
  });

  it("tells its rows how wide it is, once it has been laid out, and again as the window is resized", () => {
    // jsdom lays nothing out: a width of 0 is no width, which rows read as
    // room for everything.
    const widths: Array<number | null> = [];
    function Row({ item }: { item: string }) {
      widths.push(useListWidth());
      return <p>{item}</p>;
    }
    // The observers made, and what each watches: the list's own, and the
    // virtualizer's.
    const observers: Array<{ callback: ResizeObserverCallback; targets: Element[] }> = [];
    const Observer = globalThis.ResizeObserver;
    globalThis.ResizeObserver = class {
      private readonly watched: { callback: ResizeObserverCallback; targets: Element[] };
      constructor(callback: ResizeObserverCallback) {
        this.watched = { callback, targets: [] };
        observers.push(this.watched);
      }
      observe(target: Element) {
        this.watched.targets.push(target);
      }
      unobserve() {}
      disconnect() {}
    } as unknown as typeof ResizeObserver;
    try {
      const { container } = render(
        <VirtualList items={TOOLS.slice(0, 2)} itemKey={keyOf} estimateSize={estimate} renderItem={(item) => <Row item={item} />} />,
      );
      expect(widths[widths.length - 1]).toBeNull();

      const box = container.firstElementChild as HTMLElement;
      const watching = observers.filter((observer) => observer.targets.includes(box));
      expect(watching.length).toBeGreaterThan(0);
      act(() => {
        for (const observer of watching) {
          observer.callback([{ target: box, contentRect: { width: 591.6 } } as unknown as ResizeObserverEntry], {} as ResizeObserver);
        }
      });
      expect(widths[widths.length - 1]).toBe(592);
    } finally {
      globalThis.ResizeObserver = Observer;
    }
  });
});

describe("VirtualList's arrow keys", () => {
  /** A row that takes part in the arrow keys, as `ToolRow` does, with a checkbox inside it. */
  function KeyRow({ item }: { item: string }) {
    const roving = useRovingRow();
    return (
      <div data-row-focus="" tabIndex={roving?.tabIndex} onFocus={roving?.onFocus} aria-label={item}>
        <input type="checkbox" aria-label={`Select ${item}`} />
      </div>
    );
  }
  // Every fifth slot is a heading, which the arrow keys pass over.
  const heading = (item: string) => Number(item.split("-")[1]) % 5 === 0;
  const renderItem = (item: string) => (heading(item) ? <h2>{item}</h2> : <KeyRow item={item} />);

  function renderKeyList() {
    const result = render(
      <VirtualList
        items={TOOLS}
        itemKey={keyOf}
        estimateSize={estimate}
        renderItem={renderItem}
        keyboardRows={(item) => !heading(item)}
      />,
    );
    const row = (item: string) => result.getByLabelText(item, { selector: "[data-row-focus]" });
    return { ...result, row, box: result.container.firstElementChild as HTMLElement };
  }

  it("keeps one row in the Tab order, the first to begin with, and none of the rest", () => {
    const { row, getByText } = renderKeyList();
    expect(row("tool-1")).toHaveAttribute("tabindex", "0");
    for (const item of ["tool-2", "tool-3", "tool-4", "tool-6"]) expect(row(item)).toHaveAttribute("tabindex", "-1");
    // A heading takes no part.
    expect(getByText("tool-0").closest("[data-list-slot]")?.querySelector("[tabindex]")).toBeNull();
  });

  it("moves the focus to the next row with ↓ and back with ↑, passing over what is not a row", () => {
    const { row } = renderKeyList();
    row("tool-3").focus();

    fireEvent.keyDown(row("tool-3"), { key: "ArrowDown" });
    expect(document.activeElement).toBe(row("tool-4"));
    // tool-5 is a heading.
    fireEvent.keyDown(row("tool-4"), { key: "ArrowDown" });
    expect(document.activeElement).toBe(row("tool-6"));
    fireEvent.keyDown(row("tool-6"), { key: "ArrowUp" });
    expect(document.activeElement).toBe(row("tool-4"));

    // The row the focus is in is the one Tab comes back to.
    expect(row("tool-4")).toHaveAttribute("tabindex", "0");
    expect(row("tool-1")).toHaveAttribute("tabindex", "-1");
  });

  it("moves on from a control inside a row, and makes that row the one in the Tab order", () => {
    const { row, getByLabelText } = renderKeyList();
    const box = getByLabelText("Select tool-2");
    act(() => box.focus());
    expect(row("tool-2")).toHaveAttribute("tabindex", "0");

    const moved = fireEvent.keyDown(box, { key: "ArrowDown" });
    expect(moved).toBe(false);
    expect(document.activeElement).toBe(row("tool-3"));
  });

  it("stops at either end, and leaves the page where it is", () => {
    const { row } = renderKeyList();
    row("tool-1").focus();
    // tool-0 above it is a heading: nothing to move to.
    expect(fireEvent.keyDown(row("tool-1"), { key: "ArrowUp" })).toBe(false);
    expect(document.activeElement).toBe(row("tool-1"));
  });

  it("draws a row out of sight before focusing it", async () => {
    // jsdom scrolls nothing: a scroll to a row moves the box as a browser would.
    const scrollTo = vi.fn(function (this: HTMLElement, options?: ScrollToOptions | number) {
      const top = typeof options === "object" ? (options.top ?? 0) : 0;
      Object.defineProperty(this, "scrollTop", { configurable: true, value: top });
      fireEvent.scroll(this);
    });
    Object.defineProperty(HTMLElement.prototype, "scrollTo", { configurable: true, value: scrollTo });
    onTestFinished(() => {
      delete (HTMLElement.prototype as { scrollTo?: unknown }).scrollTo;
    });
    // How far the box can scroll, which the virtualizer keeps a scroll within.
    vi.spyOn(HTMLElement.prototype, "scrollHeight", "get").mockReturnValue(TOOLS.length * ROW);
    vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(VIEWPORT);
    const { row, queryByLabelText } = renderKeyList();
    // Ten rows fill the box: tool-11 is not drawn yet.
    expect(queryByLabelText("tool-11", { selector: "[data-row-focus]" })).toBeNull();
    row("tool-9").focus();
    fireEvent.keyDown(row("tool-9"), { key: "ArrowDown" });
    // tool-10 is a heading; tool-11 is next, drawn one below the box.
    await waitFor(() => expect(document.activeElement).toBe(queryByLabelText("tool-11", { selector: "[data-row-focus]" })));
  });

  it("leaves ↑ and ↓ to what handles them first, such as an open menu", () => {
    const { row } = renderKeyList();
    row("tool-3").focus();
    const handled = new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true, cancelable: true });
    handled.preventDefault();
    row("tool-3").dispatchEvent(handled);
    expect(document.activeElement).toBe(row("tool-3"));
  });

  it("does nothing with the arrow keys in a list that has not asked for them", () => {
    const { container, getByLabelText } = render(
      <VirtualList items={TOOLS} itemKey={keyOf} estimateSize={estimate} renderItem={(item) => <KeyRow item={item} />} />,
    );
    const first = getByLabelText("tool-1", { selector: "[data-row-focus]" });
    expect(first).not.toHaveAttribute("tabindex");
    const box = getByLabelText("Select tool-1");
    box.focus();
    expect(fireEvent.keyDown(box, { key: "ArrowDown" })).toBe(true);
    expect(document.activeElement).toBe(box);
    expect(container.querySelectorAll('[tabindex="0"]')).toHaveLength(0);
  });
});
