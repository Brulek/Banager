import { describe, expect, it, vi } from "vitest";
import { fireEvent, render } from "@testing-library/react";
import { PageHeader } from "../PageHeader";
import { renderWithProviders } from "../../test/setup";
import { Menu, type MenuItem } from "./Menu";

function items(overrides: Partial<Record<string, Partial<MenuItem>>> = {}): MenuItem[] {
  return [
    { id: "skip", label: "Skip this version", hint: "You'll be reminded again when its next version is out.", onSelect: vi.fn() },
    { id: "never", label: "Never remind me about this software", onSelect: vi.fn() },
    { id: "copy", label: "Copy command", onSelect: vi.fn() },
  ].map((item) => ({ ...item, ...overrides[item.id] }));
}

function renderMenu(list: MenuItem[] = items()) {
  const result = render(
    <div>
      <Menu label="More actions for glib" items={list} />
      <button type="button">Elsewhere</button>
    </div>,
  );
  const button = result.getByRole("button", { name: "More actions for glib" });
  return { ...result, button, list };
}

describe("Menu", () => {
  it("is a menu button: it says it has a menu, and opening it puts focus on the first item", () => {
    const { button, getByRole, queryByRole } = renderMenu();
    expect(button).toHaveAttribute("aria-haspopup", "menu");
    expect(button).toHaveAttribute("aria-expanded", "false");
    expect(queryByRole("menu")).toBeNull();

    fireEvent.click(button);

    const menu = getByRole("menu", { name: "More actions for glib" });
    expect(button).toHaveAttribute("aria-expanded", "true");
    expect(button).toHaveAttribute("aria-controls", menu.id);
    expect(document.activeElement).toBe(getByRole("menuitem", { name: "Skip this version" }));
    // Each item says what it does.
    expect(getByRole("menuitem", { name: "Skip this version" })).toHaveAccessibleDescription(
      "You'll be reminded again when its next version is out.",
    );
  });

  it("moves between the items with the arrow keys, Home and End, and wraps round", () => {
    const { button, getByRole } = renderMenu();
    fireEvent.click(button);
    const menu = getByRole("menu");
    const [skip, never, copy] = ["Skip this version", "Never remind me about this software", "Copy command"].map(
      (name) => getByRole("menuitem", { name }),
    );

    fireEvent.keyDown(menu, { key: "ArrowDown" });
    expect(document.activeElement).toBe(never);
    fireEvent.keyDown(menu, { key: "ArrowDown" });
    fireEvent.keyDown(menu, { key: "ArrowDown" });
    expect(document.activeElement).toBe(skip);
    fireEvent.keyDown(menu, { key: "ArrowUp" });
    expect(document.activeElement).toBe(copy);
    fireEvent.keyDown(menu, { key: "Home" });
    expect(document.activeElement).toBe(skip);
    fireEvent.keyDown(menu, { key: "End" });
    expect(document.activeElement).toBe(copy);
  });

  it("opens from the keyboard: ↓ at the first item, ↑ at the last", () => {
    const { button, getByRole } = renderMenu();
    fireEvent.keyDown(button, { key: "ArrowUp" });
    expect(document.activeElement).toBe(getByRole("menuitem", { name: "Copy command" }));

    fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });
    fireEvent.keyDown(button, { key: "ArrowDown" });
    expect(document.activeElement).toBe(getByRole("menuitem", { name: "Skip this version" }));
  });

  it("does what an item says when it is chosen, then closes and puts focus back on the button", () => {
    const { button, getByRole, queryByRole, list } = renderMenu();
    fireEvent.click(button);
    fireEvent.click(getByRole("menuitem", { name: "Never remind me about this software" }));

    expect(list[1].onSelect).toHaveBeenCalledTimes(1);
    expect(list[0].onSelect).not.toHaveBeenCalled();
    expect(queryByRole("menu")).toBeNull();
    expect(document.activeElement).toBe(button);
  });

  it("closes on Escape, with focus back on the button, and on a click outside", () => {
    const { button, queryByRole } = renderMenu();
    fireEvent.click(button);
    fireEvent.keyDown(document.activeElement ?? document.body, { key: "Escape" });
    expect(queryByRole("menu")).toBeNull();
    expect(document.activeElement).toBe(button);

    fireEvent.click(button);
    fireEvent.mouseDown(document.body);
    expect(queryByRole("menu")).toBeNull();
  });

  it("leaves on Tab, closed, from its button", () => {
    const { button, getByRole, queryByRole } = renderMenu();
    fireEvent.click(button);
    fireEvent.keyDown(getByRole("menu"), { key: "Tab" });
    expect(queryByRole("menu")).toBeNull();
    expect(document.activeElement).toBe(button);
  });

  it("shows a disabled item, which does nothing when chosen", () => {
    const { button, getByRole, list } = renderMenu(items({ skip: { disabled: true } }));
    fireEvent.click(button);
    const skip = getByRole("menuitem", { name: "Skip this version" });

    expect(skip).toHaveAttribute("aria-disabled", "true");
    fireEvent.click(skip);
    expect(list[0].onSelect).not.toHaveBeenCalled();
    expect(getByRole("menu")).toBeInTheDocument();
  });

  it("gives the focus to the page's title when its row goes after an item is chosen", () => {
    // ⋯ → Skip this version: the row, and its ⋯ button, leave the list.
    const page = (shown: boolean) => (
      <>
        <PageHeader title="Updates" actions={null} />
        {shown ? <Menu label="More actions for glib" items={items()} /> : null}
        <button type="button">Elsewhere</button>
      </>
    );
    const { getByRole, rerender } = renderWithProviders(page(true));
    const button = getByRole("button", { name: "More actions for glib" });
    fireEvent.click(button);
    fireEvent.click(getByRole("menuitem", { name: "Skip this version" }));
    expect(document.activeElement).toBe(button);

    rerender(page(false));

    expect(document.activeElement).toBe(getByRole("heading", { name: "Updates" }));
  });

  it("leaves the focus where it is when its row goes while something else has it", () => {
    const page = (shown: boolean) => (
      <>
        <PageHeader title="Updates" actions={null} />
        {shown ? <Menu label="More actions for glib" items={items()} /> : null}
        <button type="button">Elsewhere</button>
      </>
    );
    const { getByRole, rerender } = renderWithProviders(page(true));
    const elsewhere = getByRole("button", { name: "Elsewhere" });
    elsewhere.focus();

    rerender(page(false));

    expect(document.activeElement).toBe(elsewhere);
  });
});
