import type { ReactNode } from "react";
import { InfoIcon, WarningIcon } from "./icons";
import { Popover } from "./ui/Popover";

/**
 * The look of a "Details" button beside a sentence: a notice's, or its
 * error's. Not of the fold's 「还有 N 条」 and 「收起」, which have a look
 * of their own, a disclosure's (`SourceNotices`).
 */
export const DETAILS_TRIGGER_CLASS =
  "shrink-0 rounded-sm text-small font-medium text-accent-text outline-none hover:underline focus-visible:ring-2 focus-visible:ring-accent";

export type SourceNoticeVariant = "info" | "warning";

export interface SourceNoticeAction {
  label: string;
  onClick: () => void;
}

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

/** The notice's icon: a warning, or information. */
function NoticeIcon({ variant }: { variant: SourceNoticeVariant }) {
  return variant === "warning" ? (
    <WarningIcon size={16} className="mt-px shrink-0 text-warning" />
  ) : (
    <InfoIcon size={16} className="mt-px shrink-0 text-muted" />
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
  return (
    <div className="flex gap-2.5 rounded-row bg-hover/60 px-3 py-2.5 text-body">
      <NoticeIcon variant={variant} />
      <div className="min-w-0 flex-1">
        <p className="font-medium text-foreground">{title}</p>
        {description ? <p className="mt-0.5 text-muted">{description}</p> : null}
        {action ? (
          <button
            type="button"
            onClick={action.onClick}
            className="mt-2 rounded-button border border-border bg-surface px-2.5 py-0.5 text-small font-medium text-foreground outline-none transition-colors hover:bg-hover focus-visible:ring-2 focus-visible:ring-accent"
          >
            {action.label}
          </button>
        ) : null}
        {/* A <div>: the error's own "Details" panel is one. */}
        {error ? (
          <div role="alert" className="mt-1.5 text-small font-medium text-danger">
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
 * The same notice as one compact line, for the top of a list -- the
 * Updates page's and the Installed page's: an icon, the short title, and
 * "Details" -- a popover with the description -- then the notice's own
 * button, if it has one (Open Ollama, Check again), which stays in the line
 * rather than behind the popover. A failed press says so under the line.
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
    <div className="flex flex-col gap-0.5">
      <div className="flex min-w-0 items-center gap-2 text-body">
        {variant === "warning" ? (
          <WarningIcon size={16} className="shrink-0 text-warning" />
        ) : (
          <InfoIcon size={16} className="shrink-0 text-muted" />
        )}
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
            className="shrink-0 rounded-button border border-border bg-surface px-2.5 py-0.5 text-small font-medium text-foreground outline-none transition-colors hover:bg-hover focus-visible:ring-2 focus-visible:ring-accent"
          >
            {action.label}
          </button>
        ) : null}
        {trailing}
      </div>
      {/* A <div>: the error's own "Details" panel is one. */}
      {error ? (
        <div role="alert" className="pl-6 text-small font-medium text-danger">
          {error}
        </div>
      ) : null}
    </div>
  );
}
