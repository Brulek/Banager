import {
  createContext,
  Fragment,
  useCallback,
  useContext,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type RefObject,
} from "react";
import { MoreIcon } from "../icons";
import { panelBounds, useDismiss, usePlacement } from "./floating";
import { focusOrFallback } from "./focus";

export interface MenuItem {
  /** Stable React key. */
  id: string;
  label: string;
  /** What choosing it does: its tooltip, and its accessible description. */
  hint?: string;
  /** Shown, and reachable with the arrow keys, but choosing it does nothing. */
  disabled?: boolean;
  /**
   * It starts a group of its own: a hairline over it, as a Mac menu parts
   * what does different kinds of things -- Skip and Stop Reminding Me from
   * Copy Command.
   */
  separatorBefore?: boolean;
  onSelect: () => void;
}

export interface MenuProps {
  /** The ⋯ button's accessible name, e.g. "More actions for glib". */
  label: string;
  items: MenuItem[];
}

/** Opens a row's menu with its top-left corner at a point of the window (`clientX`, `clientY`). */
export type OpenMenuAt = (x: number, y: number) => void;

/**
 * Where a row (`ToolRow`) keeps the way to open its ⋯ menu at the
 * pointer, for a right-click anywhere on it: the `Menu` inside the row
 * puts its `OpenMenuAt` here while it is drawn. Null outside a row.
 */
export const RowMenuContext = createContext<RefObject<OpenMenuAt | null> | null>(null);

/**
 * The ⋯ button's look on a row (spec R4): always there, so a row's other
 * actions are never a surprise, but quiet -- a grey of its own at rest
 * (`glyph-rest`, 3:1: the only way to Skip this version or Don't remind
 * me, it must be seen, where the tertiary grey's 1.9:1 hid it), the muted
 * grey while the row is under the pointer, holds the focus or is
 * selected, or the menu is open -- and never a fill. 24 wide, the glyph
 * at 16; white on a selection in the accent (index.css).
 */
const ROW_TRIGGER =
  "inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-control text-glyph-rest group-hover/row:text-muted group-focus-within/row:text-muted group-data-[selected]/row:text-muted aria-expanded:text-muted [&>svg]:size-4";

/** Where a menu opened at a point sits: from the ⋯ button's box, as `left`/`top` offsets. */
interface PointPlacement {
  left: number;
  top: number;
}

/** The items of an open menu, in order. */
function itemsOf(menu: HTMLElement | null): HTMLElement[] {
  if (menu === null) return [];
  return Array.from(menu.querySelectorAll<HTMLElement>('[role="menuitem"]'));
}

/**
 * The ⋯ button at the end of a row and the menu it opens: the WAI-ARIA
 * menu button. The button says it has a menu (`aria-haspopup`) and whether
 * it is open; opening it -- a click, Enter, Space or ↓ -- puts focus on the
 * first item (↑: the last), the arrow keys, Home and End move between the
 * items, Enter or Space chooses one, and Escape closes the menu and puts
 * focus back on the button. Tab leaves it, closed. A disabled item stays
 * in the list and says so (`aria-disabled`); choosing it does nothing.
 *
 * Inside a row (`RowMenuContext`), a right-click anywhere on the row opens
 * the same menu at the pointer, as a Mac list's context menu opens: its
 * corner at the point, turned up or left where the list has no room, and
 * no item highlighted until the pointer or an arrow key picks one -- the
 * focus is on the menu itself, so ↓ goes to the first item.
 */
