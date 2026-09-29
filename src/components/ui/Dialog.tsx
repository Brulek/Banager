import * as RadixDialog from "@radix-ui/react-dialog";
import { useCallback, useId, useLayoutEffect, useRef, type ReactNode, type RefObject } from "react";
import { focusOrFallback } from "./focus";
import { watchMoreBelow } from "./moreBelow";

/**
 * How wide a dialog is (spec §3.6, §3.10): 360 for a question about one
 * tool -- near an alert's own width (native-alert-light.png), so that its
 * 48 icon, question, line and text fill it and its buttons end under
 * them, not out in an empty corner -- 480 for one about several, whose
 * list needs the room for a version beside each name, and for the icon
 * credits' list, and 560 for an operation's log, whose lines are a
 * program's own.
 */
export const DIALOG_WIDTHS = { one: 360, several: 480, log: 560 } as const;

export type DialogWidth = keyof typeof DIALOG_WIDTHS;

export interface DialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** The question, as the sheet's name: 「要卸载“jq”吗？」, "Update 3 tools?". */
  title: string;
  children: ReactNode;
  /** `one` unless said: see `DIALOG_WIDTHS`. */
  width?: DialogWidth;
  /**
   * Over the title, as NSAlert puts an app's icon over its question: the
   * tool's own, at 48, where the dialog is about one tool.
   */
  icon?: ReactNode;
  /** Under the title, quieter: the tool's source and version (11/14 muted). */
  subtitle?: ReactNode;
  /**
   * The ids of what the dialog says in its body, its main text, which
   * describes it to a screen reader after its subtitle (`aria-describedby`)
   * as it opens: the uninstall's sentence, the quit question's. Where it
   * has `description`, that describes it instead.
   */
  describedBy?: string;
  /**
   * The first thing in the body, 13 in the label colour, which also
   * describes the dialog to a screen reader: the icon credits' sentence
   * about whose the logos are.
   */
  description?: ReactNode;
  /**
   * The buttons, 16 under what it says, on the right: the grey one first,
   * then the one it asks for, the default button -- large, both
   * (`BUTTON.large` in ./controls.ts), and never red, removing something
   * included.
   */
  footer?: ReactNode;
  /** At the foot's other end, on the left: the log's Copy. */
  footerStart?: ReactNode;
  /**
   * The buttons stand one over the other, as wide as the dialog, the
   * default one on top -- as NSAlert stacks its buttons when they do not
   * fit side by side. `footer` then lists the default button first.
   */
  stackedFooter?: boolean;
  /**
   * The body does not scroll as a whole: its child fills it and scrolls
   * what it chooses -- the log, which keeps to its end as lines arrive.
   */
  fillBody?: boolean;
  /**
   * What gets the focus back when it closes: what opened it -- a row's
   * button, the ⋯ button whose menu item opened it, Update all. Without
   * it, or once it is gone from the page, whatever had the focus as the
   * sheet opened.
   */
  returnFocusTo?: RefObject<HTMLElement | null>;
  /**
   * What has the focus as it opens, in place of its first control. While
   * that cannot take it -- Update, off until its plan has arrived -- the
   * sheet itself has the focus, so that nothing is pressed by a key meant
   * for something else, and it stays inside the sheet.
   */
  initialFocus?: RefObject<HTMLElement | null>;
  /**
   * The dialog itself takes the focus as it opens, rather than any of its
   * controls, and a screen reader reads its name: the log, which opens by
   * itself as an uninstall starts, one Return away from Done otherwise.
   */
  focusSelf?: boolean;
  /** Called once it has closed and handed the focus back. */
  onClosed?: () => void;
}

/**
 * A dialog in the manner of macOS's own alerts and sheets (spec §3.6,
 * measured off NSAlert on macOS 27): no edge, the corners of a group (10),
 * the dialog's shadow, hung 52 from the top of the window -- under the
 * toolbar -- and 20 in from its edges all round. Its 48 icon where it has
 * one, 12 over its question in 13 bold; what it is about under that; its
 * buttons 16 below, on the right and 8 apart, with no line or band of
 * their own. Its body scrolls between the question and the buttons,
 * which stay put, and it grows downwards as what it has to say arrives,
 * to the window's height less 96; while more of it is below what is in
 * sight, its bottom edge fades out (`watchMoreBelow`, index.css), so it
 * does not look cut off.
 *
 * A Radix modal dialog: the page under it is out of reach but not dimmed
 * -- a Mac's alert and sheet darken nothing; the dialog's shadow and edge
 * set it apart -- Tab stays inside it, and Escape or a click on the page
 * around it closes it.
 * Escape closes an ⓘ open inside it first, and only that: the ⓘ closes
 * itself (`useDismiss` in ./floating.ts), and the sheet stays.
 *
 * Radix gives the focus back only to a Dialog.Trigger, and nothing that
 * opens these is one. So the sheet gives it back itself, to
 * `returnFocusTo`, or to what had it as the sheet opened -- noted in a
 * layout effect, which runs ahead of Radix's own -- but only when the
 * focus went with the sheet. When something else took it meanwhile, such
 * as the log an uninstall opens, it stays there. When the opener is gone
 * or off by then, it goes to the page's title (`focusOrFallback`) -- and
 * `onClosed` may put it somewhere better from there: the update
 * confirmation puts it on the row whose Update gave way to its progress,
 * or on the list after Update all (`useUpdateConfirm`'s `onStarted`).
 */
