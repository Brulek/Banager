import { describe, expect, it, vi } from "vitest";
import { fireEvent, waitFor } from "@testing-library/react";
import { renderWithProviders } from "../test/setup";
import { SourceNotice } from "./SourceNotice";

describe("SourceNotice", () => {
  it("renders a title and description with no action button when none is given", () => {
    const { getByText, queryByRole } = renderWithProviders(
      <SourceNotice variant="info" title="Read-only" description="Cannot be changed here." />,
    );
    expect(getByText("Read-only")).toBeInTheDocument();
    expect(getByText("Cannot be changed here.")).toBeInTheDocument();
    expect(queryByRole("button")).not.toBeInTheDocument();
  });

  it("renders and fires the action button when one is given", async () => {
    const onClick = vi.fn();
    const { getByRole } = renderWithProviders(
      <SourceNotice variant="warning" title="Not running" action={{ label: "Open it", onClick }} />,
    );
    fireEvent.click(getByRole("button", { name: "Open it" }));
    await waitFor(() => expect(onClick).toHaveBeenCalledTimes(1));
  });
});
