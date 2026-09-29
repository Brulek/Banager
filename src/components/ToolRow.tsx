import { Fragment, useLayoutEffect, useRef, useState, type KeyboardEvent, type MouseEvent, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { ArtifactKey } from "../lib/types";
import { keepsOwnMenu } from "../lib/contextMenu";
import { middleCut, textMeasurer } from "../lib/middleCut";
import { ToolAvatar } from "./ToolAvatar";
import { BUTTON } from "./ui/controls";
import { RowMenuContext, type OpenMenuAt } from "./ui/Menu";
import { useElementWidth, useListWidth } from "./VirtualList";
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
  /**
   * The row's one status word (`StatusChip`), in a column of its own just
   * before the version: 120 wide, the word at its left, and there on every
   * row whether it has a word or not -- so that down a list the words
   * start in one line and the versions end in one (spec §3.3).
   */
  status?: ReactNode;
  /** The version column: "7.1 → 7.2", or a word where a version would mean nothing. */
  version?: ReactNode;
  /**
   * The version an update brings, alone: what the version column shows in
   * its place once the window is too narrow for both, after an arrow ("→
   * 7.2" for "7.1 → 7.2"; spec R9) -- a bare "7.2" would read as the
   * version installed -- the whole of it still said to a screen reader.
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
  /**
   * The row is the one selected -- the Installed page's, whose inspector
   * shows it (spec R11): filled as a Mac list fills its selection, the
   * accent with white words while the list has the focus, the grey
   * `row-selected` while it does not (index.css).
   */
  selected?: boolean;
}

/**
 * How much of a row's columns fit in the list's width (spec R9), as the
 * window narrows: `full`, everything; `compact`, the version column says
 * only the version an update brings, after its arrow; `narrow`, that, and
 * the status word moves to the start of the description's line, a dot
 * between them; `minimal` -- a list the Installed page's inspector has
 * narrowed -- the version column goes as well, and an update's versions
 * move to that line after the status word: the inspector says the
 * selected row's version, and the others' are one click away; `tiny` --
 * the inspector's list in the narrowest window -- the row's button goes
 * too, the version it would bring on that line after its arrow, and only
 * the ⋯ stays at the row's end: what the button did is there and in the
 * inspector. Past that, the description is cut short -- never the name --
 * and, left room for no more than a few characters after the words before
 * it, dropped from the line (`DESCRIPTION_MIN_CHARACTERS`).
 */
export type RowFit = "full" | "compact" | "narrow" | "minimal" | "tiny";

/**
 * The widths of the list a row is drawn in -- measured, not the window's
 * -- at which its columns give way (`RowFit`): the list is 752 wide in a
 * window at its default 960, with room for everything, and 592 at its
 * narrowest, 800; beside the inspector, 452 at 960 and under 300 at 800.
 */
export const ROW_FIT_WIDTHS = { full: 700, compact: 640, narrow: 520, minimal: 340 } as const;

