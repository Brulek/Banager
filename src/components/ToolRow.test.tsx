import { describe, expect, it, vi } from "vitest";
import { fireEvent } from "@testing-library/react";
import { renderWithProviders } from "../test/setup";
import { ToolRow } from "./ToolRow";

describe("ToolRow", () => {
  it("shows the source's avatar, the name with its source chip, and one line about it", () => {
    const { container, getByText } = renderWithProviders(
      <ToolRow
        adapterId="brew"
        sourceLabel="Homebrew"
        name="jq"
        nameChip="Homebrew"
        description="Lightweight and flexible command-line JSON processor"
      />,
    );

    const row = container.querySelector("[data-tool-row]") as HTMLElement;
    const avatar = row.querySelector('[aria-hidden="true"]');
    expect(avatar).toHaveTextContent("H");
    expect(avatar?.className).toContain("bg-source-homebrew");
    // 32px: the size a row's avatar is.
    expect(avatar?.className).toContain("h-8");
    expect(getByText("jq").tagName).toBe("P");
    expect(getByText("Homebrew", { selector: "span" })).toBeInTheDocument();
    // One line, cut off at the end, with the whole sentence on hover.
    const blurb = getByText("Lightweight and flexible command-line JSON processor");
    expect(blurb.className).toContain("truncate");
    expect(blurb).toHaveAttribute("title", "Lightweight and flexible command-line JSON processor");
  });

  it("ticks its checkbox through onToggle, named for what it selects", () => {
    const onToggle = vi.fn();
    const { getByRole } = renderWithProviders(
      <ToolRow
        adapterId="brew"
        sourceLabel="Homebrew"
        name="glib"
        description="Core application library for C"
        selectable={{ checked: false, onToggle, ariaLabel: "Select glib for update" }}
      />,
    );

    const checkbox = getByRole("checkbox", { name: "Select glib for update" });
    expect(checkbox).not.toBeChecked();
    fireEvent.click(checkbox);
    expect(onToggle).toHaveBeenCalledTimes(1);
  });

  it("draws a column for every slot it is given, even an empty one, and none for a slot left out", () => {
    const { container, getByRole, getByText, rerender } = renderWithProviders(
      <ToolRow
        adapterId="brew"
        sourceLabel="Homebrew"
        name="glib"
        description="Core application library for C"
        status={<span>Pinned</span>}
        version="2.88.3 → 2.90.0"
        action={<button type="button">Update</button>}
        menu={<button type="button">More</button>}
      />,
    );
    const row = () => container.querySelector("[data-tool-row]") as HTMLElement;
    const columns = () => row().children.length;

    expect(getByText("Pinned")).toBeInTheDocument();
    expect(getByText("2.88.3 → 2.90.0").className).toContain("tabular-nums");
    expect(getByRole("button", { name: "Update" })).toBeInTheDocument();
    expect(getByRole("button", { name: "More" })).toBeInTheDocument();
    const withEverything = columns();

    // `null` keeps the action's column, so this row's version lines up
    // with the rows above that have a button there.
    rerender(
      <ToolRow
        adapterId="brew"
        sourceLabel="Homebrew"
        name="glib"
        description="Core application library for C"
        status={<span>Pinned</span>}
        version="2.88.3 → 2.90.0"
        action={null}
        menu={<button type="button">More</button>}
      />,
    );
    expect(columns()).toBe(withEverything);

    // Left out: no column at all.
    rerender(<ToolRow adapterId="brew" sourceLabel="Homebrew" name="glib" description="Core application library for C" />);
    expect(columns()).toBe(withEverything - 4);
  });
});
