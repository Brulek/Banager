import { describe, expect, it, vi } from "vitest";
import { renderWithProviders } from "../../test/setup";
import { Dialog } from "./Dialog";

describe("Dialog", () => {
  it("renders the title, children and footer when open", () => {
    const { getByRole, getByText } = renderWithProviders(
      <Dialog
        open
        onOpenChange={vi.fn()}
        title="Confirm"
        footer={<button type="button">OK</button>}
      >
        <p>Body content</p>
      </Dialog>,
    );

    expect(getByRole("dialog")).toBeInTheDocument();
    expect(getByText("Confirm")).toBeInTheDocument();
    expect(getByText("Body content")).toBeInTheDocument();
    expect(getByRole("button", { name: "OK" })).toBeInTheDocument();
  });

  it("does not render when closed", () => {
    const { queryByRole } = renderWithProviders(
      <Dialog open={false} onOpenChange={vi.fn()} title="Confirm">
        <p>Body content</p>
      </Dialog>,
    );

    expect(queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("renders without a footer when none is passed", () => {
    const { getByRole, queryByRole } = renderWithProviders(
      <Dialog open onOpenChange={vi.fn()} title="Confirm">
        <p>Body content</p>
      </Dialog>,
    );

    expect(getByRole("dialog")).toBeInTheDocument();
    expect(queryByRole("button")).not.toBeInTheDocument();
  });
});