/** Which of a row's columns fit a list `width` wide: everything, where nothing measured it. */
export function rowFitFor(width: number | null): RowFit {
  if (width === null || width >= ROW_FIT_WIDTHS.full) return "full";
  if (width >= ROW_FIT_WIDTHS.compact) return "compact";
  if (width >= ROW_FIT_WIDTHS.narrow) return "narrow";
  return width >= ROW_FIT_WIDTHS.minimal ? "minimal" : "tiny";
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
 * The fewest characters of a description worth showing after a status
 * word and a version on its line: with less room than this -- and the dot
 * before it -- it is dropped from sight rather than cut to 「G…」, which
 * says nothing (the inspector says it whole). Measured in the
 * description's own first characters, so a Chinese one asks for more room
 * than a Latin one.
 */
export const DESCRIPTION_MIN_CHARACTERS = 8;

/**
 * The name, on one line: cut short at its end, or -- a very long one, such
 * as `modelscope.cn/Qwen/Qwen2.5-Coder-7B-Instruct-GGUF` -- in its middle,
 * keeping its end, as Finder cuts a long file name: the end is what tells
 * two such names apart. Its whole text in its tooltip either way, and for
 * a screen reader.
 *
 * The middle cut is one string, the start, "…" and the end
 * (`middleCut`), fitted to the room the name's line leaves it -- measured
 * in the name's own font -- so the "…" stands against both halves; two
 * boxes side by side, the first cut at its end, left a hole of up to a
 * glyph before the second. Where nothing can be measured (a test's
 * jsdom), the whole name, cut at its end.
 */
function RowName({ name, lineWidth }: { name: string; lineWidth: number | null }) {
  const className = "min-w-0 truncate text-name font-semibold text-foreground";
  // A name in Latin letters is not Chinese whatever the window's language:
  // said so, its "…" is the system font's, not a full-width Chinese one.
  const lang = /^[\u0020-\u024f]*$/.test(name) ? "en" : undefined;
  const ref = useRef<HTMLParagraphElement>(null);
  const [cut, setCut] = useState<string | null>(null);
  const cuts = name.length > MIDDLE_CUT_FROM;
  useLayoutEffect(() => {
    const paragraph = ref.current;
    const line = paragraph?.parentElement;
    if (!cuts || lineWidth === null || paragraph == null || line == null) {
      setCut(null);
      return;
    }
    const measure = textMeasurer(paragraph);
    if (measure === null) {
      setCut(null);
      return;
    }
    // The line's width less what else stands on it in sight: the source's
    // name, where it follows (R3). Its screen reader's copy takes no room.
    let room = line.getBoundingClientRect().width;
    for (const other of Array.from(line.children)) {
      if (other === paragraph || getComputedStyle(other).position === "absolute") continue;
      room -= other.getBoundingClientRect().width + parseFloat(getComputedStyle(other).marginLeft || "0");
    }
    const fitted = middleCut(name, Math.floor(room), measure, NAME_TAIL);
    setCut(fitted === name ? null : fitted);
  }, [cuts, name, lineWidth]);
  if (cut === null) {
    return (
      <p ref={ref} title={name} lang={lang} data-cut-middle={cuts ? "" : undefined} className={className}>
        {name}
      </p>
    );
  }
  return (
    <p ref={ref} title={name} lang={lang} data-cut-middle="" className={className}>
      <span aria-hidden="true">{cut}</span>
      <span className="sr-only">{name}</span>
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
 * muted), then the columns on the right, 16 apart: the status word in
 * one 120 wide, the version in one at least 120 wide, the row's button in
 * one 80 wide (wider only for a label such as "Uninstall…"), the ⋯ menu
 * in one 24 wide. The status word's column is on every row, so the words
 * line up down a list; the others are drawn whenever their prop is
 * given, even as `null`, so rows that leave one empty still line up with
 * rows that fill it; leave the prop out to drop the column altogether.
 * As the list narrows, they give way in turn (`RowFit`).
 *
 * No corners, no fill and nothing under the pointer but the ⋯ turning a
 * shade darker, as a Mac's list rows have none; a hairline under each,
 * from where its text starts to 20 from the right, and none under the
 * last row of a run (index.css). `data-tool-row` marks the row for
 * anything that needs to find it from a name inside it, `data-status`
 * its status word where it has one, and `data-status-column` the column
 * that holds it, there either way.
 *
 * With `onOpen`, the whole row is a button: one that covers it, under its
 * checkbox, avatar, status, action and menu, which each stay their own
 * control. It takes the focus when pressed -- WebKit leaves a clicked
 * button unfocused -- so whatever it opens can hand the focus back to it.
 * Its focus ring, and the row's own (index.css's, for the keyboard), is
 * drawn inset as the selection is, 10 in and rounded, never square round
 * the row's box.
 *
 * In a list whose rows ↑ and ↓ move between (`VirtualList`'s
 * `keyboardRows`), the row itself takes the focus: Space ticks its
 * checkbox -- or, on a row with none that opens, does what pressing it
 * does -- and Enter does nothing: it opens nothing and starts nothing.
 *
 * `selected` fills it as a Mac list's selection (index.css), with no
 * hairline under it or over it.
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
  selected = false,
}: ToolRowProps) {
  const { t } = useTranslation();
  const fit = rowFitFor(useListWidth());
  const roving = useRovingRow();
  // The name's line, measured only where a long name may be cut in its
  // middle to fit it (`RowName`).
  const [nameLine, setNameLine] = useState<HTMLDivElement | null>(null);
  const nameLineWidth = useElementWidth(name.length > MIDDLE_CUT_FROM ? nameLine : null);
  // The ⋯ menu's way to open at the pointer, which it leaves here (`Menu`).
  const openMenuAt = useRef<OpenMenuAt | null>(null);
  const openButton = useRef<HTMLButtonElement>(null);
  // The description's line, and whether the description has room on it
  // after what goes before it (below).
  const descriptionLine = useRef<HTMLDivElement>(null);
  const [descriptionFits, setDescriptionFits] = useState(true);

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
      // Its checkbox, or -- a row with none that opens (the Installed
      // page's) -- what pressing it does, as Space shows a Finder
      // selection in Quick Look.
      if (selectable !== undefined && selectable !== null) selectable.onToggle();
      else if (selectable === undefined) onOpen?.();
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
  // The status word's column, or -- narrower -- its place at the start of
  // the description's line.
  const statusColumn = fit === "full" || fit === "compact";
  const statusInline = !statusColumn && hasStatus;
  const hasUpdate = newVersion !== undefined && version !== undefined && version !== null;
  // Where the version column has gone (`minimal`, `tiny`): an update's
  // versions on the description's line, and nothing for a row with no
  // update.
  const versionColumn = version !== undefined && (statusColumn || fit === "narrow");
  const versionInline = (fit === "minimal" || fit === "tiny") && hasUpdate;
  const actionColumn = action !== undefined && fit !== "tiny";
  // The version an update brings, alone, after its arrow; the whole
  // change for a screen reader.
  const toNewVersion = (
    <>
      <span aria-hidden="true">{t("updates.versionTo", { target: newVersion })}</span>
      <span className="sr-only">{version}</span>
    </>
  );
  const shownVersion = fit !== "full" && newVersion !== undefined ? toNewVersion : version;
  // What the description's line says before the description, in order,
  // set apart from it and from each other by a dot: the status word, then
  // an update's versions (the whole change beside the inspector, the new
  // version alone in the narrowest list).
  const leading: Array<{ key: string; node: ReactNode }> = [];
  if (statusInline) {
    leading.push({
      key: "status",
      node: (
        <span data-status="" className="relative z-10 flex shrink-0 items-center">
          {status}
        </span>
      ),
    });
  }
  if (versionInline) {
    leading.push({
      key: "version",
      // Cut short only once the description is gone: the description is
      // laid out from nothing and takes what is left (below), so none of
      // the line's shortfall falls on this -- a share of it, however
      // small, would cut its last digits for an "…".
      node: (
        <span data-version="" className="min-w-0 truncate whitespace-nowrap tabular-nums">
          {fit === "tiny" ? toNewVersion : version}
        </span>
      ),
    });
  }
  const dot = (
    <span aria-hidden="true" data-line-dot="" className="shrink-0 whitespace-pre">
      {" · "}
    </span>
  );
  // What goes before the description leaves it the rest of the line; with
  // less than its first few characters' room (`DESCRIPTION_MIN_CHARACTERS`)
  // it leaves the line, and its dot with it -- still said to a screen
  // reader. Measured after every layout, as nothing but the text measures
  // what the words before it take: the room is the line's width less
  // everything on it but the description and its dot, the same whether it
  // is in sight or not. Nothing measured (a line not laid out, or jsdom):
  // it stays, for its box to cut.
  const hasLeading = leading.length > 0;
  useLayoutEffect(() => {
    const line = descriptionLine.current;
    const width = line?.getBoundingClientRect().width ?? 0;
    if (!hasLeading || line == null || width === 0) {
      setDescriptionFits(true);
      return;
    }
    const measure = textMeasurer(line);
    if (measure === null) {
      setDescriptionFits(true);
      return;
    }
    let room = width;
    for (const part of Array.from(line.children)) {
      if (part.hasAttribute("data-description") || part.hasAttribute("data-description-dot")) continue;
      room -= part.getBoundingClientRect().width;
    }
    setDescriptionFits(room >= measure(` · ${description.slice(0, DESCRIPTION_MIN_CHARACTERS)}`));
  });
  const descriptionShown = !hasLeading || descriptionFits;
  const descriptionText = (
    <span
      title={description}
      data-description=""
      className={
        descriptionShown
          ? `min-w-0 truncate ${versionInline ? "flex-1" : ""} ${selectableDescription ? "select-text" : ""}`
          : "sr-only"
      }
    >
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
        data-selected={selected ? "" : undefined}
        className="group/row relative isolate flex h-13 items-center px-5"
      >
        {selected ? (
          // The selection's fill, 10 in from either side with a control's
          // corners, as a Mac's inset list draws it, under the row's words
          // (the row is a stacking context of its own for it); its colour,
          // and the words' over it, are index.css's.
          <span
            aria-hidden="true"
            data-row-selection=""
            className="pointer-events-none absolute inset-y-0 left-2.5 right-2.5 -z-10 rounded-control"
          />
        ) : null}
        {onOpen ? (
          <button
            ref={openButton}
            type="button"
            aria-label={openLabel}
            aria-pressed={selected}
            data-row-open=""
            // In a list the arrow keys move through, the row itself is
            // what Tab and ↑ ↓ reach; this is for the pointer.
            tabIndex={roving === null ? undefined : -1}
            onClick={open}
            className="absolute inset-0"
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
          <div ref={setNameLine} className="flex min-w-0 items-baseline">
            <RowName name={name} lineWidth={nameLineWidth} />
            {source !== undefined ? (
              <span className={showSource ? "ml-1.5 shrink-0 text-small text-muted" : "sr-only"}>{source}</span>
            ) : null}
          </div>
          <div ref={descriptionLine} className="mt-0.5 flex min-w-0 items-center text-small text-muted">
            {leading.map((part, index) => (
              <Fragment key={part.key}>
                {index > 0 ? dot : null}
                {part.node}
              </Fragment>
            ))}
            {hasLeading && descriptionShown ? (
              <span aria-hidden="true" data-line-dot="" data-description-dot="" className="shrink-0 whitespace-pre">
                {" · "}
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
        {statusColumn ? (
          // 120 wide on every row, empty or not, the word at its left: the
          // words start in one line down the list (wider only for a word
          // that needs it).
          <div
            data-status-column=""
            data-status={hasStatus ? "" : undefined}
            className="relative z-10 ml-4 flex min-w-30 shrink-0 items-center justify-start"
          >
            {status}
          </div>
        ) : null}
        {versionColumn ? (
          // At least as wide as a usual change ("2.1.282 → 2.1.290"), so the
          // status column before it stands in one place down the list.
          <div
            data-version=""
            className={`ml-4 shrink-0 whitespace-nowrap text-right text-body tabular-nums text-muted ${
              fit === "full" ? "min-w-30" : "min-w-20"
            }`}
          >
            {shownVersion}
          </div>
        ) : null}
        {actionColumn ? (
          <div className="relative z-10 ml-4 flex min-w-20 shrink-0 justify-end">{action}</div>
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
