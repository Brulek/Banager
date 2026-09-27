import type { ReactNode } from "react";
import { InfoIcon } from "./icons";
import { Popover } from "./ui/Popover";

export interface StatusChipProps {
  /** One or two words: 「已固定」, "Read-only". */
  label: string;
  /**
   * Why, in at most two short sentences, behind the chip's ⓘ. A chip with
   * no detail is a plain label.
   */
  detail?: ReactNode;
  /** Which edge of the chip the detail lines up with (`Popover`'s `align`). */
  align?: "start" | "end";
}

const CHIP = "inline-flex items-center gap-1 whitespace-nowrap rounded-full bg-hover px-2 py-0.5 text-small font-medium text-muted";

/**
 * A short grey label for what a row is, in place of a sentence about it:
 * the redesign's rule that a row says what it is and what it can do, and
 * keeps the why one click away. With a detail it is a button: the chip's
 * word is its name, and pressing it shows the detail under it.
 */
export function StatusChip({ label, detail, align = "end" }: StatusChipProps) {
  if (detail === undefined) {
    return <span className={CHIP}>{label}</span>;
  }
  return (
    <Popover
      trigger={
        <>
          {label}
          <InfoIcon size={13} className="shrink-0 opacity-70" />
        </>
      }
      triggerClassName={`${CHIP} outline-none transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-accent aria-expanded:text-foreground`}
      align={align}
    >
      {detail}
    </Popover>
  );
}