export function Dialog({
  open,
  onOpenChange,
  title,
  children,
  width = "one",
  icon,
  subtitle,
  describedBy,
  description,
  footer,
  footerStart,
  stackedFooter = false,
  fillBody = false,
  returnFocusTo,
  initialFocus,
  focusSelf = false,
  onClosed,
}: DialogProps) {
  const noted = useRef<HTMLElement | null>(null);
  const contentRef = useRef<HTMLDivElement>(null);
  const subtitleId = useId();
  const hasSubtitle = subtitle !== undefined && subtitle !== null;
  // Its subtitle, then its main text; nothing, where it has neither.
  const describedByIds = [hasSubtitle ? subtitleId : null, describedBy ?? null].filter((id) => id !== null).join(" ");
  useLayoutEffect(() => {
    if (open) noted.current = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  }, [open]);
  const hasFooter = footer !== undefined && footer !== null;
  // A body that scrolls says when more of it is below (`watchMoreBelow`);
  // one that fills itself (`fillBody`) scrolls what is inside it instead.
  const attachBody = useCallback(
    (body: HTMLDivElement | null) => (body === null || fillBody ? undefined : watchMoreBelow(body)),
    [fillBody],
  );
  return (
    <RadixDialog.Root open={open} onOpenChange={onOpenChange}>
      <RadixDialog.Portal>
        {/* Clear: it only keeps the pointer from the page under it. */}
        <RadixDialog.Overlay data-dialog-overlay="" className="fixed inset-0" />
        <RadixDialog.Content
          ref={contentRef}
          {...(description === undefined ? { "aria-describedby": describedByIds === "" ? undefined : describedByIds } : {})}
          data-dialog-width={DIALOG_WIDTHS[width]}
          onOpenAutoFocus={(event) => {
            if (focusSelf) {
              // Radix's own would pick the first control. The content is
              // focusable by script: Radix gives it `tabIndex={-1}`.
              event.preventDefault();
              contentRef.current?.focus();
              return;
            }
            const target = initialFocus?.current;
            if (target) {
              event.preventDefault();
              target.focus();
              // A disabled button takes no focus: the sheet has it instead
              // (Radix's content is focusable for this), rather than the
              // button that opened it, on the page under it.
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
            if (lost) focusOrFallback(opener);
            onClosed?.();
          }}
          // The width as a style: one of three, and never wider than the
          // window less 16 on either side.
          style={{ width: `min(${DIALOG_WIDTHS[width]}px, calc(100vw - 32px))` }}
          className="fixed left-1/2 top-[52px] flex max-h-[calc(100vh-96px)] -translate-x-1/2 flex-col rounded-group bg-surface text-foreground shadow-dialog outline-none motion-safe:animate-sheet-in"
        >
          <div className="shrink-0 px-5 pt-5">
            {icon !== undefined && icon !== null ? (
              <div data-dialog-icon="" className="mb-3 flex">
                {icon}
              </div>
            ) : null}
            <RadixDialog.Title className="break-words text-title text-foreground">{title}</RadixDialog.Title>
            {hasSubtitle ? (
              <p id={subtitleId} data-dialog-subtitle="" className="mt-0.5 break-words text-small text-muted">
                {subtitle}
              </p>
            ) : null}
          </div>
          <div
            ref={attachBody}
            data-dialog-body=""
            className={`min-h-0 flex-1 px-5 pt-2 ${fillBody ? "flex flex-col" : "overflow-y-auto"} ${
              hasFooter ? "" : "pb-5"
            }`}
          >
            {description !== undefined ? (
              <RadixDialog.Description className="break-words text-body text-foreground">
                {description}
              </RadixDialog.Description>
            ) : null}
            {children}
          </div>
          {hasFooter ? (
            <div
              data-dialog-footer=""
              className={
                stackedFooter
                  ? "flex shrink-0 flex-col items-stretch gap-2 px-5 pb-5 pt-4 [&>button]:w-full"
                  : "flex shrink-0 items-center justify-end gap-2 px-5 pb-5 pt-4"
              }
            >
              {footerStart !== undefined && footerStart !== null ? (
                <div className="mr-auto flex items-center gap-2">{footerStart}</div>
              ) : null}
              {footer}
            </div>
          ) : null}
        </RadixDialog.Content>
      </RadixDialog.Portal>
    </RadixDialog.Root>
  );
}