export function Menu({ label, items }: MenuProps) {
  const [open, setOpen] = useState(false);
  const [startAt, setStartAt] = useState<"first" | "last" | "menu">("first");
  // Set while the menu is open at a point, null while it hangs from the button.
  const [point, setPoint] = useState<PointPlacement | null>(null);
  const buttonId = useId();
  const menuId = useId();
  const wrapperRef = useRef<HTMLSpanElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const close = useCallback(() => setOpen(false), []);
  useDismiss(open, close, wrapperRef, triggerRef);
  const placement = usePlacement(open && point === null, triggerRef, menuRef, "end");

  // Opens at (x, y) in the window: the offsets from the button's own box,
  // which the menu is placed in, since a list's slot is moved by a
  // transform and `fixed` would be measured from the slot.
  const openAtPoint = useCallback<OpenMenuAt>((x, y) => {
    const box = wrapperRef.current?.getBoundingClientRect();
    if (box === undefined) return;
    setPoint({ left: x - box.left, top: y - box.top });
    setStartAt("menu");
    setOpen(true);
  }, []);
  const rowMenu = useContext(RowMenuContext);
  useLayoutEffect(() => {
    if (rowMenu === null) return;
    rowMenu.current = openAtPoint;
    return () => {
      if (rowMenu.current === openAtPoint) rowMenu.current = null;
    };
  }, [rowMenu, openAtPoint]);

  // A menu opened at a point turns up where the list has no room below
  // it, and leftwards where it has none to the right: measured once it is
  // drawn, before it is painted.
  useLayoutEffect(() => {
    const menu = menuRef.current;
    const wrapper = wrapperRef.current;
    if (!open || point === null || menu === null || wrapper === null) return;
    const bounds = panelBounds(wrapper);
    const box = wrapper.getBoundingClientRect();
    const x = box.left + point.left;
    const y = box.top + point.top;
    let { left, top } = point;
    if (y + menu.offsetHeight > bounds.bottom && y - menu.offsetHeight >= bounds.top) top -= menu.offsetHeight;
    if (x + menu.offsetWidth > bounds.right && x - menu.offsetWidth >= bounds.left) left -= menu.offsetWidth;
    if (left !== point.left || top !== point.top) setPoint({ left, top });
    // Once each time it opens: the point it was opened at is what it is
    // measured from, not where it was moved to.
  }, [open]);

  // A row goes once its ⋯ → Skip this version or Don't remind me has done
  // its work, and this button with it: the focus goes to the page's title
  // rather than to the window's body (`focusOrFallback`). Run as the button
  // leaves, while it still has the focus.
  useLayoutEffect(() => {
    const trigger = triggerRef.current;
    return () => {
      if (trigger !== null && document.activeElement === trigger) focusOrFallback(null);
    };
  }, []);

  useLayoutEffect(() => {
    if (!open) return;
    if (startAt === "menu") {
      menuRef.current?.focus();
      return;
    }
    const all = itemsOf(menuRef.current);
    (startAt === "last" ? all[all.length - 1] : all[0])?.focus();
  }, [open, startAt]);

  function openAt(where: "first" | "last") {
    setPoint(null);
    setStartAt(where);
    setOpen(true);
  }

  function onTriggerKeyDown(event: KeyboardEvent<HTMLButtonElement>) {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      openAt("first");
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      openAt("last");
    }
  }

  function onMenuKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    const all = itemsOf(menuRef.current);
    if (all.length === 0) return;
    const at = all.indexOf(document.activeElement as HTMLElement);
    const focus = (index: number) => all[(index + all.length) % all.length]?.focus();
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        focus(at + 1);
        break;
      case "ArrowUp":
        event.preventDefault();
        focus(at < 0 ? all.length - 1 : at - 1);
        break;
      case "Home":
        event.preventDefault();
        focus(0);
        break;
      case "End":
        event.preventDefault();
        focus(all.length - 1);
        break;
      case "Tab":
        // Back on the button first, so the Tab itself moves on from there
        // to whatever comes after it (or, with Shift, before it).
        close();
        triggerRef.current?.focus();
        break;
      default:
        break;
    }
  }

  function choose(item: MenuItem) {
    if (item.disabled) return;
    close();
    triggerRef.current?.focus();
    item.onSelect();
  }

  return (
    <span
      ref={wrapperRef}
      data-popup-open={open ? "" : undefined}
      className="relative inline-flex"
    >
      <button
        ref={triggerRef}
        id={buttonId}
        type="button"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? menuId : undefined}
        aria-label={label}
        onClick={() => (open ? close() : openAt("first"))}
        onKeyDown={onTriggerKeyDown}
        className={ROW_TRIGGER}
      >
        <MoreIcon size={16} />
      </button>
      {open ? (
        <div
          ref={menuRef}
          id={menuId}
          role="menu"
          aria-labelledby={buttonId}
          // Takes the focus itself when opened at a point (`startAt`).
          tabIndex={-1}
          onKeyDown={onMenuKeyDown}
          style={point === null ? undefined : { left: point.left, top: point.top }}
          // A macOS menu (spec §3.10): at least 180 wide, the corners of a
          // group, 5 in, the menu's shadow and hairline and no other edge.
          className={`absolute z-30 flex min-w-45 flex-col rounded-group bg-surface p-[5px] shadow-menu outline-none ${
            point !== null
              ? ""
              : `${placement.align === "end" ? "right-0" : "left-0"} ${
                  placement.side === "above" ? "bottom-full mb-1" : "top-full mt-1"
                }`
          }`}
        >
          {items.map((item, index) => (
            <Fragment key={item.id}>
              {item.separatorBefore && index > 0 ? (
                <div role="separator" className="mx-2.5 my-[5px] h-px shrink-0 bg-separator" />
              ) : null}
              <button
                type="button"
                role="menuitem"
                tabIndex={-1}
                aria-disabled={item.disabled ? true : undefined}
                title={item.hint}
                onClick={() => choose(item)}
                onMouseEnter={(event) => event.currentTarget.focus()}
                // 22 high, 13, 10 in; its highlight is its focus, the
                // pointer's or the keyboard's, as a Mac menu's is: the
                // accent with white words, corners of 6, at once -- no
                // ring besides and no fade. One that is off is in the
                // tertiary grey and never lights up.
                className="flex h-5.5 w-full shrink-0 items-center whitespace-nowrap rounded-control px-2.5 text-left text-body text-foreground outline-none focus:bg-accent focus:text-accent-foreground aria-disabled:cursor-default aria-disabled:text-tertiary aria-disabled:focus:bg-transparent aria-disabled:focus:text-tertiary"
              >
                {item.label}
              </button>
            </Fragment>
          ))}
        </div>
      ) : null}
    </span>
  );
}
