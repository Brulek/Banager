import { afterEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render } from "@testing-library/react";
import { Popover } from "./Popover";
import { StatusChip } from "../StatusChip";

function renderTwo() {
  return render(
    <div>
      <Popover trigger="Pinned" triggerClassName="chip">
        <p>It's pinned in Homebrew.</p>
      </Popover>
      <Popover trigger="Read-only" triggerClassName="chip">
        <p>Packages installed with pip can only be viewed here.</p>
      </Popover>
      <button type="button">Elsewhere</button>
    </div>,
  );
}

describe("Popover", () => {
  it("is a button that says whether it is open and which panel it opens, with the panel right after it", () => {
    const { getByRole, queryByText } = renderTwo();
    const pinned = getByRole("button", { name: "Pinned" });

    expect(pinned.tagName).toBe("BUTTON");
    expect(pinned).toHaveAttribute("aria-expanded", "false");
    expect(pinned).not.toHaveAttribute("aria-controls");
    expect(queryByText("It's pinned in Homebrew.")).toBeNull();

    fireEvent.click(pinned);

    expect(pinned).toHaveAttribute("aria-expanded", "true");
    const panel = document.getElementById(pinned.getAttribute("aria-controls") ?? "");
    expect(panel).toHaveTextContent("It's pinned in Homebrew.");
    // Next in reading order: a screen reader finds it straight after the button.
    expect(pinned.nextElementSibling).toBe(panel);
    // Its wrapper says a panel is open, which lifts the list slot it is in.
    expect(pinned.parentElement).toHaveAttribute("data-popup-open");

    fireEvent.click(pinned);
    expect(pinned).toHaveAttribute("aria-expanded", "false");
    expect(queryByText("It's pinned in Homebrew.")).toBeNull();
  });

  it("closes on Escape and puts focus back on its button", () => {
    const { getByRole, queryByText } = renderTwo();
    const pinned = getByRole("button", { name: "Pinned" });
    act(() => pinned.focus());
    fireEvent.click(pinned);
    expect(queryByText("It's pinned in Homebrew.")).not.toBeNull();

    act(() => getByRole("button", { name: "Elsewhere" }).focus());
    // Focus moved away: that closed it already. Open it again, from the keyboard's place.
    act(() => pinned.focus());
    fireEvent.click(pinned);
    fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });

    expect(queryByText("It's pinned in Homebrew.")).toBeNull();
    expect(document.activeElement).toBe(pinned);
  });

  it("closes on a click outside, and stays open for a click on its own text", () => {
    const { getByRole, getByText, queryByText } = renderTwo();
    fireEvent.click(getByRole("button", { name: "Pinned" }));

    fireEvent.mouseDown(getByText("It's pinned in Homebrew."));
    expect(queryByText("It's pinned in Homebrew.")).not.toBeNull();

    fireEvent.mouseDown(document.body);
    expect(queryByText("It's pinned in Homebrew.")).toBeNull();
  });

  it("closes when focus moves to another control, so two are never open at once", () => {
    const { getByRole, queryByText } = renderTwo();
    const pinned = getByRole("button", { name: "Pinned" });
    const readOnly = getByRole("button", { name: "Read-only" });

    act(() => pinned.focus());
    fireEvent.click(pinned);
    act(() => readOnly.focus());
    fireEvent.click(readOnly);

    expect(queryByText("It's pinned in Homebrew.")).toBeNull();
    expect(queryByText("Packages installed with pip can only be viewed here.")).not.toBeNull();
    expect(pinned).toHaveAttribute("aria-expanded", "false");
  });
});

