import type { ReactNode } from "react";

export type BadgeVariant = "neutral" | "warning" | "info";

export interface ArtifactRowSelectable {
  checked: boolean;
  onToggle: () => void;
  ariaLabel: string;
}

export interface ArtifactRowProps {
  name: string;
  description: string;
  badgeText: string;
  badgeVariant: BadgeVariant;
  primaryActionLabel: string;
  onPrimaryAction: () => void;
  primaryActionDisabled?: boolean;
  selectable?: ArtifactRowSelectable;
  /** Extra inline content between the description and the badge (Task 12 uses this for an Ignore link). */
  secondaryContent?: ReactNode;
}

const BADGE_CLASSES: Record<BadgeVariant, string> = {
  neutral: "bg-[var(--color-hover)] text-[var(--color-muted)]",
  warning: "bg-[var(--color-danger)]/10 text-[var(--color-danger)]",
  info: "bg-[var(--color-accent)]/10 text-[var(--color-accent)]",
};

export function ArtifactRow({
  name,
  description,
  badgeText,
  badgeVariant,
  primaryActionLabel,
  onPrimaryAction,
  primaryActionDisabled,
  selectable,
  secondaryContent,
}: ArtifactRowProps) {
  return (
    <div className="flex items-center gap-3 border-b border-[var(--color-border)] px-4 py-2">
      {selectable ? (
        <input
          type="checkbox"
          aria-label={selectable.ariaLabel}
          checked={selectable.checked}
          onChange={selectable.onToggle}
          className="h-4 w-4 shrink-0"
        />
      ) : null}
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm font-medium text-[var(--color-foreground)]">{name}</p>
        <p className="truncate text-xs text-[var(--color-muted)]">{description}</p>
      </div>
      {secondaryContent}
      <span
        className={`shrink-0 rounded-full px-2 py-1 text-xs font-medium ${BADGE_CLASSES[badgeVariant]}`}
      >
        {badgeText}
      </span>
      <button
        type="button"
        onClick={onPrimaryAction}
        disabled={primaryActionDisabled}
        className="shrink-0 rounded-md bg-[var(--color-accent)] px-3 py-1 text-sm font-medium text-[var(--color-accent-foreground)] disabled:opacity-50"
      >
        {primaryActionLabel}
      </button>
    </div>
  );
}
