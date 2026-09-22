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

  it("clips the description to one line by default", () => {
    // A package blurb is a nicety next to the name and the badge, so it
    // gets one line and an ellipsis.
    const { getByText } = renderWithProviders(
      <ArtifactRow name="jq" description="a blurb" badgeText="Up to date" badgeVariant="neutral" />,
    );
    expect(getByText("a blurb").className).toContain("truncate");
  });

  it("lets the description wrap when it is an explanation", () => {
    // jsdom does not lay text out, so this asserts the only thing that
    // decides whether the user sees the end of the sentence in the real
    // app: that the single-line clip is off and long unspaced tokens
    // (a URL out of a tool's stderr) are allowed to break.
    const { getByText } = renderWithProviders(
      <ArtifactRow
        name="urllib3"
        description="Canager couldn't check this one for updates just now. pip list --outdated: ERROR: Could not fetch URL https://pypi.org/simple/"
        badgeText="Read-only"
        badgeVariant="neutral"
        wrapDescription
      />,
    );
    const description = getByText(/Could not fetch URL/);
    expect(description.className).not.toContain("truncate");
    expect(description.className).toContain("break-words");
  });

  it("renders no primary action button when the row offers none", () => {
    const { queryByRole } = renderWithProviders(
      <ArtifactRow name="numpy" description="desc" badgeText="Up to date" badgeVariant="neutral" />,
    );
    expect(queryByRole("button")).not.toBeInTheDocument();
  });
});
