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
    // Where it fits, it is not moved, and nor is its arrow.
    expect(panel.style.transform).toBe("");
    expect((panel.querySelector("[data-popover-arrow]") as unknown as HTMLElement).style.left).toBe("");
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

  // The ⓘ beside a list notice in the list column the details panel has
  // narrowed to 332 (208..540) at an 800-wide window: the ⓘ at 365..385,
  // the panel 260 wide. From the ⓘ's middle it would run to 611, and lined
  // up from its right it would start at 139 -- past the column either way
  // (r30 Z2, measured in the mock). The list is 560 high; it shows its rows
  // in all of that unless it draws a scroll bar: `scrollBar` wide down its
  // right side, `scrollBarBelow` high along its foot, as a Mac set to show
  // scroll bars always does. `room` can be changed while the panel is open.
  interface ListRoom {
    left: number;
    right: number;
    scrollBar?: number;
    scrollBarBelow?: number;
  }
  function inNarrowList(
    left: number,
    right: number,
    {
      list = { left: 208, right: 540 },
      align = "start",
      top = 100,
    }: { list?: ListRoom; align?: "start" | "end"; top?: number } = {},
  ) {
    const room: ListRoom = { ...list };
    vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (this: HTMLElement) {
      if (this.tagName === "BUTTON") {
        return { left, right, top, bottom: top + 20, width: right - left, height: 20 } as DOMRect;
      }
      if (this.dataset.list !== undefined) {
        const width = room.right - room.left;
        return { left: room.left, right: room.right, top: 0, bottom: 560, width, height: 560 } as DOMRect;
      }
      return { left: 0, right: 0, top: 0, bottom: 0, width: 0, height: 0 } as DOMRect;
    });
    vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockImplementation(function (this: HTMLElement) {
      return this.dataset.list !== undefined ? room.right - room.left - (room.scrollBar ?? 0) : 0;
    });
    vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockImplementation(function (this: HTMLElement) {
      return this.dataset.list !== undefined ? 560 - (room.scrollBarBelow ?? 0) : 0;
    });
    vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(260);
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(120);
    const { getByRole, container } = render(
      <div data-list="" style={{ overflowY: "auto" }}>
        <p>
          npm couldn't run.{" "}
          <Popover trigger="ⓘ" triggerLabel="Details: npm" triggerClassName="info" align={align}>
            <span>It ran into an error.</span>
            <button type="button">Copy Error Details</button>
          </Popover>
        </p>
      </div>,
    );
    const button = getByRole("button", { name: "Details: npm" });
    fireEvent.click(button);
    const panel = document.getElementById(button.getAttribute("aria-controls") ?? "") as HTMLElement;
    return {
      panel,
      arrow: panel.querySelector("[data-popover-arrow]") as unknown as HTMLElement,
      getByRole,
      button,
      room,
      list: container.querySelector("[data-list]") as HTMLElement,
    };
  }

  it("is moved inside a list too narrow for either edge, so nothing in it is cut off (r30 Z2)", () => {
    const { panel, arrow, getByRole } = inNarrowList(365, 385);
    // Still lined up from the start, then moved 71 left: 351 - 71 = 280,
    // and 280 + 260 = 540, the list's right side.
    expect(panel).toHaveAttribute("data-align", "start");
    expect(panel.className).toContain("left-[calc(50%-24px)]");
    expect(panel.style.transform).toBe("translateX(-71px)");
    // Its button inside it, where the list shows it.
    expect(panel).toContainElement(getByRole("button", { name: "Copy Error Details" }));
    // The arrow moved the other way: its middle 24 + 71 in from the
    // panel's side, at 280 + 95 = 375, the ⓘ's middle.
    expect(arrow.style.left).toBe(`${17 + 71}px`);
    expect(arrow.style.right).toBe("");
  });

  it("is moved rightwards when lined up from the end and too wide for either edge", () => {
    // The same list, the ⓘ at 363..383 (middle 373): from its right the
    // panel would start at 397 - 260 = 137, 71 short of 208.
    const { panel, arrow } = inNarrowList(363, 383, { align: "end" });
    expect(panel).toHaveAttribute("data-align", "end");
    expect(panel.style.transform).toBe("translateX(71px)");
    expect(arrow.style.right).toBe(`${17 + 71}px`);
  });

  it("keeps its start inside a list narrower than itself, its arrow still at the ⓘ", () => {
    // 200 wide: the start, which is read first, wins: 186 - 86 = 100.
    const { panel, arrow } = inNarrowList(200, 220, { list: { left: 100, right: 300 } });
    expect(panel.style.transform).toBe("translateX(-86px)");
    expect(arrow.style.left).toBe(`${17 + 86}px`);
  });

  // A Mac set to show scroll bars always (or with a mouse plugged in)
  // draws a long list's scroll bar inside the list, 15 wide down its right
  // side: the rows end at 525, not 540 (r30 Z2's skeptic, measured in the
  // mock at 800 x 600 in English).
  it("keeps clear of the scroll bar a list draws down its right side", () => {
    const { panel, arrow } = inNarrowList(365, 385, { list: { left: 208, right: 540, scrollBar: 15 } });
    // 351 + 260 = 611, 86 past 525: 351 - 86 = 265, and 265 + 260 = 525.
    expect(panel.style.transform).toBe("translateX(-86px)");
    expect(arrow.style.left).toBe(`${17 + 86}px`);
  });

  it("opens upwards where a scroll bar along the list's foot leaves too little room below", () => {
    // The ⓘ at 410..430, the panel 120 high and 8 from it: below the ⓘ
    // there are 130 to the list's foot at 560, but only 115 above its
    // scroll bar at 545.
    const { panel } = inNarrowList(365, 385, { list: { left: 208, right: 724, scrollBarBelow: 15 }, top: 410 });
    expect(panel).toHaveAttribute("data-side", "above");
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
    for (const look of ["w-65", "rounded-group", "bg-popover", "shadow-menu", "p-3", "text-body-long", "text-foreground"]) {
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

describe("Popover inside a line of text", () => {
  // An ⓘ sits in a sentence, often a <p>: its panel must be phrasing
  // content, or React warns "<div> cannot be a descendant of <p>" (and the
  // HTML parser would close the paragraph early).
  it("logs no DOM nesting warning for a TextWithInfo or a chip's detail lines inside a <p>", async () => {
    const { TextWithInfo } = await import("../InfoDetail");
    const { detailLines } = await import("../updateDetails");
    const errors = vi.spyOn(console, "error").mockImplementation(() => {});
    const { getAllByRole } = render(
      <>
        <p>
          <TextWithInfo text="Checking for updates" label="Details: Checking for updates">
            The list shows what each source has installed.
          </TextWithInfo>
        </p>
        <p>
          <TextWithInfo text="Installed twice" label="Details: Installed twice">
            {detailLines(["npm has a copy too.", "Typing claude in Terminal runs the copy from npm."])}
          </TextWithInfo>
        </p>
      </>,
    );
    for (const button of getAllByRole("button")) fireEvent.click(button);
    const nesting = errors.mock.calls.filter((call) =>
      /cannot be a descendant of|cannot contain a nested|validateDOMNesting|hydration error/i.test(
        call.map(String).join(" "),
      ),
    );
    errors.mockRestore();
    expect(nesting).toEqual([]);
    // Still laid out as a block, and its lines as lines.
    const panels = document.querySelectorAll("[data-side]");
    expect(panels).toHaveLength(2);
    for (const panel of panels) {
      expect(panel.tagName).toBe("SPAN");
      expect(panel.className).toContain("block");
    }
    expect([...document.querySelectorAll("[data-detail-line]")].map((line) => line.textContent)).toEqual([
      "npm has a copy too.",
      "Typing claude in Terminal runs the copy from npm.",
    ]);
  });
});
