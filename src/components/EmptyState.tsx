import type { ReactNode } from "react";
import { Popover } from "./ui/Popover";
import { DETAILS_TRIGGER_CLASS } from "./SourceNotice";
import { BUTTON } from "./ui/controls";
import { CheckCircleIcon, InfoIcon, WarningIcon } from "./icons";

export interface EmptyStateAction {
  label: string;
  onClick: () => void;
  /** Off, as Check again is while a check runs, whoever started it. */
  disabled?: boolean;
}

/** What the description leaves for "Details", and that button's words. */
export interface EmptyStateDetail {
  /** 「详情」/"Details". */
  label: string;
  /** The button's accessible name, which says what it is the details of. */
  ariaLabel: string;
  content: ReactNode;
}

export interface EmptyStateProps {
  title: string;
  /**
   * One sentence, ending with its full stop -- or a label, such as when
   * the last check was, with none; or nothing.
   */
  description?: string;
  /** More than the one line, behind a "Details" button after it. */
  detail?: EmptyStateDetail;
  action?: EmptyStateAction;
  /**
   * Over an empty list, the symbol: a ✓ in a circle where there is
   * nothing to do, a ⚠︎ where something went wrong (a source that did not
   * answer), an ⓘ in a circle for anything else (the default).
   */
  symbol?: "check" | "info" | "warning";
}

/**
 * What a list says when it has nothing to show, as macOS says it
 * (ContentUnavailableView, measured in native-sui-empty; spec §3.9):
 * centred in the list's area, a 36 symbol in the tertiary grey -- a ✓ or
 * an ⓘ in a circle, never green: an empty list is not a success to
 * celebrate -- then 24 below it the title, 15/20 semibold, and 8 below
 * that one line, 15/20 regular, no wider than 360; both in the
 * secondary grey, as the native view sets them. 16 under it, at most one
 * button, regular and grey.
 */
export function EmptyState({
  title,
  description,
  detail,
  action,
  symbol = "info",
}: EmptyStateProps) {
  const details = detail ? (
    <>
      {" "}
      <Popover trigger={detail.label} triggerLabel={detail.ariaLabel} triggerClassName={DETAILS_TRIGGER_CLASS}>
        {detail.content}
      </Popover>
    </>
  ) : null;

  return (
    <div data-empty-state="" className="flex h-full flex-1 flex-col items-center justify-center px-5 py-10 text-center">
      {symbol === "check" ? (
        <CheckCircleIcon size={36} className="shrink-0 text-tertiary" />
      ) : symbol === "warning" ? (
        <WarningIcon size={36} className="shrink-0 text-tertiary" />
      ) : (
        <InfoIcon size={36} className="shrink-0 text-tertiary" />
      )}
      <p className="mt-6 text-section text-muted">{title}</p>
      {/* A <div>, not a <p>: the "Details" panel is a <div>. */}
      {description || detail ? (
        <div className="mt-2 max-w-90 text-section font-normal text-muted">
          {description}
          {details}
        </div>
      ) : null}
      {action ? (
        // The one way on: Check again, or where the hidden ones are. Left
        // class-less it would render as one more line of text under the
        // sentence, and the way out would be invisible.
        <button
          type="button"
          onClick={action.onClick}
          disabled={action.disabled}
          className={`mt-4 ${BUTTON.regular.grey}`}
        >
          {action.label}
        </button>
      ) : null}
    </div>
  );
}
