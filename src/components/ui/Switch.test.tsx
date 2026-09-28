import { describe, expect, it, vi } from "vitest";
import { renderWithProviders } from "../../test/setup";
import { Switch } from "./Switch";

describe("Switch", () => {
  it("reflects the checked prop and exposes an aria-label", () => {
    const { getByRole } = renderWithProviders(
      <Switch checked onCheckedChange={vi.fn()} aria-label="Show technical details" />,
    );

    expect(getByRole("switch", { name: "Show technical details" })).toBeChecked();
  });

  it("calls onCheckedChange with the new value when clicked", () => {
    const onCheckedChange = vi.fn();
    const { getByRole } = renderWithProviders(
      <Switch checked={false} onCheckedChange={onCheckedChange} aria-label="Greedy casks" />,
    );

    getByRole("switch", { name: "Greedy casks" }).click();
    expect(onCheckedChange).toHaveBeenCalledWith(true);
  });

  it("changes nothing while disabled, and draws itself faded", () => {
    const onCheckedChange = vi.fn();
    const { getByRole } = renderWithProviders(
      <Switch checked={false} onCheckedChange={onCheckedChange} aria-label="Notify me" disabled />,
    );

    const toggle = getByRole("switch", { name: "Notify me" });
    expect(toggle).toBeDisabled();
    expect(toggle.className).toContain("disabled:opacity-50");
    toggle.click();
    expect(onCheckedChange).not.toHaveBeenCalled();
    expect(toggle).not.toBeChecked();
  });
});
