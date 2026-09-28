import type { ReactNode } from "react";
import { Popover } from "./ui/Popover";
import { DETAILS_TRIGGER_CLASS } from "./SourceNotice";

export interface EmptyStateAction {
  label: string;
  onClick: () => void;
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
  description: string;
  /** More than the one line, behind a "Details" button after it. */
  detail?: EmptyStateDetail;
  action?: EmptyStateAction;
  variant?: "empty" | "banner";
  icon?: ReactNode;
}

export function EmptyState({
  title,
  description,
  detail,
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
          ? "flex items-center gap-4 border-b border-[var(--color-border)] bg-[var(--color-surface)] px-6 py-3"
          : "flex h-full flex-col items-center justify-center gap-3 p-12 text-center"
      }
    >
      {icon}
      <div className={isBanner ? "flex-1" : undefined}>
        <p className={isBanner ? "font-medium" : "text-lg font-semibold"}>{title}</p>
        {/* A <div>, not a <p>: the "Details" panel is a <div>. */}
        <div className="text-sm text-[var(--color-muted-foreground)]">
          {description}
          {detail ? (
            <>
              {" "}
              <Popover trigger={detail.label} triggerLabel={detail.ariaLabel} triggerClassName={DETAILS_TRIGGER_CLASS}>
                {detail.content}
              </Popover>
            </>
          ) : null}
        </div>
      </div>
      {action && (
        // This is the Check again button of the load-failed states, the
        // only way out of them. Left class-less it rendered as one more line
        // of text under the explanation, so the recovery the screen is
        // offering was invisible.
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
