import type { MouseEvent, ReactNode } from "react";
import type { ArtifactKey } from "../lib/types";
import { ToolAvatar } from "./ToolAvatar";

/**
 * How a row's own button looks: `accent` for what the row recommends --
 * Update, 360's one obvious button -- and `quiet` for what it only offers:
 * Uninstall, an outline in the muted colour that takes the danger colour
 * only under the pointer or the keyboard's focus, so that removing
 * something never looks like the thing to do.
 */
export type RowActionTone = "accent" | "quiet";

const ROW_ACTION_CLASSES: Record<RowActionTone, string> = {
  accent:
    "h-7 rounded-button bg-accent/10 px-3.5 text-body font-semibold text-accent-text outline-none transition-colors hover:bg-accent hover:text-accent-foreground focus-visible:ring-2 focus-visible:ring-accent disabled:opacity-50 disabled:hover:bg-accent/10 disabled:hover:text-accent-text",
  quiet:
    "h-7 rounded-button border border-border px-3.5 text-body font-medium text-muted outline-none transition-colors hover:border-danger/40 hover:bg-danger/10 hover:text-danger focus-visible:border-danger/40 focus-visible:text-danger focus-visible:ring-2 focus-visible:ring-danger/40 disabled:opacity-50 disabled:hover:border-border disabled:hover:bg-transparent disabled:hover:text-muted",
};

export interface RowActionProps {
  tone: RowActionTone;
  /** Handed the event, so what it opens can hand the focus back to the button. */
  onClick: (event: MouseEvent<HTMLButtonElement>) => void;
  disabled?: boolean;
  children: ReactNode;
}

/** A row's own button, for `ToolRow`'s `action`: Update or Uninstall. `data-tone` says which look it has. */
export function RowAction({ tone, onClick, disabled, children }: RowActionProps) {
  return (
    <button type="button" data-tone={tone} onClick={onClick} disabled={disabled} className={ROW_ACTION_CLASSES[tone]}>
      {children}
    </button>
  );
}

export interface ToolRowSelectable {
  checked: boolean;
  onToggle: () => void;
  /** The checkbox's accessible name: "Select glib for update". */
  ariaLabel: string;
}

/**
 * What stands at the start of a row: the tool's avatar (`ToolAvatar`) --
 * its app's own icon for a cask or its logo, by `iconKey`, with the
 * source's mark on its corner, and otherwise the source's, by its adapter
 * id and name, the logo or the colour and the letter -- or, for a row that
 * belongs to no source, such as the Unknown page's, an avatar of its own.
 */
export type ToolRowAvatarProps =
  | { adapterId: string; sourceLabel: string; iconKey?: ArtifactKey; avatar?: never }
  | { avatar: ReactNode; adapterId?: never; sourceLabel?: never; iconKey?: never };

export type ToolRowProps = ToolRowAvatarProps & ToolRowContentProps;

export interface ToolRowContentProps {
  /** The tool's name as the user knows it. */
  name: string;
  /**
   * A quiet chip after the name, such as the source's name on a list that
   * mixes sources. Leave it out where it would only repeat the name (a
   * tool with its own installer is its own source).
   */
  nameChip?: string;
  /**
   * One line about what it is: its line in the window's language, the
   * source's description, or what the source says it is when it gave none
   * (`toolDescription` in src/lib/sources.ts), so a row never reads "No
   * description".
   */
  description: string;
  /**
   * The description is text a user copies, and selects (`select-text`):
   * the Unknown page's path. A tool's own description does not, as no
   * other text on a row does. Not with `onOpen`, whose button lies over
   * the row's text.
   */
  selectableDescription?: boolean;
  /**
   * A few words after the description, on its line, that stay whole when
   * the row is too narrow for both -- the description gives way to them:
   * the Unknown page's "Points into Docker.app". Never selected with the
   * description.
   */
  descriptionNote?: string;
  /** A checkbox before the avatar, for a list that acts on several rows. */
  selectable?: ToolRowSelectable;
  /** Status chips (`StatusChip`), just before the version. */
  status?: ReactNode;
  /** The version column: "7.1 → 7.2", or a word where a version would mean nothing. */
  version?: ReactNode;
  /** The row's own button (`RowAction`), or what stands in for it, such as an update's progress. */
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
 * One tool on a list: the Updates and Installed pages' rows
 * (docs/superpowers/2026-09-27-ui-redesign.md, 更新页 and 已安装页), and
 * the Unknown page's programs, with an avatar of their own.
 *
 * The avatar -- an app's own icon, the tool's logo, or the source's -- the
 * name with its one line of description under it, then the columns on the
 * right: status chips, the version, the primary action and the ⋯ menu. A
 * column is drawn whenever its prop is given, even as `null`, so rows that
 * leave one empty still line up with rows that fill it; leave the prop out
 * to drop the column altogether.
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
  iconKey,
  avatar,
  name,
  nameChip,
  description,
  selectableDescription = false,
  descriptionNote,
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
          className="relative z-10 h-4 w-4 shrink-0"
        />
      ) : null}
      {avatar ?? <ToolAvatar adapterId={adapterId ?? ""} sourceLabel={sourceLabel ?? ""} iconKey={iconKey} />}
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-1.5">
          <p title={name} className="truncate text-name font-semibold text-foreground">
            {name}
          </p>
          {/* Gives way to the name on a narrow row -- a window under
              928px, down to its narrowest, 800px; the default 960px has
              room for it -- where the avatar still shows the source.
              Hidden from sight only: the avatar is decorative, so this is
              the only place a screen reader hears the source. */}
          {nameChip ? (
            <span className="shrink-0 rounded-full border border-border px-1.5 text-[11px] leading-4 text-muted @max-2xl:sr-only">
              {nameChip}
            </span>
          ) : null}
        </div>
        {descriptionNote === undefined ? (
          <p
            title={description}
            className={`truncate text-small text-muted ${selectableDescription ? "select-text" : ""}`}
          >
            {description}
          </p>
        ) : (
          // The description gives way to the note, cut short first; the
          // note is cut short only on a row too narrow for it alone.
          <p className="flex min-w-0 text-small text-muted">
            <span
              title={description}
              className={`min-w-0 truncate ${selectableDescription ? "select-text" : ""}`}
            >
              {description}
            </span>
            <span className="flex max-w-full shrink-0">
              {/* The dot a size and a date have between them, in both
                  languages; for the eye only. */}
              <span aria-hidden="true" className="shrink-0 px-1">
                ·
              </span>
              <span className="min-w-0 truncate">{descriptionNote}</span>
            </span>
          </p>
        )}
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
