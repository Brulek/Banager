import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { suppressBrowserContextMenu } from "./contextMenu";

/** A right-click at (x, y) on `target`; whether the web view would still show its menu. */
function rightClick(target: Element, x = 0, y = 0): boolean {
  const event = new MouseEvent("contextmenu", { bubbles: true, cancelable: true, button: 2, clientX: x, clientY: y });
  return target.dispatchEvent(event);
}

/**
 * The selection, as WebKit leaves it when it has selected the word under
 * the pointer: jsdom lays nothing out, so the word's box is given.
 */
function selectionOver(box: { left: number; top: number; right: number; bottom: number } | null): Selection {
  return {
    isCollapsed: box === null,
    rangeCount: box === null ? 0 : 1,
    getRangeAt: () => ({ getClientRects: () => [box] }),
  } as unknown as Selection;
}

let undo: () => void = () => {};

beforeEach(() => {
  document.body.innerHTML = `
    <h1>Installed</h1>
    <div role="log"><p>==> Pouring jq</p></div>
    <input type="search" aria-label="Search" />
    <textarea aria-label="Notes"></textarea>
    <input type="checkbox" aria-label="Select jq" />
  `;
});

afterEach(() => {
  undo();
  vi.unstubAllEnvs();
  vi.restoreAllMocks();
  document.body.innerHTML = "";
});

function element(selector: string): Element {
  const found = document.querySelector(selector);
  if (found === null) throw new Error(`no ${selector}`);
  return found;
}

describe("the right-click menu in a build", () => {
  beforeEach(() => {
    vi.stubEnv("PROD", true);
    undo = suppressBrowserContextMenu(window);
  });

  it("is not shown on the page: no Reload, no Inspect Element", () => {
    expect(rightClick(element("h1"))).toBe(false);
    expect(rightClick(element("[role=log] p"))).toBe(false);
    expect(rightClick(document.body)).toBe(false);
  });

  it("is a text field's own on a field: Cut, Copy, Paste", () => {
    expect(rightClick(element("input[type=search]"))).toBe(true);
    expect(rightClick(element("textarea"))).toBe(true);
    // A checkbox's would be the page's.
    expect(rightClick(element("input[type=checkbox]"))).toBe(false);
  });

  it("is the selection's on selected text -- Copy, Look Up -- and only on it", () => {
    vi.spyOn(window, "getSelection").mockReturnValue(selectionOver({ left: 10, top: 20, right: 60, bottom: 36 }));
    const line = element("[role=log] p");

    expect(rightClick(line, 30, 28)).toBe(true);
    // Beside the word, where WebKit would show the page's menu.
    expect(rightClick(line, 80, 28)).toBe(false);
    expect(rightClick(line, 30, 40)).toBe(false);
  });

  it("is not shown on text when nothing is selected", () => {
    vi.spyOn(window, "getSelection").mockReturnValue(selectionOver(null));

    expect(rightClick(element("[role=log] p"), 30, 28)).toBe(false);
  });

  it("comes back once undone", () => {
    undo();

    expect(rightClick(element("h1"))).toBe(true);
  });
});

describe("the right-click menu in development", () => {
  it("is left alone everywhere, so Inspect Element is a right-click away", () => {
    // `pnpm tauri dev`, `pnpm tauri:mock` and the browser preview:
    // `import.meta.env.PROD` is false there, as it is under vitest.
    expect(import.meta.env.PROD).toBe(false);
    undo = suppressBrowserContextMenu(window);

    expect(rightClick(element("h1"))).toBe(true);
    expect(rightClick(element("input[type=checkbox]"))).toBe(true);
    expect(rightClick(document.body)).toBe(true);
  });
});
