import { InfoIcon, WarningIcon } from "./icons";
import { Popover } from "./ui/Popover";

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
   * user's language. Shown under the description rather than in place of
   * it: the notice is still true, and its button can be pressed again.
   */
  error?: string;
}

const VARIANT_CLASSES: Record<SourceNoticeVariant, string> = {
  info: "bg-[var(--color-hover)] text-[var(--color-foreground)]",
  warning: "bg-[var(--color-danger)]/10 text-[var(--color-danger)]",
};

/**
 * One banner about one source: pip's read-only note, or Ollama's "not
 * running" notice with a button to start it. The Installed page renders
 * these under the source's own heading, directly above that source's rows
 * and no one else's; the Updates page puts the same notices at its top, a
 * line each (`SourceNoticeLine`). `SourceNotices` maps `sourceNoticesFor`'s
 * specs onto either.
 *
 * Purely presentational -- callers decide when it applies and what its
 * action does; this component never calls `invoke`.
 */
export function SourceNotice({ variant, title, description, action, error }: SourceNoticeProps) {
  return (
    <div
      className={`mb-2 mt-1 flex items-center justify-between gap-3 rounded-md px-3 py-2 text-sm ${VARIANT_CLASSES[variant]}`}
    >
      <div className="min-w-0">
        <p className="font-medium">{title}</p>
        {description ? <p className="mt-0.5 text-xs opacity-80">{description}</p> : null}
        {error ? (
          <p role="alert" className="mt-1 text-xs font-medium">
            {error}
          </p>
        ) : null}
      </div>
      {action ? (
        <button
          type="button"
          onClick={action.onClick}
          className="shrink-0 rounded-md bg-[var(--color-accent)] px-3 py-1 text-xs font-medium text-[var(--color-accent-foreground)]"
        >
          {action.label}
        </button>
      ) : null}
    </div>
  );
}

export interface SourceNoticeLineProps extends SourceNoticeProps {
  /** The words on the button that shows `description`: 「详情」/"Details". */
  detailsLabel: string;
  /** That button's accessible name, which says which notice it belongs to. */
  detailsAriaLabel: string;
}

/**
 * The same notice as one compact line, for the top of the Updates page:
 * an icon, the short title, and "Details" -- a popover with the
 * description -- then the notice's own button, if it has one (Open
 * Ollama, Try again), which stays in the line rather than behind the
 * popover. A failed press says so under the line.
 */
export function SourceNoticeLine({
  variant,
  title,
  description,
  action,
  error,
  detailsLabel,
  detailsAriaLabel,
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
          <Popover
            trigger={detailsLabel}
            triggerLabel={detailsAriaLabel}
            triggerClassName="shrink-0 rounded-sm text-small font-medium text-accent-text outline-none hover:underline focus-visible:ring-2 focus-visible:ring-accent"
          >
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
      </div>
      {error ? (
        <p role="alert" className="pl-6 text-small font-medium text-danger">
          {error}
        </p>
      ) : null}
    </div>
  );
}
