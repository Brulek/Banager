import { describe, expect, it, vi } from "vitest";
import { renderWithProviders } from "../test/setup";
import { Sidebar } from "./Sidebar";

describe("Sidebar", () => {
  it("renders a button for each page, in order, and marks the active one", () => {
    const onSelectPage = vi.fn();
    const { getByRole, getAllByRole } = renderWithProviders(
      <Sidebar page="installed" onSelectPage={onSelectPage} />,
    );

    const installedButton = getByRole("button", { name: "Installed" });
    const updatesButton = getByRole("button", { name: "Updates" });
    const unknownButton = getByRole("button", { name: "Unknown" });
    const settingsButton = getByRole("button", { name: "Settings" });

    expect(installedButton).toHaveAttribute("aria-current", "page");
    expect(updatesButton).not.toHaveAttribute("aria-current");
    expect(unknownButton).not.toHaveAttribute("aria-current");
    expect(settingsButton).not.toHaveAttribute("aria-current");
    // Unknown sits with the two pages about the machine; Settings stays last.
    expect(getAllByRole("button").map((b) => b.textContent)).toEqual([
      "Installed",
      "Updates",
      "Unknown",
      "Settings",
    ]);
  });

  it("calls onSelectPage with the clicked page", () => {
    const onSelectPage = vi.fn();
    const { getByRole } = renderWithProviders(
      <Sidebar page="installed" onSelectPage={onSelectPage} />,
    );

    getByRole("button", { name: "Updates" }).click();
    expect(onSelectPage).toHaveBeenCalledWith("updates");

    getByRole("button", { name: "Unknown" }).click();
    expect(onSelectPage).toHaveBeenCalledWith("unknown");
  });
});
