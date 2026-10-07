import { useCallback, useId, useRef, useState, type ReactNode } from "react";
import { useDismiss, usePlacement } from "./floating";

/** How far the panel's side stands from the button's middle, where its arrow points: 24. */
const ARROW_IN = 24;

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
 * Drawn as a macOS popover (spec §3.10): 260 wide, the corners of a group,
 * no edge but the menu's shadow and its hairline, 12 in, its text 13/18,
 * and a 14 by 7 arrow on the side facing the button, pointing at the
 * button's middle. The panel stands 24 to the side of that middle, so the
 * arrow clears its rounded corner however small the button -- an ⓘ is 20.
 *
 * Phrasing content throughout -- a `span` panel, laid out as a block --
 * because an ⓘ sits inside a line of text, often a `<p>`, where a `<div>`
 * is not allowed (`detailLines` writes its sentences as block `span`s for
 * the same reason). Children given to it should be phrasing content too.
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
  const panelRef = useRef<HTMLSpanElement>(null);
  const close = useCallback(() => setOpen(false), []);
  useDismiss(open, close, wrapperRef, triggerRef);
  const placement = usePlacement(open, triggerRef, panelRef, align, ARROW_IN);
  const above = placement.side === "above";
  const fromEnd = placement.align === "end";
  const { shift } = placement;

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
        <span
          ref={panelRef}
          id={panelId}
          data-side={placement.side}
          data-align={placement.align}
          style={shift === 0 ? undefined : { transform: `translateX(${shift}px)` }}
          className={`absolute z-30 block w-65 whitespace-normal rounded-group bg-popover p-3 text-left text-body-long font-normal text-foreground shadow-menu ${
            fromEnd ? "right-[calc(50%-24px)]" : "left-[calc(50%-24px)]"
          } ${above ? "bottom-full mb-[9px]" : "top-full mt-[9px]"}`}
        >
          <PopoverArrow above={above} fromEnd={fromEnd} shift={shift} />
          {children}
        </span>
      ) : null}
    </span>
  );
}

/**
 * The popover's arrow, 14 by 7, its tip rounded as AppKit's is: the
 * panel's fill, and its hairline along the two slanting sides, so that it
 * reads as part of the panel. Its middle is 24 in from the panel's side,
 * where the button's middle is -- and further in by as much as the panel
 * was moved (`shift`) to stay inside its list, so it still points there.
 */
function PopoverArrow({ above, fromEnd, shift }: { above: boolean; fromEnd: boolean; shift: number }) {
  return (
    <svg
      data-popover-arrow=""
      aria-hidden="true"
      width={14}
      height={7}
      viewBox="0 0 14 7"
      style={
        shift === 0
          ? undefined
          : fromEnd
            ? { right: ARROW_IN - 7 + shift }
            : { left: ARROW_IN - 7 - shift }
      }
      className={`absolute ${fromEnd ? "right-[17px]" : "left-[17px]"} ${
        above ? "top-full rotate-180" : "bottom-full"
      } overflow-visible text-black/15 dark:text-white/15`}
    >
      <path d="M0 7.5 L5.6 1.3 Q7 -0.1 8.4 1.3 L14 7.5 Z" className="fill-popover" />
      <path d="M0 7 L5.6 1.3 Q7 -0.1 8.4 1.3 L14 7" fill="none" stroke="currentColor" strokeWidth={0.5} />
    </svg>
  );
}
