import { type ReactNode, useId } from "react";
import { InfoIcon, WarningFilledIcon } from "./icons";
import { InfoDetail } from "./InfoDetail";
import { BUTTON, LINK } from "./ui/controls";

/**
 * The look of a "Details" button beside a sentence -- the Overview's
 * failed check, an empty list's reason: a link (`LINK`), in the size of
 * the line it is in. Not a notice line's, which has a muted ⓘ in its
 * place (`SourceNoticeLine`), so a stack of notices is not a stack of
 * blue links.
 */
export const DETAILS_TRIGGER_CLASS = `shrink-0 ${LINK}`;

/**
 * Which list's columns a notice line lines up with (`SourceNoticeLine`):
 * `checkbox`, a list whose rows start with a checkbox -- the Updates
 * page's -- its symbol centred on the avatars' column, past the
 * checkboxes', and its words where the rows' names start; `avatar`, a list
 * whose rows start with the avatar -- the Installed and Unknown pages' --
 * its symbol centred on the avatars' column and its words where the
 * names start.
 */
export type NoticeGrid = "checkbox" | "avatar";

/**
 * A grid's classes, from the line's left edge -- 20 in, where the rows'
 * content starts: the symbol in the avatars' 32 column, past a checkbox's
 * 16 and 12 where the rows have checkboxes (a ⚠︎ out in the checkboxes'
 * column read as cut off from its words); the 12 from it to the words, and
 * the same room as a padding for a line that has no symbol of its own; and
 * a hairline starting where the words do (28 + 32 + 12 = 72, or
 * 32 + 12 = 44), as a row's does.
 */
export const NOTICE_GRID: Record<NoticeGrid, { symbol: string; gap: string; inset: string; hairline: string }> = {
  checkbox: { symbol: "ml-7 w-8", gap: "ml-3", inset: "pl-18", hairline: "left-18" },
  avatar: { symbol: "w-8", gap: "ml-3", inset: "pl-11", hairline: "left-11" },
};

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
  details?: ReactNode;
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
export function SourceNotice({ variant, title, description, details, action, error }: SourceNoticeProps) {
  // No fill and no corners (spec §3.8): the icon, the title, and the
  // description on the line under it, quieter, as a Mac list's secondary
  // line is.
  return (
    <div className="flex gap-2 text-body">
      <NoticeIcon variant={variant} />
      <div className="min-w-0 flex-1">
        <p className="text-foreground">{title}</p>
        {description ? <p className="mt-0.5 text-small text-muted">{description}</p> : null}
        {details}
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
  /**
   * Not shown: the line's ⓘ has no words. Kept for the callers that still
   * pass the words "Details" once showed.
   */
  detailsLabel?: string;
  /** The ⓘ's accessible name, which says which notice it explains: 「详情：…」/"Details: …". */
  detailsAriaLabel: string;
  /**
   * Last in the line, after the notice's own button: 「还有N条提示」 on the
   * one line a page's notices fold into (`SourceNotices`).
   */
  trailing?: ReactNode;
  /** The list's columns it lines up with (`NoticeGrid`); the avatar's by default. */
  grid?: NoticeGrid;
}

/**
 * The same notice as one line of a list, at least 32 high (spec §3.8; the
 * disclosed rows in cork-outdated.png), in one kind of control: the icon
 * in the column the rows start with, the short title where their names
 * start, a muted ⓘ that shows the description in a popover -- no accent
 * link -- then the notice's own button, if it has one (Open Ollama, Check
 * again), small and grey, which stays in the line rather than behind the
 * popover. The pages make the line their list's first row. A failed press
 * says so under the title. The line's edges are its container's: the lists
 * put it 20 in, where their rows start.
 */
export function SourceNoticeLine({
  variant,
  title,
  description,
  details,
  action,
  error,
  detailsAriaLabel,
  trailing,
  grid = "avatar",
}: SourceNoticeLineProps) {
  const columns = NOTICE_GRID[grid];
  // The button's own word (Show, Check Again) says what, not which: with
  // the notices unfolded a screen reader would hear two alike. The line's
  // title tells them apart, as a description, so the name stays the word
  // the button shows.
  const titleId = useId();
  return (
    <div className="flex flex-col">
      <div data-notice-line="" className="flex min-h-8 min-w-0 items-center py-1 text-body">
        <span data-notice-symbol="" className={`flex shrink-0 justify-center ${columns.symbol}`}>
          <NoticeIcon variant={variant} />
        </span>
        <span id={titleId} title={title} className={`min-w-0 break-words text-foreground ${columns.gap}`}>
          {title}
        </span>
        {description ? (
          // Its 20 box holds the 12 ⓘ 4 from the title, as a status word's.
          <span className="flex shrink-0">
            <InfoDetail label={detailsAriaLabel}>{description}{details}</InfoDetail>
          </span>
        ) : null}
        {action ? (
          <button
            type="button"
            onClick={action.onClick}
            disabled={action.disabled}
            aria-describedby={titleId}
            className={`ml-2 shrink-0 ${ACTION_CLASS}`}
          >
            {action.label}
          </button>
        ) : null}
        {trailing}
      </div>
      {/* A <div>: the error's own ⓘ panel is one. Under the title. */}
      {error ? (
        <div role="alert" className={`pb-1 text-small text-danger-text ${columns.inset}`}>
          {error}
        </div>
      ) : null}
    </div>
  );
}
