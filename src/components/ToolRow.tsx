import { useRef, type KeyboardEvent, type MouseEvent, type ReactNode } from "react";
import type { ArtifactKey } from "../lib/types";
import { keepsOwnMenu } from "../lib/contextMenu";
import { ToolAvatar } from "./ToolAvatar";
import { BUTTON } from "./ui/controls";
import { RowMenuContext, type OpenMenuAt } from "./ui/Menu";
import { useListWidth } from "./VirtualList";
import { useRovingRow } from "./rovingRows";

export interface RowActionProps {
  /** Handed the event, so what it opens can hand the focus back to the button. */
  onClick: (event: MouseEvent<HTMLButtonElement>) => void;
  disabled?: boolean;
  children: ReactNode;
}

/**
 * A row's own button, for `ToolRow`'s `action`: Update, Retry or
 * Uninstall…. A regular grey button, whichever it is (spec §3.5): the list
 * recommends none of them over the rest -- the accent is kept for the one
 * thing a screen asks for -- and Uninstall is not tinted red, under the
 * pointer or not, as a Mac's button for something the user chose is not.
 */
export function RowAction({ onClick, disabled, children }: RowActionProps) {
  return (
    <button type="button" onClick={onClick} disabled={disabled} className={BUTTON.regular.grey}>
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
   * Whether the source's name follows the tool's, in small muted words:
   * where the list being shown has this name under more than one source
   * (spec R3), as Mail names the account beside a mailbox two accounts
   * have. Elsewhere the avatar's mark says the source, with its name in
   * the avatar's tooltip and, for a screen reader, after the tool's name.
   */
  showSource?: boolean;
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
   * A few words after the description, on its line and set apart from it
   * by space alone, that stay whole when the row is too narrow for both --
   * the description gives way to them: the Unknown page's "Points into
   * Docker.app". Never selected with the description.
   */
  descriptionNote?: string;
  /**
   * A checkbox before the avatar, for a list that acts on several rows;
   * `null` for a row of such a list that has none -- an update under way,
   * one that can't be updated here -- which keeps the checkbox's room, so
   * its avatar stays in line with the rows around it.
   */
  selectable?: ToolRowSelectable | null;
  /** The row's one status word (`StatusChip`), just before the version. */
  status?: ReactNode;
  /** The version column: "7.1 → 7.2", or a word where a version would mean nothing. */
  version?: ReactNode;
  /**
   * The version an update brings, alone: what the version column shows in
   * its place once the window is too narrow for both ("7.2" for "7.1 →
   * 7.2"; spec R9), the whole of it still said to a screen reader.
   */
  newVersion?: string;
  /** The row's own button (`RowAction`), or what stands in for it, such as an update's progress. */
  action?: ReactNode;
  /** The ⋯ menu (`Menu`), which a right-click anywhere on the row opens too, at the pointer. */
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
 * How much of a row's columns fit in the list's width (spec R9), as the
 * window narrows: `full`, everything; `compact`, the version column says
 * only the version an update brings; `narrow`, that, and the status word
 * moves to the start of the description's line. Past that, the
 * description is cut short -- never the name, never the row's button.
 */
export type RowFit = "full" | "compact" | "narrow";

/**
 * The list widths at which a row's columns give way (`RowFit`): the list
 * is 752 wide in a window at its default 960, with room for everything,
 * and 592 at its narrowest, 800.
 */
export const ROW_FIT_WIDTHS = { full: 700, compact: 640 } as const;

/** Which of a row's columns fit a list `width` wide: everything, where nothing measured it. */
export function rowFitFor(width: number | null): RowFit {
  if (width === null || width >= ROW_FIT_WIDTHS.full) return "full";
  return width >= ROW_FIT_WIDTHS.compact ? "compact" : "narrow";
}

/**
 * A name longer than this is cut short in its middle when it does not fit,
 * keeping its last `NAME_TAIL` characters: a model's tag and quantisation
 * ("…Instruct-GGUF:Q4_K_M"). Shorter names are cut at their end, as any
 * other text -- which at the window's narrowest they do not reach.
 */
export const MIDDLE_CUT_FROM = 32;
const NAME_TAIL = 12;

/**
 * The name, on one line: cut short at its end, or -- a very long one, such
 * as `modelscope.cn/Qwen/Qwen2.5-Coder-7B-Instruct-GGUF` -- in its middle,
 * keeping its end, as Finder cuts a long file name: the end is what tells
 * two such names apart. Its whole text in its tooltip either way, and as
 * the paragraph's text: the two halves, one after the other.
 */
function RowName({ name }: { name: string }) {
  const className = "min-w-0 text-name font-semibold text-foreground";
  if (name.length <= MIDDLE_CUT_FROM) {
    return (
      <p title={name} className={`truncate ${className}`}>
        {name}
      </p>
    );
  }
  return (
    <p title={name} data-cut-middle="" className={`flex ${className}`}>
      <span className="min-w-0 truncate">{name.slice(0, -NAME_TAIL)}</span>
      <span className="shrink-0 whitespace-pre">{name.slice(-NAME_TAIL)}</span>
    </p>
  );
}

/**
 * One tool on a list: the Updates and Installed pages' rows, and the
 * Unknown page's programs, with an avatar of their own -- laid out as a Mac
 * list's rows are (spec §3.3, R2; native-sui-rows-light.png,
 * latest-updates-2025.png): 52 high, the content 20 in from either side;
 * the checkbox, the avatar 12 after it, 32 square, then 12 after that the
 * name (13, semibold) with its one line of description under it (11,
 * muted), then the columns on the right, 16 apart: the status word, the
 * version, the row's button in a column 80 wide, the ⋯ menu in one 24
 * wide. A column is drawn whenever its prop is given, even as `null`, so
 * rows that leave one empty still line up with rows that fill it; leave
 * the prop out to drop the column altogether.
 *
 * No corners, no fill and nothing under the pointer but the ⋯ turning a
 * shade darker, as a Mac's list rows have none; a hairline under each,
 * from where its text starts to 20 from the right, and none under the
 * last row of a run (index.css). `data-tool-row` marks the row for
 * anything that needs to find it from a name inside it, and `data-status`
 * its status word.
 *
 * With `onOpen`, the whole row is a button: one that covers it, under its
 * checkbox, avatar, status, action and menu, which each stay their own
 * control. It takes the focus when pressed -- WebKit leaves a clicked
 * button unfocused -- so whatever it opens can hand the focus back to it.
 * Its focus ring (index.css's, for the keyboard) is drawn just inside the
 * row, where the list's own edge cannot clip it.
 *
 * In a list whose rows ↑ and ↓ move between (`VirtualList`'s
 * `keyboardRows`), the row itself takes the focus: Space ticks its
 * checkbox, and Enter does nothing -- it opens nothing and starts nothing.
 */
export function ToolRow({
  adapterId,
  sourceLabel,
  iconKey,
  avatar,
  name,
  showSource = false,
  description,
  selectableDescription = false,
  descriptionNote,
  selectable,
  status,
  version,
  newVersion,
  action,
  menu,
  onOpen,
  openLabel,
}: ToolRowProps) {
  const fit = rowFitFor(useListWidth());
  const roving = useRovingRow();
  // The ⋯ menu's way to open at the pointer, which it leaves here (`Menu`).
  const openMenuAt = useRef<OpenMenuAt | null>(null);
  const openButton = useRef<HTMLButtonElement>(null);

  const open = (event: MouseEvent<HTMLButtonElement>) => {
    event.currentTarget.focus();
    onOpen?.();
  };
  // A right-click anywhere on the row opens its ⋯ menu at the pointer --
  // except on selected text, where the web view keeps its own, to copy it.
  const onContextMenu = (event: MouseEvent<HTMLDivElement>) => {
    if (openMenuAt.current === null || keepsOwnMenu(event.nativeEvent, window.getSelection())) return;
    event.preventDefault();
    openMenuAt.current(event.clientX, event.clientY);
  };
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.target !== event.currentTarget) return;
    if (event.key === " ") {
      event.preventDefault();
      selectable?.onToggle();
    } else if (event.key === "Enter") {
      event.preventDefault();
    }
  };

  // The source's name, where the avatar's mark alone says it: after the
  // tool's name, for a screen reader (the avatar is decorative) or in
  // sight (`showSource`), and in the avatar's tooltip. Not for a tool with
  // its own installer, its own source: its name would only come twice.
  const source = sourceLabel !== undefined && sourceLabel !== name ? sourceLabel : undefined;
  const hasStatus = status !== undefined && status !== null;
  const statusInline = fit === "narrow" && hasStatus;
  const shownVersion =
    fit !== "full" && newVersion !== undefined ? (
      <>
        <span aria-hidden="true">{newVersion}</span>
        <span className="sr-only">{version}</span>
      </>
    ) : (
      version
    );
  const descriptionText = (
    <span title={description} className={`min-w-0 truncate ${selectableDescription ? "select-text" : ""}`}>
      {description}
    </span>
  );

  return (
    <RowMenuContext.Provider value={openMenuAt}>
      <div
        data-tool-row=""
        data-row-focus={roving === null ? undefined : ""}
        tabIndex={roving?.tabIndex}
        onFocus={roving?.onFocus}
        onKeyDown={roving === null ? undefined : onKeyDown}
        onContextMenu={onContextMenu}
        className="group/row relative flex h-13 items-center px-5 -outline-offset-3"
      >
        {onOpen ? (
          <button
            ref={openButton}
            type="button"
            aria-label={openLabel}
            onClick={open}
            className="absolute inset-0 -outline-offset-3"
          />
        ) : null}
        {selectable !== undefined ? (
          <span className="relative z-10 mr-3 flex w-4 shrink-0">
            {selectable !== null ? (
              <input
                type="checkbox"
                aria-label={selectable.ariaLabel}
                checked={selectable.checked}
                onChange={selectable.onToggle}
                className="h-4 w-4"
              />
            ) : null}
          </span>
        ) : null}
        {avatar ?? (
          // Over the row's own button, for its tooltip: the source's name,
          // which the mark on the avatar's corner is all the row shows of.
          // Pressed, it opens the row all the same.
          <span title={source} onClick={() => openButton.current?.click()} className="relative z-10 flex shrink-0">
            <ToolAvatar adapterId={adapterId ?? ""} sourceLabel={sourceLabel ?? ""} iconKey={iconKey} />
          </span>
        )}
        <div className="ml-3 min-w-0 flex-1">
          <div className="flex min-w-0 items-baseline">
            <RowName name={name} />
            {source !== undefined ? (
              <span className={showSource ? "ml-1.5 shrink-0 text-small text-muted" : "sr-only"}>{source}</span>
            ) : null}
          </div>
          <div className="mt-0.5 flex min-w-0 items-center text-small text-muted">
            {statusInline ? (
              <span data-status="" className="relative z-10 mr-2 flex shrink-0 items-center">
                {status}
              </span>
            ) : null}
            {descriptionText}
            {descriptionNote !== undefined ? (
              // The description gives way to the note, cut short first; the
              // note is cut short only on a row too narrow for it alone.
              // Space sets the two apart, not a dot between them.
              <span className="max-w-full shrink-0 truncate pl-3">{descriptionNote}</span>
            ) : null}
          </div>
        </div>
        {status !== undefined && !statusInline ? (
          <div data-status="" className="relative z-10 ml-4 flex shrink-0 items-center">
            {status}
          </div>
        ) : null}
        {version !== undefined ? (
          <div className="ml-4 min-w-16 shrink-0 whitespace-nowrap text-right text-body tabular-nums text-muted">
            {shownVersion}
          </div>
        ) : null}
        {action !== undefined ? (
          <div className="relative z-10 ml-4 flex w-20 shrink-0 justify-end">{action}</div>
        ) : null}
        {menu !== undefined ? <div className="relative z-10 ml-4 flex w-6 shrink-0 justify-end">{menu}</div> : null}
        {/* The hairline under the row, from where its text starts to 20
            from the right; hidden under a run's last row (index.css). */}
        <span
          aria-hidden="true"
          data-row-separator=""
          className={`pointer-events-none absolute bottom-0 right-5 h-px bg-separator ${
            selectable !== undefined ? "left-[5.75rem]" : "left-16"
          }`}
        />
      </div>
    </RowMenuContext.Provider>
  );
}
