import * as RadixScrollArea from "@radix-ui/react-scroll-area";
import { forwardRef, type ReactNode, type UIEvent } from "react";

export interface ScrollAreaProps {
  children: ReactNode;
  className?: string;
  onViewportScroll?: (event: UIEvent<HTMLDivElement>) => void;
}

export const ScrollArea = forwardRef<HTMLDivElement, ScrollAreaProps>(
  ({ children, className, onViewportScroll }, ref) => (
    <RadixScrollArea.Root className={className} type="auto">
      <RadixScrollArea.Viewport ref={ref} className="h-full w-full" onScroll={onViewportScroll}>
        {children}
      </RadixScrollArea.Viewport>
      <RadixScrollArea.Scrollbar orientation="vertical" className="w-2 bg-transparent">
        <RadixScrollArea.Thumb className="rounded-full bg-[var(--color-border)]" />
      </RadixScrollArea.Scrollbar>
    </RadixScrollArea.Root>
  ),
);
ScrollArea.displayName = "ScrollArea";
