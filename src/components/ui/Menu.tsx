import {
  useCallback,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import { MoreIcon } from "../icons";
import { useDismiss, usePlacement } from "./floating";
import { focusOrFallback } from "./focus";

export interface MenuItem {
  /** Stable React key. */
  id: string;
  label: string;
  /** What choosing it does: its tooltip, and its accessible description. */
  hint?: string;
  /** Shown, and reachable with the arrow keys, but choosing it does nothing. */
  disabled?: boolean;
  onSelect: () => void;
}

export interface MenuProps {
  /** The ⋯ button's accessible name, e.g. "More actions for glib". */
  label: string;
  items: MenuItem[];
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
 */
export function Menu({ label, items }: MenuProps) {
  const [open, setOpen] = useState(false);
  const [startAt, setStartAt] = useState<"first" | "last">("first");
  const buttonId = useId();
  const menuId = useId();
  const wrapperRef = useRef<HTMLSpanElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const close = useCallback(() => setOpen(false), []);
  useDismiss(open, close, wrapperRef, triggerRef);
  const placement = usePlacement(open, triggerRef, menuRef, "end");

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
    const all = itemsOf(menuRef.current);
    (startAt === "last" ? all[all.length - 1] : all[0])?.focus();
  }, [open, startAt]);

  function openAt(where: "first" | "last") {
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
        className="flex h-7 w-7 items-center justify-center rounded-button text-muted outline-none transition-colors hover:bg-hover hover:text-foreground focus-visible:ring-2 focus-visible:ring-accent aria-expanded:bg-hover aria-expanded:text-foreground"
      >
        <MoreIcon size={18} />
      </button>
      {open ? (
        <div
          ref={menuRef}
          id={menuId}
          role="menu"
          aria-labelledby={buttonId}
          onKeyDown={onMenuKeyDown}
          className={`absolute z-30 flex min-w-52 flex-col rounded-button border border-border bg-surface p-1 shadow-lg shadow-black/10 ${
            placement.align === "end" ? "right-0" : "left-0"
          } ${placement.side === "above" ? "bottom-full mb-1" : "top-full mt-1"}`}
        >
          {items.map((item) => (
            <button
              key={item.id}
              type="button"
              role="menuitem"
              tabIndex={-1}
              aria-disabled={item.disabled ? true : undefined}
              title={item.hint}
              onClick={() => choose(item)}
              onMouseEnter={(event) => event.currentTarget.focus()}
              className="w-full whitespace-nowrap rounded-[6px] px-2.5 py-1.5 text-left text-body text-foreground outline-none focus:bg-accent focus:text-accent-foreground aria-disabled:cursor-default aria-disabled:opacity-50 aria-disabled:focus:bg-hover aria-disabled:focus:text-foreground"
            >
              {item.label}
            </button>
          ))}
        </div>
      ) : null}
    </span>
  );
}
