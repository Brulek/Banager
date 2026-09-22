import type { ReactNode } from "react";

export interface EmptyStateAction {
  label: string;
  onClick: () => void;
}

export interface EmptyStateProps {
  title: string;
  description: string;
  action?: EmptyStateAction;
  variant?: "empty" | "banner";
  icon?: ReactNode;
}

export function EmptyState({
  title,
  description,
  action,
  variant = "empty",
  icon,
}: EmptyStateProps) {
  const isBanner = variant === "banner";

  return (
    <div
      role={isBanner ? "status" : undefined}
      className={
        isBanner
          ? "flex items-center gap-4 border-b border-[var(--color-border)] bg-[var(--color-sidebar-bg)] px-6 py-3"
          : "flex h-full flex-col items-center justify-center gap-3 p-12 text-center"
      }
    >
      {icon}
      <div className={isBanner ? "flex-1" : undefined}>
        <p className={isBanner ? "font-medium" : "text-lg font-semibold"}>{title}</p>
        <p className="text-sm text-[var(--color-muted-foreground)]">{description}</p>
      </div>
      {action && (
        // This is the Retry button of the refresh-failed states, the only way
        // out of them. Left class-less it rendered as one more line of text
        // under the explanation, so the recovery the screen is offering was
        // invisible.
        <button
          type="button"
          onClick={action.onClick}
          className="shrink-0 rounded-md bg-[var(--color-accent)] px-3 py-1 text-sm font-medium text-[var(--color-accent-foreground)]"
        >
          {action.label}
        </button>
      )}
    </div>
  );
}
