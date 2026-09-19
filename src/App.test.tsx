import { describe, expect, it } from "vitest";
import { fireEvent } from "@testing-library/react";
import { renderWithProviders } from "./test/setup";
import App from "./App";

describe("App", () => {
  it("shows the Installed page heading by default", () => {
    const { getByRole } = renderWithProviders(<App />);
    expect(getByRole("heading", { name: "Installed" })).toBeInTheDocument();
  });

  it("switches the content area when a sidebar link is clicked", async () => {
    const { getByRole, findByRole } = renderWithProviders(<App />);

    fireEvent.click(getByRole("button", { name: "Updates" }));

    expect(await findByRole("heading", { name: "Updates" })).toBeInTheDocument();
  });
});