describe("Popover placement", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  // A button at `left`..`right` on a 1024 x 768 window, and a panel 256px
  // wide and 80px tall -- what a browser would measure.
  function layOut(left: number, right: number, top = 100) {
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (
      this: HTMLElement,
    ) {
      return this.tagName === "BUTTON"
        ? ({ left, right, top, bottom: top + 20, width: right - left, height: 20 } as DOMRect)
        : ({ left: 0, right: 0, top: 0, bottom: 0, width: 0, height: 0 } as DOMRect);
    });
    vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(256);
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(80);
    vi.spyOn(window, "innerWidth", "get").mockReturnValue(1024);
    vi.spyOn(window, "innerHeight", "get").mockReturnValue(768);
  }

  function openPanel(align: "start" | "end") {
    const { getByRole } = render(
      <Popover trigger="Details" triggerClassName="details" align={align}>
        <p>More.</p>
      </Popover>,
    );
    const button = getByRole("button", { name: "Details" });
    fireEvent.click(button);
    return document.getElementById(button.getAttribute("aria-controls") ?? "") as HTMLElement;
  }

  // The panel stands 24 to the side of the button's middle, its arrow's.
  it("opens rightwards from the button when it fits", () => {
    layOut(100, 160);
    const panel = openPanel("start");
    expect(panel).toHaveAttribute("data-align", "start");
    expect(panel.className).toContain("left-[calc(50%-24px)]");
  });

  it("opens leftwards from near the window's right edge instead of past it", () => {
    layOut(900, 960);
    const panel = openPanel("start");
    expect(panel).toHaveAttribute("data-align", "end");
    expect(panel.className).toContain("right-[calc(50%-24px)]");
  });

  it("opens rightwards from near the window's left edge when asked to line up with the button's right", () => {
    layOut(20, 80);
    expect(openPanel("end")).toHaveAttribute("data-align", "start");
  });

  it("opens upwards at the foot of the window, and downwards where there is room", () => {
    layOut(100, 160, 720);
    const panel = openPanel("start");
    expect(panel).toHaveAttribute("data-side", "above");
    expect(panel.className).toContain("bottom-full");
  });

  it("is a macOS popover: 260 wide, the corners of a group, the menu's shadow, 12 in, 13/18, no edge", () => {
    layOut(100, 160);
    const panel = openPanel("start");
    for (const look of ["w-65", "rounded-group", "bg-surface", "shadow-menu", "p-3", "text-body-long", "text-foreground"]) {
      expect(panel).toHaveClass(look);
    }
    expect(panel.className).not.toMatch(/\bborder\b|shadow-lg/);
  });

  it("has a 14 by 7 arrow on the side facing the button, pointing at it", () => {
    layOut(100, 160);
    const below = openPanel("start");
    const arrow = below.querySelector("[data-popover-arrow]") as SVGElement;
    expect(arrow).not.toBeNull();
    expect(arrow).toHaveAttribute("width", "14");
    expect(arrow).toHaveAttribute("height", "7");
    // Under the button: on the panel's top edge, pointing up, 17 in (its
    // middle 24 in, where the button's middle is).
    expect(arrow.getAttribute("class")).toMatch(/\bbottom-full\b/);
    expect(arrow.getAttribute("class")).not.toMatch(/rotate-180/);
    expect(arrow.getAttribute("class")).toMatch(/left-\[17px\]/);
  });

  it("turns its arrow down when it opens above the button", () => {
    layOut(900, 960, 720);
    const above = openPanel("start");
    const arrow = above.querySelector("[data-popover-arrow]") as SVGElement;
    expect(arrow.getAttribute("class")).toMatch(/\btop-full\b/);
    expect(arrow.getAttribute("class")).toMatch(/rotate-180/);
    // Lined up from the right, its arrow is too.
    expect(arrow.getAttribute("class")).toMatch(/right-\[17px\]/);
  });
});

describe("StatusChip", () => {
  it("is a plain label without a detail, and a button that shows it with one", () => {
    const { getByText, getByRole, queryByRole, rerender } = render(<StatusChip label="Pinned" />);
    expect(getByText("Pinned").tagName).toBe("SPAN");
    expect(queryByRole("button")).toBeNull();

    rerender(<StatusChip label="Pinned" detail="It's pinned in Homebrew." />);
    const chip = getByRole("button", { name: "Pinned" });
    // The ⓘ is drawn, and hidden from assistive technology: the word is the name.
    expect(chip.querySelector("svg")).toHaveAttribute("aria-hidden", "true");
    fireEvent.click(chip);
    expect(getByText("It's pinned in Homebrew.")).toBeInTheDocument();
  });
});
