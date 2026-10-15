import { createRef } from "react";
import { fireEvent, render } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ScrollArea } from "./ScrollArea";

describe("ScrollArea", () => {
  it("leaves its viewport out of the Tab order and unnamed unless given a name", () => {
    const { container } = render(
      <ScrollArea>
        <p>Text</p>
      </ScrollArea>,
    );
    const viewport = container.querySelector("[data-radix-scroll-area-viewport]") as HTMLElement;
    expect(viewport).not.toHaveAttribute("tabindex");
    expect(viewport).not.toHaveAttribute("role");
    expect(viewport).not.toHaveAttribute("aria-label");
  });

  it("makes the viewport itself, what scrolls, a named Tab stop when given a name", () => {
    const ref = createRef<HTMLDivElement>();
    const onViewportScroll = vi.fn();
    const { container, getByRole } = render(
      <ScrollArea ref={ref} viewportLabel="Operation log" onViewportScroll={onViewportScroll}>
        <p>Text</p>
      </ScrollArea>,
    );
    const viewport = container.querySelector("[data-radix-scroll-area-viewport]") as HTMLElement;
    expect(getByRole("region", { name: "Operation log" })).toBe(viewport);
    expect(viewport).toHaveAttribute("tabindex", "0");
    expect(viewport.className).toContain("focus-visible:outline-offset-[-3px]");
    // Still the one the ref and the scroll handler see.
    expect(ref.current).toBe(viewport);
    fireEvent.scroll(viewport);
    expect(onViewportScroll).toHaveBeenCalledTimes(1);
  });
});
