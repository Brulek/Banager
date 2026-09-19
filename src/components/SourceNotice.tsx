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
}

const VARIANT_CLASSES: Record<SourceNoticeVariant, string> = {
  info: "bg-[var(--color-hover)] text-[var(--color-foreground)]",
  warning: "bg-[var(--color-danger)]/10 text-[var(--color-danger)]",
};

/**
 * A per-source banner rendered under an Installed-page group header: pip's
 * read-only note, or Ollama's "daemon not running" notice with a button to
 * start it (Task 12). Purely presentational -- callers decide when it
 * applies and what its action does; this component never calls `invoke`.
 */
export function SourceNotice({ variant, title, description, action }: SourceNoticeProps) {
  return (
    <div
      className={`mb-2 mt-1 flex items-center justify-between gap-3 rounded-md px-3 py-2 text-sm ${VARIANT_CLASSES[variant]}`}
    >
      <div className="min-w-0">
        <p className="font-medium">{title}</p>
        {description ? <p className="mt-0.5 text-xs opacity-80">{description}</p> : null}
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
