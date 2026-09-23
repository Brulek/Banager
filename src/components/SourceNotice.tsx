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
 * running" notice with a button to start it. Both pages render these
 * under the source's own heading, directly above that source's rows and
 * no one else's; the Updates page also renders them above its "nothing to
 * update" sentence, where there are no rows at all and the page would
 * otherwise announce that everything is up to date. `SourceNotices` maps
 * `sourceNoticesFor`'s specs onto this.
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
