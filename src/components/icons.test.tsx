import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";
import { SpinnerIcon } from "./icons";

describe("SpinnerIcon", () => {
  it("draws macOS's spinning indicator: eight round spokes, the top one in the caller's colour and each behind it fainter", () => {
    const { container } = render(<SpinnerIcon size={16} className="text-muted" />);
    const svg = container.querySelector("svg") as SVGElement;
    expect(svg).toHaveAttribute("width", "16");
    expect(svg).toHaveAttribute("aria-hidden", "true");
    // The caller's colour, whatever it is: the muted grey in a row.
    expect(svg).toHaveAttribute("stroke", "currentColor");
    expect(svg).toHaveAttribute("stroke-linecap", "round");
    expect(svg.getAttribute("class")).toContain("text-muted");
    // No ring with a gap, as a web page's.
    expect(svg.querySelector("circle, path")).toBeNull();

    const spokes = [...svg.querySelectorAll("line")];
    expect(spokes).toHaveLength(8);
    // Each a short stroke out from the centre, turned an eighth further
    // round than the one before, clockwise from twelve o'clock.
    expect(spokes.map((spoke) => spoke.getAttribute("transform"))).toEqual(
      [0, 45, 90, 135, 180, 225, 270, 315].map((angle) => `rotate(${angle} 12 12)`),
    );
    for (const spoke of spokes) {
      expect(spoke).toHaveAttribute("x1", "12");
      expect(spoke).toHaveAttribute("x2", "12");
      expect(Number(spoke.getAttribute("y1"))).toBeLessThan(Number(spoke.getAttribute("y2")));
    }
    // The top one full, then an eighth fainter going back anticlockwise:
    // the one at half past one, next to light up, the faintest.
    expect(spokes.map((spoke) => Number(spoke.getAttribute("opacity")))).toEqual([
      1, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875,
    ]);
  });

  it("turns in steps, and only for someone who has not asked for less motion", () => {
    const { container } = render(<SpinnerIcon />);
    const classes = (container.querySelector("svg")?.getAttribute("class") ?? "").split(/\s+/);
    expect(classes).toContain("motion-safe:animate-spinner");
    expect(classes).not.toContain("animate-spinner");
    expect(classes).not.toContain("motion-safe:animate-spin");
  });
});
