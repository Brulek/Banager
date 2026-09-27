import { useCallback, useId, useRef, useState, type ReactNode } from "react";
import { useDismiss, usePlacement } from "./floating";

export interface PopoverProps {
  /** What the button shows: a chip's word and ⓘ, or "Details". */
  trigger: ReactNode;
  /** The button's accessible name, when its content alone does not say it. */
  triggerLabel?: string;
  triggerClassName: string;
  /** Which edge of the button the panel lines up with by preference (`usePlacement` swaps it where it would not fit): `end` near the right of the window. */
  align?: "start" | "end";
  /** The detail: one or two short sentences. */
  children: ReactNode;
}

/**
 * A button that shows a small panel of detail under it and hides it again
 * -- a status chip's "why", a notice's description. A disclosure, not a
 * dialog: the button says whether it is open (`aria-expanded`) and which
 * panel it opens (`aria-controls`), and the panel follows it in the page,
 * so a screen reader reads it next and nothing takes focus away. Escape, a
 * click outside or focus moving elsewhere closes it (`useDismiss`).
 *
 * `data-popup-open` marks it while open: a row of a virtualized list sits
 * in a slot of its own, and index.css lifts the slot that holds an open
 * panel above the slots after it, which would otherwise be painted over it.
 */
export function Popover({ trigger, triggerLabel, triggerClassName, align = "start", children }: PopoverProps) {
  const [open, setOpen] = useState(false);
  const panelId = useId();
  const wrapperRef = useRef<HTMLSpanElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const close = useCallback(() => setOpen(false), []);
  useDismiss(open, close, wrapperRef, triggerRef);
  const placement = usePlacement(open, triggerRef, panelRef, align);

  return (
    <span
      ref={wrapperRef}
      data-popup-open={open ? "" : undefined}
      className="relative inline-flex"
    >
      <button
        ref={triggerRef}
        type="button"
        aria-expanded={open}
        aria-controls={open ? panelId : undefined}
        aria-label={triggerLabel}
        onClick={() => setOpen((was) => !was)}
        className={triggerClassName}
      >
        {trigger}
      </button>
      {open ? (
        <div
          ref={panelRef}
          id={panelId}
          className={`absolute z-30 w-64 rounded-button border border-border bg-surface px-3 py-2.5 text-left text-small font-normal text-foreground shadow-lg shadow-black/10 ${
            placement.align === "end" ? "right-0" : "left-0"
          } ${placement.side === "above" ? "bottom-full mb-1.5" : "top-full mt-1.5"}`}
        >
          {children}
        </div>
      ) : null}
    </span>
  );
}
