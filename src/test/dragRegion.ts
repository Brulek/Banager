/**
 * Whether a press on `target` would start dragging the window, by the rule
 * Tauri's drag script applies in the app (`src/window/scripts/drag.js` in
 * the tauri crate, 2.11): walking up from the element pressed, the first
 * one with a `data-tauri-drag-region` decides -- `"deep"` drags, `"false"`
 * does not, and a bare one (or `"true"`) drags only when it is the
 * element pressed itself -- unless a clickable element with none comes
 * first, which keeps the press for the page. The script is injected into
 * the Tauri window alone, so jsdom never runs it; this copy of its rule
 * lets a test ask what a press on an element would do there.
 */
export function dragsWindow(target: Element): boolean {
  for (let el: Element | null = target; el !== null; el = el.parentElement) {
    // As the script does: an SVG icon inside a button is looked through.
    if (!(el instanceof HTMLElement)) continue;
    const region = el.getAttribute("data-tauri-drag-region");
    if (region === null) {
      if (isClickable(el)) return false;
      continue;
    }
    if (region === "false") return false;
    if (region === "deep") return true;
    if (region === "" || region === "true") return el === target;
  }
  return false;
}

const CLICKABLE_TAGS = new Set(["A", "BUTTON", "INPUT", "SELECT", "TEXTAREA", "LABEL", "SUMMARY"]);
const INTERACTIVE_ROLES = new Set(["button", "link", "menuitem", "tab", "checkbox", "radio", "switch", "option"]);

/** The script's `isClickableElement`. */
function isClickable(el: HTMLElement): boolean {
  const editable = el.getAttribute("contenteditable");
  const tabIndex = el.getAttribute("tabindex");
  return (
    CLICKABLE_TAGS.has(el.tagName) ||
    (editable !== null && editable !== "false") ||
    (tabIndex !== null && tabIndex !== "-1") ||
    INTERACTIVE_ROLES.has(el.getAttribute("role") ?? "")
  );
}
