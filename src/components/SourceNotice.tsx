import type { ReactNode } from "react";
import { InfoIcon, WarningFilledIcon } from "./icons";
import { Popover } from "./ui/Popover";
import { BUTTON, LINK } from "./ui/controls";

/**
 * The look of a "Details" button beside a sentence: a notice's, or its
 * error's -- a link (`LINK`), in the size of the line it is in. Not of the
 * fold's 「还有 N 条」 and 「收起」, which have a look of their own, a
 * disclosure's (`SourceNotices`).
 */
export const DETAILS_TRIGGER_CLASS = `shrink-0 ${LINK}`;

export type SourceNoticeVariant = "info" | "warning";

export interface SourceNoticeAction {
  label: string;
  onClick: () => void;
  /** Off, as Check again is while a check runs, whoever started it. */
  disabled?: boolean;
}

/** The look of a notice's own button, in a line or whole: a small grey one, its words dimmed while it is off. */
const ACTION_CLASS = BUTTON.small.grey;

export interface SourceNoticeProps {
  variant: SourceNoticeVariant;
  title: string;
  description?: string;
  action?: SourceNoticeAction;
  /**
   * What went wrong the last time `action` was pressed, already in the
   * user's language -- with its own "Details", when it has one. Shown
   * under the description rather than in place of it: the notice is still
   * true, and its button can be pressed again.
   */
  error?: ReactNode;
}

/**
 * The notice's icon, 16: a warning, the orange ⚠︎ filled as macOS marks
 * one -- the colour the symbol's, never the text's -- or information, a
 * muted ⓘ.
 */
function NoticeIcon({ variant, className = "" }: { variant: SourceNoticeVariant; className?: string }) {
  return variant === "warning" ? (
    <WarningFilledIcon size={16} className={`shrink-0 text-warning ${className}`} />
  ) : (
    <InfoIcon size={16} className={`shrink-0 text-muted ${className}`} />
  );
}

/**
 * One notice, whole: its title, its description under it and its button
 * -- Ollama's "not running" with Open Ollama. Where there is room for the
 * explanation, next to what it explains: a tool's detail drawer on the
 * Installed page, and the Unknown page's "the scan stopped early". The
 * lists put their sources' notices at their top, a line each
 * (`SourceNoticeLine`), with the description behind "Details".
 * `SourceNotices` maps `sourceNoticesFor`'s specs onto either.
 *
 * Purely presentational -- callers decide when it applies and what its
 * action does; this component never calls `invoke`.
 */
export function SourceNotice({ variant, title, description, action, error }: SourceNoticeProps) {
  // No fill and no corners (spec §3.8): the icon, the title, and the
  // description on the line under it, quieter, as a Mac list's secondary
  // line is.
  return (
    <div className="flex gap-2 text-body">
      <NoticeIcon variant={variant} />
      <div className="min-w-0 flex-1">
        <p className="text-foreground">{title}</p>
        {description ? <p className="mt-0.5 text-small text-muted">{description}</p> : null}
        {action ? (
          <button
            type="button"
            onClick={action.onClick}
            disabled={action.disabled}
            className={`mt-2 ${ACTION_CLASS}`}
          >
            {action.label}
          </button>
        ) : null}
        {/* A <div>: the error's own "Details" panel is one. */}
        {error ? (
          <div role="alert" className="mt-1.5 text-small text-danger-text">
            {error}
          </div>
        ) : null}
      </div>
    </div>
  );
}

export interface SourceNoticeLineProps extends SourceNoticeProps {
  /** The words on the button that shows `description`: 「详情」/"Details". */
  detailsLabel: string;
  /** That button's accessible name, which says which notice it belongs to. */
  detailsAriaLabel: string;
  /**
   * Last in the line, after the notice's own button: 「还有 N 条」 on the
   * one line a page's notices fold into (`SourceNotices`).
   */
  trailing?: ReactNode;
}

/**
 * The same notice as one line of a list, 32 high (spec §3.8; the
 * disclosed rows in cork-outdated.png): the icon, the short title, and
 * "Details" -- a link that shows the description under it -- then the
 * notice's own button, if it has one (Open Ollama, Check again), small and
 * grey, which stays in the line rather than behind the popover. The Updates
 * page makes the line its list's first row; the Installed page puts it
 * over its list. A failed press says so under the line. The line's edges
 * are its container's: the lists put it 20 in, where their rows start.
 */
export function SourceNoticeLine({
  variant,
  title,
  description,
  action,
  error,
  detailsLabel,
  detailsAriaLabel,
  trailing,
}: SourceNoticeLineProps) {
  return (
    <div className="flex flex-col">
      <div data-notice-line="" className="flex h-8 min-w-0 items-center gap-2 text-body">
        <NoticeIcon variant={variant} />
        <span title={title} className="min-w-0 truncate text-foreground">
          {title}
        </span>
        {description ? (
          <Popover trigger={detailsLabel} triggerLabel={detailsAriaLabel} triggerClassName={DETAILS_TRIGGER_CLASS}>
            {description}
          </Popover>
        ) : null}
        {action ? (
          <button
            type="button"
            onClick={action.onClick}
            disabled={action.disabled}
            className={`shrink-0 ${ACTION_CLASS}`}
          >
            {action.label}
          </button>
        ) : null}
        {trailing}
      </div>
      {/* A <div>: the error's own "Details" panel is one. */}
      {/* Under the title, past the icon. */}
      {error ? (
        <div role="alert" className="pb-1 pl-6 text-small text-danger-text">
          {error}
        </div>
      ) : null}
    </div>
  );
}
