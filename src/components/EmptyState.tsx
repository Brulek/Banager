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
          : "flex flex-1 flex-col items-center justify-center gap-3 p-12 text-center"
      }
    >
      {icon}
      <div className={isBanner ? "flex-1" : undefined}>
        <p className={isBanner ? "font-medium" : "text-lg font-semibold"}>{title}</p>
        <p className="text-sm text-[var(--color-muted-foreground)]">{description}</p>
      </div>
      {action && (
        <button type="button" onClick={action.onClick}>
          {action.label}
        </button>
      )}
    </div>
  );
}
