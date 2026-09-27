import * as RadixDialog from "@radix-ui/react-dialog";
import { useLayoutEffect, useRef, type ReactNode, type RefObject } from "react";

export interface DialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** The question, as the sheet's name: 「卸载 Claude Code？」, "Update 3 tools?". */
  title: string;
  children: ReactNode;
  /** The buttons, at the foot: the quiet one first, then the one it asks for (`SHEET_BUTTON`). */
  footer?: ReactNode;
  /**
   * What gets the focus back when it closes: what opened it -- a row's
   * button, the ⋯ button whose menu item opened it, a drawer's action,
   * Update all. Without it, or once it is gone from the page, whatever had
   * the focus as the sheet opened.
   */
  returnFocusTo?: RefObject<HTMLElement | null>;
  /**
   * What has the focus as it opens, in place of its first control. While
   * that cannot take it -- Update, off until its plan has arrived -- the
   * sheet itself has the focus, so that nothing is pressed by a key meant
   * for something else, and it stays inside the sheet.
   */
  initialFocus?: RefObject<HTMLElement | null>;
  /** Called once it has closed and handed the focus back. */
  onClosed?: () => void;
}

/**
 * The buttons at a sheet's foot: the quiet one (Cancel, Close), the one it
 * asks for (Update), and the one it asks for when that removes something
 * (Uninstall), which is the only red button in the app.
 */
export const SHEET_BUTTON = {
  secondary:
    "h-8 rounded-button border border-border bg-surface px-4 text-body font-medium text-foreground outline-none transition-colors hover:bg-hover focus-visible:ring-2 focus-visible:ring-accent disabled:opacity-50 disabled:hover:bg-surface",
  primary:
    "h-8 min-w-20 rounded-button bg-accent px-4 text-body font-semibold text-accent-foreground outline-none transition-colors hover:bg-accent-hover focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2 focus-visible:ring-offset-surface disabled:opacity-50 disabled:hover:bg-accent",
  danger:
    "h-8 min-w-20 rounded-button bg-danger px-4 text-body font-semibold text-white outline-none transition-colors hover:bg-danger/90 focus-visible:ring-2 focus-visible:ring-danger focus-visible:ring-offset-2 focus-visible:ring-offset-surface disabled:opacity-50 disabled:hover:bg-danger",
} as const;

/**
 * A calm sheet for a question that needs an answer before anything runs:
 * the update and uninstall confirmations. The question as its title, what
 * it is about under it, then its buttons at the foot. Its body scrolls
 * between the two, which stay put; it hangs from near the top of the
 * window, so it grows downwards as what it has to say arrives.
 *
 * A Radix modal dialog: the page under it is dimmed and out of reach, Tab
 * stays inside it, and Escape or a click on the dimmed page closes it.
 * Escape closes an ⓘ open inside it first, and only that: the ⓘ closes
 * itself (`useDismiss` in ./floating.ts), and the sheet stays.
 *
 * Radix gives the focus back only to a Dialog.Trigger, and nothing that
 * opens these is one. So the sheet gives it back itself, to
 * `returnFocusTo`, or to what had it as the sheet opened -- noted in a
 * layout effect, which runs ahead of Radix's own, as the Drawer does --
 * but only when the focus went with the sheet. When something else took
 * it meanwhile, such as the log drawer an uninstall opens, it stays there.
 */
export function Dialog({
  open,
  onOpenChange,
  title,
  children,
  footer,
  returnFocusTo,
  initialFocus,
  onClosed,
}: DialogProps) {
  const noted = useRef<HTMLElement | null>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    if (open) noted.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  }, [open]);
  return (
    <RadixDialog.Root open={open} onOpenChange={onOpenChange}>
      <RadixDialog.Portal>
        <RadixDialog.Overlay className="fixed inset-0 bg-[var(--color-overlay)] motion-safe:animate-fade-in" />
        <RadixDialog.Content
          ref={contentRef}
          aria-describedby={undefined}
          onOpenAutoFocus={(event) => {
            const target = initialFocus?.current;
            if (target) {
              event.preventDefault();
              target.focus();
              // A disabled button takes no focus: the sheet has it instead
              // (Radix's content is focusable for this), rather than the
              // button that opened it, under the dimmed page.
              if (document.activeElement !== target) contentRef.current?.focus();
            }
          }}
          onEscapeKeyDown={(event) => {
            if (contentRef.current?.querySelector("[data-popup-open]")) event.preventDefault();
          }}
          onCloseAutoFocus={(event) => {
            // Radix's own would look for a Dialog.Trigger.
            event.preventDefault();
            const focus = document.activeElement;
            const lost = focus === null || focus === document.body || !focus.isConnected;
            const opener = [returnFocusTo?.current, noted.current].find(
              (element): element is HTMLElement => element instanceof HTMLElement && element.isConnected,
            );
            if (lost && opener !== undefined) opener.focus();
            onClosed?.();
          }}
          className="fixed left-1/2 top-14 flex max-h-[calc(100vh-7rem)] w-[calc(100vw-2rem)] max-w-[460px] -translate-x-1/2 flex-col rounded-panel border border-border bg-surface text-foreground shadow-2xl shadow-black/25 outline-none motion-safe:animate-sheet-in"
        >
          <RadixDialog.Title className="shrink-0 break-words px-6 pb-4 pt-6 text-title text-foreground">
            {title}
          </RadixDialog.Title>
          <div className="min-h-0 flex-1 overflow-y-auto px-6 pb-6">{children}</div>
          {footer ? (
            <div className="flex shrink-0 items-center justify-end gap-2 border-t border-border px-6 py-3.5">
              {footer}
            </div>
          ) : null}
        </RadixDialog.Content>
      </RadixDialog.Portal>
    </RadixDialog.Root>
  );
}
