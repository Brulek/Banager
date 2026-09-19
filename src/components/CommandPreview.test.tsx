import { describe, expect, it } from "vitest";
import { renderWithProviders } from "../test/setup";
import { CommandPreview } from "./CommandPreview";

describe("CommandPreview", () => {
  it("renders the joined program and arguments under a label", () => {
    const { getByText } = renderWithProviders(
      <CommandPreview program="/opt/homebrew/bin/brew" args={["upgrade", "--cask", "onyx"]} />,
    );

    expect(getByText("This will run:")).toBeInTheDocument();
    expect(getByText("/opt/homebrew/bin/brew upgrade --cask onyx")).toBeInTheDocument();
  });

  it("quotes tokens that contain whitespace so argument boundaries stay visible", () => {
    const { getByText } = renderWithProviders(
      <CommandPreview program="/Users/Alice Smith/bin/brew" args={["upgrade", "--cask", "onyx"]} />,
    );

    expect(getByText("'/Users/Alice Smith/bin/brew' upgrade --cask onyx")).toBeInTheDocument();
  });
});
