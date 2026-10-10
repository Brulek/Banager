import { expect, it, vi } from "vitest";
import { ACCENT_INK_ATTRIBUTE } from "./lib/accentInk";

// The window's start as main.tsx runs it, with the page drawn as nothing:
// what this checks is what main.tsx sets up before the first page, which
// nothing else runs -- src/lib/accentInk.test.ts tests the watcher alone.
vi.mock("./App", () => ({ default: () => null }));

it("marks the window for black words on a light accent as it starts (decision I21b)", async () => {
  document.body.innerHTML = '<div id="root"></div>';
  document.documentElement.removeAttribute(ACCENT_INK_ATTRIBUTE);
  // The accent the page computes: macOS 27's yellow, as AppKit draws it in
  // the dark appearance. jsdom computes no `AccentColor`, so it is given
  // where the watcher reads it, a probe drawn in the accent.
  const computed = window.getComputedStyle.bind(window);
  vi.spyOn(window, "getComputedStyle").mockImplementation((element, pseudo) =>
    element instanceof HTMLElement && element.style.color === "var(--color-accent)"
      ? ({ color: "rgb(255, 198, 0)" } as CSSStyleDeclaration)
      : computed(element, pseudo),
  );

  await import("./main");

  expect(document.documentElement.getAttribute(ACCENT_INK_ATTRIBUTE)).toBe("dark");
});
