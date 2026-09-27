import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { SourceAvatar } from "./SourceAvatar";

export interface ToolRowSelectable {
  checked: boolean;
  onToggle: () => void;
  /** The checkbox's accessible name: "Select glib for update". */
  ariaLabel: string;
}

export interface ToolRowProps {
  /** The source's adapter id and name, for the avatar's colour and letter. */
  adapterId: string;
  sourceLabel: string;
  /** The tool's name as the user knows it. */
  name: string;
  /**
   * A quiet chip after the name, such as the source's name on a list that
   * mixes sources. Leave it out where it would only repeat the name (a
   * tool with its own installer is its own source).
   */
  nameChip?: string;
  /**
   * One line about what it is. `null` (or empty) only when there is truly
   * none: the row then says 「暂无简介」/"No description" rather than
   * leaving a gap.
   */
  description: string | null;
  /** A checkbox before the avatar, for a list that acts on several rows. */
  selectable?: ToolRowSelectable;
  /** Status chips (`StatusChip`), just before the version. */
  status?: ReactNode;
  /** The version column: "7.1 → 7.2", or a word where a version would mean nothing. */
  version?: ReactNode;
  /** The primary action -- a button -- or what stands in for it, such as an update's progress. */
  action?: ReactNode;
  /** The ⋯ menu (`Menu`). */
  menu?: ReactNode;
}

/**
 * One tool on a list: the Updates page's rows now, the Installed page's
 * next (docs/superpowers/2026-09-27-ui-redesign.md, 更新页 and 已安装页).
 *
 * The source's avatar, the name with its one line of description under it,
 * then the columns on the right: status chips, the version, the primary
 * action and the ⋯ menu. A column is drawn whenever its prop is given, even
 * as `null`, so rows that leave one empty still line up with rows that
 * fill it; leave the prop out to drop the column altogether.
 *
 * No borders between rows but a hairline, which gives way to the hover
 * background; `data-tool-row` marks the row for anything that needs to
 * find it from a name inside it.
 */
export function ToolRow({
  adapterId,
  sourceLabel,
  name,
  nameChip,
  description,
  selectable,
  status,
  version,
  action,
  menu,
}: ToolRowProps) {
  const { t } = useTranslation();
  const blurb = description === null || description === "" ? t("toolRow.noDescription") : description;
  return (
    <div
      data-tool-row=""
      className="group relative flex min-h-[60px] items-center gap-3 rounded-row px-3 py-2 transition-colors hover:bg-hover"
    >
      {selectable ? (
        <input
          type="checkbox"
          aria-label={selectable.ariaLabel}
          checked={selectable.checked}
          onChange={selectable.onToggle}
          className="h-4 w-4 shrink-0 cursor-pointer"
        />
      ) : null}
      <SourceAvatar adapterId={adapterId} label={sourceLabel} size="md" />
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5">
          <p title={name} className="truncate text-name font-semibold text-foreground">
            {name}
          </p>
          {nameChip ? (
            <span className="shrink-0 rounded-full border border-border px-1.5 text-[11px] leading-4 text-muted">
              {nameChip}
            </span>
          ) : null}
        </div>
        <p title={blurb} className="truncate text-small text-muted">
          {blurb}
        </p>
      </div>
      {status !== undefined ? (
        <div className="flex shrink-0 items-center gap-1.5">{status}</div>
      ) : null}
      {version !== undefined ? (
        <div className="min-w-24 shrink-0 whitespace-nowrap text-right text-small tabular-nums text-muted">
          {version}
        </div>
      ) : null}
      {action !== undefined ? (
        <div className="flex w-[5.75rem] shrink-0 justify-end">{action}</div>
      ) : null}
      {menu !== undefined ? <div className="flex w-7 shrink-0 justify-end">{menu}</div> : null}
      {/* The hairline under the row, from where its text starts. */}
      <span
        aria-hidden="true"
        className={`pointer-events-none absolute bottom-0 right-3 h-px bg-border transition-opacity group-hover:opacity-0 ${
          selectable ? "left-[5.25rem]" : "left-14"
        }`}
      />
    </div>
  );
}
