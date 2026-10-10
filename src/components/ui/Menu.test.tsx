import { describe, expect, it, vi } from "vitest";
import { fireEvent, render } from "@testing-library/react";
import { PageHeader } from "../PageHeader";
import { renderWithProviders } from "../../test/setup";
import { createRef } from "react";
import { act } from "@testing-library/react";
import { Menu, RowMenuContext, type MenuItem, type OpenMenuAt } from "./Menu";
import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

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
    // In the tertiary grey, as a Mac menu's item that is off; never lit.
    expect(skip).toHaveClass("aria-disabled:text-tertiary", "aria-disabled:focus:bg-transparent");
    fireEvent.click(skip);
    expect(list[0].onSelect).not.toHaveBeenCalled();
    expect(getByRole("menu")).toBeInTheDocument();
  });

  it("is a macOS menu: at least 180 wide, corners of 10, 5 in, the menu's shadow and no other edge", () => {
    const { button, getByRole } = renderMenu();
    fireEvent.click(button);
    const menu = getByRole("menu");
    for (const look of ["min-w-45", "rounded-group", "p-[5px]", "shadow-menu", "bg-surface"]) {
      expect(menu).toHaveClass(look);
    }
    expect(menu.className).not.toMatch(/\bborder\b|shadow-lg/);
  });

  it("draws its items 22 high in 13, 10 in, lit with the accent and white words, corners of 6, and no fade", () => {
    const { button, getByRole } = renderMenu();
    fireEvent.click(button);
    const item = getByRole("menuitem", { name: "Skip this version" });
    for (const look of ["h-5.5", "text-body", "px-2.5", "rounded-control", "focus:bg-accent", "focus:text-accent-foreground"]) {
      expect(item).toHaveClass(look);
    }
    expect(item.className).not.toMatch(/transition|hover:/);
  });

  it("parts its groups with a hairline, 5 above and below it", () => {
    const { button, getByRole, getAllByRole } = renderMenu(items({ copy: { separatorBefore: true } }));
    fireEvent.click(button);
    const menu = getByRole("menu");
    const separators = getAllByRole("separator");
    expect(separators).toHaveLength(1);
    expect(separators[0]).toHaveClass("h-px", "bg-separator", "my-[5px]");
    // Between Never remind me and Copy command.
    expect(separators[0].previousElementSibling).toHaveTextContent("Never remind me about this software");
    expect(separators[0].nextElementSibling).toHaveTextContent("Copy command");
    expect(menu.firstElementChild).toHaveAttribute("role", "menuitem");
  });

  it("never draws a hairline over its first item", () => {
    const { button, queryByRole } = renderMenu(items({ skip: { separatorBefore: true } }));
    fireEvent.click(button);
    expect(queryByRole("separator")).toBeNull();
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

  it("is always there on a row, and quiet but seen: 24 wide, a grey of its own at rest, no fill", () => {
    const { button } = renderMenu();
    const classes = button.className.split(" ");
    // Not the tertiary grey (1.9:1 on white): it is the one way to Skip
    // this version and Don't remind me.
    expect(classes).toEqual(expect.arrayContaining(["h-6", "w-6", "text-glyph-rest"]));
    expect(classes).not.toContain("text-tertiary");
    expect(button.className).not.toMatch(/(^|\s)(hover:)?bg-/);
    // The muted grey under the pointer, with the focus, on a selected row and while open.
    expect(classes).toEqual(
      expect.arrayContaining([
        "group-hover/row:text-muted",
        "group-focus-within/row:text-muted",
        "group-data-[selected]/row:text-muted",
        "aria-expanded:text-muted",
      ]),
    );
  });

  it("draws its three dots 2.5 across at 16", () => {
    const { button } = renderMenu();
    const glyph = button.querySelector("svg") as SVGElement;
    expect(glyph.getAttribute("width")).toBe("16");
    // A dot is a round-capped stroke of no length: as wide as the stroke,
    // 3.75 of the glyph's 24 units -- 2.5 at 16.
    const dots = glyph.querySelector("path") as SVGPathElement;
    expect(Number(dots.getAttribute("stroke-width")) * (16 / 24)).toBeCloseTo(2.5);
    expect(dots.getAttribute("d")?.match(/h\.01/g)).toHaveLength(3);
  });

  it("has a rest colour of 3:1, the black's 42% and the white's 45%", () => {
    const css = readFileSync(path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../index.css"), "utf-8")
      .replace(/\/\*[\s\S]*?\*\//g, "")
      .replace(/\s+/g, " ");
    expect(css).toContain("--color-glyph-rest: rgb(0 0 0 / 0.42);");
    expect(css).toContain("--color-glyph-rest: rgb(255 255 255 / 0.45);");
  });

  it("hangs from its button moved inside a list too narrow for either edge, as a popover is (r30 Z2)", () => {
    const { getByRole } = render(
      <div data-list="" style={{ overflowY: "auto" }}>
        <Menu label="More actions for glib" items={items()} />
      </div>,
    );
    // A list at 208..540 and the ⋯ at 360..384; the menu 200 wide: from
    // the button's right it would start at 184, from its left end at 560.
    const rects = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockImplementation(function (
      this: HTMLElement,
    ) {
      if (this.tagName === "BUTTON") return new DOMRect(360, 100, 24, 24);
      if (this.dataset.list !== undefined) return new DOMRect(208, 0, 332, 560);
      return new DOMRect(0, 0, 0, 0);
    });
    // All of the list shows rows: it draws no scroll bar.
    const inside = vi.spyOn(HTMLElement.prototype, "clientWidth", "get").mockImplementation(function (this: HTMLElement) {
      return this.dataset.list !== undefined ? 332 : 0;
    });
    const insideHeight = vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockImplementation(function (
      this: HTMLElement,
    ) {
      return this.dataset.list !== undefined ? 560 : 0;
    });
    const width = vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(200);
    const height = vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(90);
    try {
      fireEvent.click(getByRole("button", { name: "More actions for glib" }));
      const menu = getByRole("menu");
      // Still lined up with the button's right, moved 24 right: 208..408.
      expect(menu.className).toContain("right-0");
      expect(menu.style.transform).toBe("translateX(24px)");
    } finally {
      rects.mockRestore();
      inside.mockRestore();
      insideHeight.mockRestore();
      width.mockRestore();
      height.mockRestore();
    }
  });

  it("opens at a point for its row, and turns up and leftwards where the window has no room", () => {
    const opener = createRef<OpenMenuAt | null>() as { current: OpenMenuAt | null };
    const { getByRole } = render(
      <RowMenuContext.Provider value={opener}>
        <Menu label="More actions for glib" items={items()} />
      </RowMenuContext.Provider>,
    );
    const button = getByRole("button", { name: "More actions for glib" });
    const wrapper = button.parentElement as HTMLElement;
    vi.spyOn(wrapper, "getBoundingClientRect").mockReturnValue(new DOMRect(900, 700, 24, 24));
    // The menu's own size, as a browser would lay it out.
    const height = vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(90);
    const width = vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(200);
    try {
      expect(opener.current).not.toBeNull();
      // Near the window's bottom-right corner (jsdom's window is 1024×768).
      act(() => opener.current?.(950, 720));
      const menu = getByRole("menu");
      // Its bottom-right corner at the point instead: 90 up, 200 left.
      expect(menu.style.left).toBe(`${950 - 900 - 200}px`);
      expect(menu.style.top).toBe(`${720 - 700 - 90}px`);
    } finally {
      height.mockRestore();
      width.mockRestore();
    }
  });
});
