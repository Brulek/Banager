import { describe, expect, it } from "vitest";
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
