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
    // The enabled look at half opacity, in either appearance: no colour of
    // its own that would fade into the group's grey.
    expect(toggle.className).toContain("disabled:opacity-50");
    expect(toggle.className).not.toMatch(/disabled:(bg|text)-/);
    toggle.click();
    expect(onCheckedChange).not.toHaveBeenCalled();
    expect(toggle).not.toBeChecked();
  });

  it("is a grouped form's switch: a 36 by 16 track, a 20 by 12 knob 2 in from its edges", () => {
    const { getByRole } = renderWithProviders(
      <Switch checked={false} onCheckedChange={vi.fn()} aria-label="Check every day" />,
    );

    const track = getByRole("switch", { name: "Check every day" });
    const trackClasses = track.className.split(" ");
    // w-9 h-4: 36 by 16, round at the ends.
    expect(trackClasses).toEqual(expect.arrayContaining(["w-9", "h-4", "rounded-full"]));
    expect(trackClasses).not.toEqual(expect.arrayContaining(["w-10"]));
    const knob = track.firstElementChild as HTMLElement;
    const knobClasses = knob.className.split(" ");
    // w-5 h-3: 20 by 12, a capsule; 2 in on the left while off, and on the
    // right while on: 36 − 2 − 20 = 14 (translate-x-3.5).
    expect(knobClasses).toEqual(
      expect.arrayContaining(["w-5", "h-3", "rounded-full", "translate-x-0.5", "data-[state=checked]:translate-x-3.5"]),
    );
    // No heavy drop shadow under the knob: a hairline edge and the
    // slightest shadow, so it reads on the light track.
    expect(knob.className).not.toMatch(/shadow-(sm|md|lg)/);
    expect(knob.className).toContain("shadow-[0_0_0_0.5px_rgb(0_0_0/0.12),0_1px_1.5px_rgb(0_0_0/0.18)]");
  });

  it("is the switch-off grey while off and the accent while on", () => {
    const { getByRole, rerender } = renderWithProviders(
      <Switch checked={false} onCheckedChange={vi.fn()} aria-label="Check every day" />,
    );

    const track = getByRole("switch", { name: "Check every day" });
    expect(track.className).toContain("bg-switch-off");
    expect(track.className).toContain("data-[state=checked]:bg-accent");
    expect(track).toHaveAttribute("data-state", "unchecked");
    rerender(<Switch checked onCheckedChange={vi.fn()} aria-label="Check every day" />);
    expect(track).toHaveAttribute("data-state", "checked");
    expect(track.firstElementChild).toHaveAttribute("data-state", "checked");
  });
});
