import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render } from "@testing-library/react";
import { VirtualList } from "./VirtualList";

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
});
