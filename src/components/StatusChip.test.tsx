import { describe, expect, it } from "vitest";
import { fireEvent } from "@testing-library/react";
import { renderWithProviders } from "../test/setup";
import { StatusChip } from "./StatusChip";

/** The classes a class list holds with no state prefix: how it looks at rest. */
function atRest(className: string): string[] {
  return className.split(/\s+/).filter((name) => name !== "" && !name.includes(":"));
}

describe("StatusChip", () => {
  it("is a word, 11 muted in the regular weight: no pill, no fill, no outline", () => {
    const { getByText } = renderWithProviders(<StatusChip label="Pinned" />);
    const word = getByText("Pinned").closest("[data-status-word]") as HTMLElement;
    expect(atRest(word.className)).toEqual(expect.arrayContaining(["text-small", "font-normal", "text-muted"]));
    expect(word.className).not.toMatch(/rounded-full|\bbg-|border|px-|font-medium/);
  });

  it("puts a muted ⓘ after the word when there is a why, and the two are one button, named by the word, that shows it", () => {
    const { getByRole, queryByText } = renderWithProviders(
      <StatusChip label="Can't check" detail={<p>Couldn't find its latest version.</p>} />,
    );
    const button = getByRole("button", { name: "Can't check" });
    expect(atRest(button.className)).toEqual(expect.arrayContaining(["text-small", "text-muted"]));
    expect(button.className).not.toMatch(/rounded-full|\bbg-/);
    // The ⓘ, 12, in the word's colour: the way to the why, which the
    // tertiary grey would not carry.
    const info = button.querySelector("svg") as SVGElement;
    expect(info.getAttribute("width")).toBe("12");
    expect(info.getAttribute("class")).not.toMatch(/tertiary|opacity/);
    expect(queryByText("Couldn't find its latest version.")).toBeNull();
    fireEvent.click(button);
    expect(queryByText("Couldn't find its latest version.")).toBeInTheDocument();
  });

  it("says a failure in the red that reads as text", () => {
    const { getByText } = renderWithProviders(<StatusChip label="Couldn't update" tone="danger" />);
    const word = getByText("Couldn't update").closest("[data-status-word]") as HTMLElement;
    expect(atRest(word.className)).toContain("text-danger-text");
    expect(word.className).not.toContain("text-muted");
  });

  it("marks a warning with a 12pt filled orange ⚠︎ before a muted word", () => {
    const { getByText } = renderWithProviders(<StatusChip label="Result differs" tone="warning" />);
    const word = getByText("Result differs").closest("[data-status-word]") as HTMLElement;
    expect(atRest(word.className)).toContain("text-muted");
    const mark = word.firstElementChild as SVGElement;
    expect(mark.tagName.toLowerCase()).toBe("svg");
    expect(mark.getAttribute("width")).toBe("12");
    expect(mark.getAttribute("class")).toContain("text-warning");
    expect(mark.querySelector("path")?.getAttribute("fill")).toBe("currentColor");
  });

  // walk-4 W4-2: in a place narrower than it, the word is cut short with
  // its symbols kept, and says itself whole in its tooltip.
  it("can be cut short, the word alone, and keeps its whole words in a tooltip", () => {
    const { getByText, getByRole } = renderWithProviders(
      <StatusChip label="Updates when run" detail={<p>Why.</p>} tone="warning" />,
    );
    const label = getByText("Updates when run");
    expect(label.getAttribute("title")).toBe("Updates when run");
    expect(atRest(label.className)).toEqual(expect.arrayContaining(["min-w-0", "truncate"]));
    const button = getByRole("button", { name: "Updates when run" });
    expect(atRest(button.className)).toContain("min-w-0");
    // The ⚠︎ and the ⓘ are not cut.
    for (const mark of Array.from(button.querySelectorAll("svg"))) expect(mark.getAttribute("class")).toContain("shrink-0");
  });
});
