import { describe, expect, it, vi } from "vitest";
import { renderWithProviders } from "../test/setup";
import { ArtifactRow } from "./ArtifactRow";

describe("ArtifactRow", () => {
  it("renders name, description and badge, and fires the primary action", () => {
    const onPrimaryAction = vi.fn();
    const { getByText, getByRole } = renderWithProviders(
      <ArtifactRow
        name="jq"
        description="Lightweight and flexible command-line JSON processor"
        badgeText="Up to date"
        badgeVariant="neutral"
        primaryActionLabel="Uninstall"
        onPrimaryAction={onPrimaryAction}
      />,
    );

    expect(getByText("jq")).toBeInTheDocument();
    expect(
      getByText("Lightweight and flexible command-line JSON processor"),
    ).toBeInTheDocument();
    expect(getByText("Up to date")).toBeInTheDocument();

    getByRole("button", { name: "Uninstall" }).click();
    expect(onPrimaryAction).toHaveBeenCalledTimes(1);
  });

  it("disables the primary action when primaryActionDisabled is set", () => {
    const { getByRole } = renderWithProviders(
      <ArtifactRow
        name="jq"
        description="desc"
        badgeText="Up to date"
        badgeVariant="neutral"
        primaryActionLabel="Uninstall"
        onPrimaryAction={vi.fn()}
        primaryActionDisabled
      />,
    );

    expect(getByRole("button", { name: "Uninstall" })).toBeDisabled();
  });

  it("renders a checkbox and calls onToggle when selectable", () => {
    const onToggle = vi.fn();
    const { getByRole } = renderWithProviders(
      <ArtifactRow
        name="glib"
        description="desc"
        badgeText="Update"
        badgeVariant="info"
        primaryActionLabel="Update"
        onPrimaryAction={vi.fn()}
        selectable={{ checked: false, onToggle, ariaLabel: "Select glib for update" }}
      />,
    );

    getByRole("checkbox", { name: "Select glib for update" }).click();
    expect(onToggle).toHaveBeenCalledTimes(1);
  });
});
