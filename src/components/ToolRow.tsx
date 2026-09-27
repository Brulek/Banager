import type { MouseEvent, ReactNode } from "react";
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
   * One line about what it is: the source's description, or what the
   * source says it is when it gave none (`toolDescription` in
   * src/lib/sources.ts), so a row never reads "No description".
   */
  description: string;
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
  /**
   * What pressing the row itself does -- anywhere but its own controls:
   * the Installed page opens the row's details. The row is then a button
   * under its controls, reached with Tab before them.
   */
  onOpen?: () => void;
  /** That button's accessible name: 「详情：jq」/"Details: jq". */
  openLabel?: string;
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
 * find it from a name inside it, and `data-status` its chips.
 *
 * With `onOpen`, the whole row is a button: one that covers it, under its
 * checkbox, chips, action and menu, which each stay their own control. It
 * takes the focus when pressed -- WebKit leaves a clicked button unfocused
 * -- so whatever it opens can hand the focus back to it.
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
  onOpen,
  openLabel,
}: ToolRowProps) {
  const open = (event: MouseEvent<HTMLButtonElement>) => {
    event.currentTarget.focus();
    onOpen?.();
  };
  return (
    <div
      data-tool-row=""
      className="@container group relative flex min-h-[60px] items-center gap-3 rounded-row px-3 py-2 transition-colors hover:bg-hover"
    >
      {onOpen ? (
        <button
          type="button"
          aria-label={openLabel}
          onClick={open}
          className="absolute inset-0 rounded-row outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent"
        />
      ) : null}
      {selectable ? (
        <input
          type="checkbox"
          aria-label={selectable.ariaLabel}
          checked={selectable.checked}
          onChange={selectable.onToggle}
          className="relative z-10 h-4 w-4 shrink-0 cursor-pointer"
        />
      ) : null}
      <SourceAvatar adapterId={adapterId} label={sourceLabel} size="md" />
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5">
          <p title={name} className="truncate text-name font-semibold text-foreground">
            {name}
          </p>
          {/* Gives way to the name on a narrow row -- the window's
              default 800px -- where the avatar still says the source. */}
          {nameChip ? (
            <span className="shrink-0 rounded-full border border-border px-1.5 text-[11px] leading-4 text-muted @max-2xl:hidden">
              {nameChip}
            </span>
          ) : null}
        </div>
        <p title={description} className="truncate text-small text-muted">
          {description}
        </p>
      </div>
      {status !== undefined ? (
        <div data-status="" className="relative z-10 flex shrink-0 items-center gap-1.5">
          {status}
        </div>
      ) : null}
      {version !== undefined ? (
        <div className="min-w-16 shrink-0 whitespace-nowrap text-right text-small tabular-nums text-muted @2xl:min-w-24">
          {version}
        </div>
      ) : null}
      {action !== undefined ? (
        <div className="relative z-10 flex w-[5.75rem] shrink-0 justify-end">{action}</div>
      ) : null}
      {menu !== undefined ? <div className="relative z-10 flex w-7 shrink-0 justify-end">{menu}</div> : null}
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
