import * as RadixScrollArea from "@radix-ui/react-scroll-area";
import { forwardRef, type ReactNode, type UIEvent } from "react";

export interface ScrollAreaProps {
  children: ReactNode;
  className?: string;
  onViewportScroll?: (event: UIEvent<HTMLDivElement>) => void;
  /**
   * The viewport's name as its own Tab stop, where nothing inside it takes
   * the focus: the log's transcript. The keyboard scrolls only what has
   * the focus; without it, ↑ ↓ and Page Up / Page Down could not reach the
   * earlier lines. The viewport itself, what scrolls, takes it.
   */
  viewportLabel?: string;
}

export const ScrollArea = forwardRef<HTMLDivElement, ScrollAreaProps>(
  ({ children, className, onViewportScroll, viewportLabel }, ref) => (
    <RadixScrollArea.Root className={className} type="auto">
      <RadixScrollArea.Viewport
        ref={ref}
        className={`h-full w-full ${viewportLabel ? "focus-visible:outline-offset-[-3px]" : ""}`}
        onScroll={onViewportScroll}
        {...(viewportLabel ? { role: "region", "aria-label": viewportLabel, tabIndex: 0 } : {})}
      >
        {children}
      </RadixScrollArea.Viewport>
      <RadixScrollArea.Scrollbar orientation="vertical" className="w-2 bg-transparent">
        <RadixScrollArea.Thumb className="rounded-full bg-[var(--color-border)]" />
      </RadixScrollArea.Scrollbar>
    </RadixScrollArea.Root>
  ),
);
ScrollArea.displayName = "ScrollArea";
