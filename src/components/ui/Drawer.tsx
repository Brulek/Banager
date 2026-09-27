import * as RadixDialog from "@radix-ui/react-dialog";
import { useLayoutEffect, useRef, type ReactNode } from "react";
import { CloseIcon } from "../icons";

export interface DrawerProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Its name, which a screen reader announces when it opens. */
  title: string;
  /** Quieter, under the title. */
  subtitle?: ReactNode;
  /** Before the title, such as the source's avatar. */
  leading?: ReactNode;
  /** The first thing under the header, which also describes the drawer to a screen reader. */
  description?: ReactNode;
  /** The close button's accessible name. */
  closeLabel: string;
  /** Kept at the bottom while the rest scrolls: the actions. */
  footer?: ReactNode;
  /**
   * Called as it closes, before the focus goes back to what had it when
   * it opened; `event.preventDefault()` leaves the focus where it is --
   * for whatever opens next, such as the log drawer.
   */
  onCloseAutoFocus?: (event: Event) => void;
  /**
   * Give the focus to the panel itself as it opens, rather than to its
   * first control -- which is the close button, one Enter from shutting
   * what was just opened. A screen reader then reads the panel's name. The
   * log drawer's: it opens by itself when an uninstall starts.
   */
  focusPanelOnOpen?: boolean;
  /**
   * The body does not scroll as a whole: its child fills it and scrolls
   * what it chooses -- the log drawer's log, which keeps to its end.
   */
  fillBody?: boolean;
  children?: ReactNode;
}

/**
 * A panel that slides in from the window's right edge over the page, for
 * the details of one thing on a list (docs/superpowers/2026-09-27-ui-
 * redesign.md, 原则 2: details on demand). A modal dialog, as Radix builds
 * one: the page under it is dimmed and out of reach, Tab stays inside it,
 * Escape, the close button or a click on the dimmed page closes it, and
 * the focus goes back to what opened it.
 *
 * That last part is the drawer's own: Radix gives the focus back to a
 * `Dialog.Trigger`, and a row that opens its details is not one -- the
 * focus went to the page's body. So the drawer notes what has the focus
 * as it opens, before Radix moves it inside (a layout effect runs ahead
 * of Radix's own effect), and gives it back on closing if it is still on
 * the page.
 */
export function Drawer({
  open,
  onOpenChange,
  title,
  subtitle,
  leading,
  description,
  closeLabel,
  footer,
  onCloseAutoFocus,
  focusPanelOnOpen = false,
  fillBody = false,
  children,
}: DrawerProps) {
  const openerRef = useRef<HTMLElement | null>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    if (open) openerRef.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  }, [open]);
  return (
    <RadixDialog.Root open={open} onOpenChange={onOpenChange}>
      <RadixDialog.Portal>
        <RadixDialog.Overlay className="fixed inset-0 bg-[var(--color-overlay)]" />
        <RadixDialog.Content
          ref={panelRef}
          onOpenAutoFocus={(event) => {
            if (!focusPanelOnOpen) return;
            // Radix's own would pick the first control. The panel is
            // focusable by script: Radix gives it `tabIndex={-1}`.
            event.preventDefault();
            panelRef.current?.focus();
          }}
          onCloseAutoFocus={(event) => {
            onCloseAutoFocus?.(event);
            const leaveFocus = event.defaultPrevented;
            // Radix's own would look for a Dialog.Trigger.
            event.preventDefault();
            const opener = openerRef.current;
            if (!leaveFocus && opener !== null && opener.isConnected) opener.focus();
          }}
          {...(description === undefined ? { "aria-describedby": undefined } : {})}
          className="fixed inset-y-0 right-0 flex w-full max-w-[400px] flex-col border-l border-border bg-surface shadow-2xl shadow-black/20 outline-none motion-safe:animate-drawer-in"
        >
          <div className="flex shrink-0 items-start gap-3 px-5 pb-4 pt-5">
            {leading}
            <div className="min-w-0 flex-1">
              <RadixDialog.Title className="break-words text-section text-foreground">{title}</RadixDialog.Title>
              {subtitle ? <p className="mt-0.5 text-small text-muted">{subtitle}</p> : null}
            </div>
            <RadixDialog.Close
              aria-label={closeLabel}
              className="-mr-1.5 -mt-0.5 flex h-7 w-7 shrink-0 items-center justify-center rounded-button text-muted outline-none transition-colors hover:bg-hover hover:text-foreground focus-visible:ring-2 focus-visible:ring-accent"
            >
              <CloseIcon size={16} />
            </RadixDialog.Close>
          </div>
          <div className={`min-h-0 flex-1 px-5 pb-5 ${fillBody ? "flex flex-col" : "overflow-y-auto"}`}>
            {description !== undefined ? (
              <RadixDialog.Description className="break-words text-body text-foreground">
                {description}
              </RadixDialog.Description>
            ) : null}
            {children}
          </div>
          {footer ? (
            <div className="flex shrink-0 items-center justify-end gap-2 border-t border-border px-5 py-3">
              {footer}
            </div>
          ) : null}
        </RadixDialog.Content>
      </RadixDialog.Portal>
    </RadixDialog.Root>
  );
}
