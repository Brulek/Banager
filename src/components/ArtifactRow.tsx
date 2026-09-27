import type { ReactNode } from "react";

/**
 * `neutral` for a row Canager cannot act on, `info` for one it can. There
 * was a `warning` variant too, for an "N warnings" badge on the Updates
 * page; that badge's only two producers were deleted on this branch, so
 * both it and the variant went with them rather than sitting here
 * unreachable.
 */
export type BadgeVariant = "neutral" | "info";

export interface ArtifactRowSelectable {
  checked: boolean;
  onToggle: () => void;
  ariaLabel: string;
}

export interface ArtifactRowProps {
  name: string;
  /** Usually a string; a node when part of it is not prose, like the
   *  Updates page's unpin command rendered as code. */
  description: ReactNode;
  badgeText: string;
  badgeVariant: BadgeVariant;
  /** Omit both this and `onPrimaryAction` for a row with no primary action
   * at all (Task 12: a read-only source, e.g. pip, offers no uninstall). */
  primaryActionLabel?: string;
  onPrimaryAction?: () => void;
  primaryActionDisabled?: boolean;
  selectable?: ArtifactRowSelectable;
  /** Extra inline content between the description and the badge (the Updates page's Skip this version and Never remind me buttons). */
  secondaryContent?: ReactNode;
  /**
   * Let the description use as many lines as it needs instead of being cut
   * off at one.
   *
   * A description is normally a package's own one-line blurb, where a single
   * clipped line is the right trade: the name and the badge are what the row
   * is for. But some rows carry an *explanation* instead -- why this row will
   * never have an Update button, why Canager could not check it this time --
   * and an explanation that is cut off mid-sentence has not been given. The
   * reason usually sits at the end (the tool's own error text, a URL), which
   * is exactly the part one line loses.
   */
  wrapDescription?: boolean;
}

const BADGE_CLASSES: Record<BadgeVariant, string> = {
  neutral: "bg-[var(--color-hover)] text-[var(--color-muted)]",
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
  wrapDescription,
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
        {/* `break-words` and not just "no truncate": the reason text often
            ends in a URL or a long unspaced token out of a tool's stderr,
            which would otherwise run off the row instead of wrapping. */}
        <p
          className={`text-xs text-[var(--color-muted)] ${
            wrapDescription ? "break-words" : "truncate"
          }`}
        >
          {description}
        </p>
      </div>
      {secondaryContent}
      <span
        className={`shrink-0 rounded-full px-2 py-1 text-xs font-medium ${BADGE_CLASSES[badgeVariant]}`}
      >
        {badgeText}
      </span>
      {primaryActionLabel && onPrimaryAction ? (
        <button
          type="button"
          onClick={onPrimaryAction}
          disabled={primaryActionDisabled}
          className="shrink-0 rounded-md bg-[var(--color-accent)] px-3 py-1 text-sm font-medium text-[var(--color-accent-foreground)] disabled:opacity-50"
        >
          {primaryActionLabel}
        </button>
      ) : null}
    </div>
  );
}
