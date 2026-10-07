import { Fragment, useId, useLayoutEffect, useRef, useState, type KeyboardEvent, type MouseEvent, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { ArtifactKey } from "../lib/types";
import { keepsOwnMenu } from "../lib/contextMenu";
import { endCut, middleCut, textMeasurer } from "../lib/middleCut";
import { ToolAvatar } from "./ToolAvatar";
import { BUTTON } from "./ui/controls";
import { RowMenuContext, type OpenMenuAt } from "./ui/Menu";
import { useListWidth, useRowFit, useStatusColumn } from "./VirtualList";
import { useRovingRow } from "./rovingRows";

// Which columns fit, by the list's width: its own module (./rowFit.ts), said here too.
export { ROW_FIT_WIDTHS, rowFitFor, type RowFit } from "./rowFit";

export interface RowActionProps {
  /** Handed the event, so what it opens can hand the focus back to the button. */
  onClick: (event: MouseEvent<HTMLButtonElement>) => void;
  disabled?: boolean;
  /**
   * Its accessible name: its words with the tool's name in them, the
   * words first -- 「更新git」, "Uninstall git…" -- as every row has the
   * same button, and a screen reader's list of buttons would otherwise be
   * ten "Update"s.
   */
  ariaLabel?: string;
  /** Why it is off, as a tooltip, where the row has no word that says so. */
  title?: string;
  children: ReactNode;
}

/**
 * A row's own button, for `ToolRow`'s `action`: Update, Retry or
 * Uninstall…. A regular grey button, whichever it is (spec §3.5): the list
 * recommends none of them over the rest -- the accent is kept for the one
 * thing a screen asks for -- and Uninstall is not tinted red, under the
 * pointer or not, as a Mac's button for something the user chose is not.
 */
export function RowAction({ onClick, disabled, ariaLabel, title, children }: RowActionProps) {
  const button = (
    <button type="button" aria-label={ariaLabel} onClick={onClick} disabled={disabled} className={BUTTON.regular.grey}>
      {children}
    </button>
  );
  // On a box around it: a disabled button gets no pointer, so no tooltip of its own.
  return title === undefined ? (
    button
  ) : (
    <span title={title} data-row-action-why="" className="inline-flex">
      {button}
    </span>
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
 * its app's own icon for a cask or its logo, by `iconKey`, and otherwise
 * the neutral grey tile, a prompt on it for a command-line program, each
 * with the source's mark, by its adapter id and name, on its corner: the
 * source's logo, or its colour and letter where it has none -- or, for a
 * row that belongs to no source, such as the Unknown page's, an avatar of
 * its own.
 */
export type ToolRowAvatarProps =
  | { adapterId: string; sourceLabel: string; iconKey?: ArtifactKey; avatar?: never }
  | { avatar: ReactNode; adapterId?: never; sourceLabel?: never; iconKey?: never };

export type ToolRowProps = ToolRowAvatarProps & ToolRowContentProps;

export interface ToolRowContentProps {
  /** The tool's name as the user knows it. */
  name: string;
  /**
   * For a name that is a path -- an Ollama model pulled from another
   * registry (`modelPath` in src/lib/names.ts) -- its two parts: `name`,
   * the last, which the row shows as the tool's name, cut at its end if it
   * still does not fit, never in its middle; and `from`, the rest, at the
   * start of the description's line. The whole name stays the row's
   * tooltip and what a screen reader hears.
   */
  namePath?: { name: string; from: string } | null;
  /**
   * Whether the source's name follows the tool's, in small muted words:
   * where the list being shown has this name under more than one source
   * (spec R3), as Mail names the account beside a mailbox two accounts
   * have. Elsewhere the avatar's mark says the source, with its name in
   * the avatar's tooltip and, for a screen reader, after the tool's name.
   * In sight, they give way to the tool's name, cut short first, and are
   * whole in their tooltip (`RowNote`).
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
   * by space alone: the Unknown page's "Points into Docker.app". Where the
   * row is too narrow for both, they give way first, cut short -- the
   * description, a path there, is what the row is about, and keeps up to
   * 70% of the line, whole if it fits (walk-3 W3-8). Never selected with
   * the description.
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
   * start in one line and the versions end in one (spec §3.3) -- in a list
   * where any row has one (`StatusColumnContext`). A row with a word keeps
   * the column whatever its list says, so no word is ever lost.
   */
  status?: ReactNode;
  /**
   * `status`'s word as words -- 「已跳过2.102.0」, "Can't check" -- for the
   * row's accessible name, where the row takes the focus (below): `status`
   * is drawn, and a name is a string.
   */
  statusText?: string;
  /** The version column: "7.1 → 7.2", or a word where a version would mean nothing. */
  version?: ReactNode;
  /**
   * What the version column says, as words for the row's accessible name,
   * where it says something other than a version: the Installed page's
   * size under By Size, 「占用空间约71.3 MB」. A screen reader that hears
   * the row's name then hears what the list is sorted by, with its term.
   */
  versionText?: string;
  /**
   * The version an update brings, alone: what the version column shows in
   * its place once the window is too narrow for both, after an arrow ("→
   * 7.2" for "7.1 → 7.2"; spec R9) -- a bare "7.2" would read as the
   * version installed -- the whole of it still said to a screen reader.
   * Where the row has room for the whole change with its name whole, it
   * shows the whole change all the same (`version` given as a string).
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
   * What Enter on the row itself does, in a list whose rows take the
   * focus: the Installed page shows the row's details and puts the focus
   * in them -- the inspector is otherwise every control of the list away
   * by Tab. Left out, Enter does nothing.
   */
  onEnter?: () => void;
  /**
   * The row is the one selected -- the Installed page's, whose inspector
   * shows it (spec R11): filled as a Mac list fills its selection, the
   * accent with white words while the list has the focus, the grey
   * `row-selected` while it does not (index.css).
   */
  selected?: boolean;
}


/**
 * A name longer than this is cut short in its middle when it does not fit,
 * keeping its last `NAME_TAIL` characters: a scoped npm package's own name
 * ("…server-filesystem"). Shorter names are cut at their end, as any other
 * text -- which at the window's narrowest they do not reach -- and so is a
 * model's last path segment (`namePath`).
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
 * as `@modelcontextprotocol/server-filesystem` -- in its middle, keeping its
 * end, as Finder cuts a long file name: the end is what tells two such
 * names apart. A model's path shows only its last segment (`shown`), cut at
 * its end. Its whole text in its tooltip either way, and for a screen
 * reader.
 *
 * The middle cut is one string, the start, "…" and the end
 * (`middleCut`), fitted to the room the name's line leaves it -- measured
 * in the name's own font -- so the "…" stands against both halves; two
 * boxes side by side, the first cut at its end, left a hole of up to a
 * glyph before the second. Where nothing can be measured (a test's
 * jsdom), the whole name, cut at its end.
 *
 * Only a name long enough to be cut so is measured (`MiddleCutName`): any
 * other is drawn once, as it is, and reads nothing of the layout -- a list
 * mounts a dozen rows at every step of a scroll.
 */
function RowName({ name, shown }: { name: string; shown?: string }) {
  // A name in Latin letters is not Chinese whatever the window's language:
  // said so, its "…" is the system font's, not a full-width Chinese one.
  const lang = /^[\u0020-\u024f]*$/.test(shown ?? name) ? "en" : undefined;
  if (shown !== undefined) {
    return (
      <p title={name} lang={lang} data-name-path="" className={NAME_CLASS}>
        <span aria-hidden="true">{shown}</span>
        <span className="sr-only">{name}</span>
      </p>
    );
  }
  if (name.length > MIDDLE_CUT_FROM) return <MiddleCutName name={name} lang={lang} />;
  return (
    <p title={name} lang={lang} className={NAME_CLASS}>
      {name}
    </p>
  );
}

/**
 * The name takes the room it needs on its line and gives none of it to the
 * source's words after it (`showSource`): they give way first, cut short
 * (`NOTE_CLASS`), and the name is cut only where it alone is wider than the
 * whole line (`max-w-full`) -- 「w…」 beside a whole
 * 「pip（/opt/homebrew/bin/python3.11）」 said nothing of which package it was.
 */
const NAME_CLASS = "max-w-full shrink-0 truncate text-name font-semibold text-foreground";

/**
 * The source's words after the name, in sight (`showSource`): small and
 * muted, and the first thing on the name's line to give way (`RowNote`) --
 * their whole text their tooltip, and what a screen reader reads. Marked
 * `data-row-note`, so that nothing measuring the name's room counts what is
 * left of them as taken.
 */
const NOTE_CLASS = "ml-1.5 min-w-0 truncate text-small text-muted";

/**
 * How many of the source's words' last characters a cut keeps (`RowNote`):
 * where two sources of one kind differ -- 「…/python3.11）」 beside
 * 「…/python3）」, 「…（Intel）」 beside 「…（Apple芯片）」.
 */
const NOTE_TAIL = 12;

/**
 * The source's words after the name (`showSource`), fitted to what the name
 * leaves of its line after every draw: whole where they fit, and else cut
 * in their middle, keeping their end (`NOTE_TAIL`), as Finder cuts a long
 * file name -- the start says the kind of source, which the avatar's mark
 * says too; the end is what tells two of a kind apart -- or, with no room
 * for a start, the end alone after a "…", or nothing (`endCut`). Whole in
 * their tooltip and to a screen reader. The name takes its room first
 * (`NAME_CLASS`), so its width does not hang on what this shows, and
 * fitting this draws only these words again, not the row. Read to be
 * drawn again with every new width of the list, as `MiddleCutName` is.
 * Where nothing can be measured (a test's jsdom), whole, for its box to
 * cut at its end.
 */
function RowNote({ note }: { note: string }) {
  useListWidth();
  const ref = useRef<HTMLSpanElement>(null);
  const [cut, setCut] = useState<string | null>(null);
  useLayoutEffect(() => {
    const span = ref.current;
    const line = span?.parentElement;
    const lineWidth = line?.getBoundingClientRect().width ?? 0;
    const measure = span == null || line == null || lineWidth === 0 ? null : textMeasurer(span);
    let fitted: string | null = null;
    if (measure !== null && span != null && line != null) {
      // The line less the space before the words and what else stands on
      // it in sight: the name. A screen reader's copy takes no room.
      let room = lineWidth - (parseFloat(getComputedStyle(span).marginLeft) || 0);
      for (const other of Array.from(line.children)) {
        if (other === span || getComputedStyle(other).position === "absolute") continue;
        room -= other.getBoundingClientRect().width + (parseFloat(getComputedStyle(other).marginLeft) || 0);
      }
      // Too little room for a start, "…" and the tail (a list beside the
      // inspector in the narrowest window): the end alone, after a "…", or
      // nothing where not even that fits (`endCut`).
      const cutRoom = Math.floor(room);
      const middle = middleCut(note, cutRoom, measure, NOTE_TAIL);
      const shown = middle === note || measure(middle) <= cutRoom ? middle : endCut(note, cutRoom, measure);
      fitted = shown === note ? null : shown;
    }
    // Set only when it changes: a state set to what it already is can
    // still draw the words once more.
    if (fitted !== cut) setCut(fitted);
  });
  return (
    <span ref={ref} title={note} data-row-note="" className={NOTE_CLASS}>
      {cut === null ? (
        note
      ) : (
        <>
          <span aria-hidden="true">{cut}</span>
          <span className="sr-only">{note}</span>
        </>
      )}
    </span>
  );
}

/**
 * A name long enough to be cut in its middle (`RowName`), fitted to its
 * line after every draw. The line takes what the row's columns leave it,
 * whatever the name says, and this is drawn again whenever that can
 * change -- the list's width (`useListWidth`), or what a column of its row
 * shows -- so it needs no observer of its own. The cut is its own state:
 * fitting it draws the name again, not the row.
 */
function MiddleCutName({ name, lang }: { name: string; lang: string | undefined }) {
  // Read to be drawn again, and fitted again, with every new width of the
  // list, which its row is not drawn again for while the same columns fit.
  useListWidth();
  const ref = useRef<HTMLParagraphElement>(null);
  const [cut, setCut] = useState<string | null>(null);
  useLayoutEffect(() => {
    const paragraph = ref.current;
    const line = paragraph?.parentElement;
    const lineWidth = line?.getBoundingClientRect().width ?? 0;
    // A line not laid out (a test's jsdom): whole, for its box to cut.
    const measure = paragraph == null || line == null || lineWidth === 0 ? null : textMeasurer(paragraph);
    let fitted: string | null = null;
    if (measure !== null && line != null) {
      // The line's width less what else stands on it in sight and does not
      // give way to the name. The source's words after it (R3) do
      // (`NOTE_CLASS`): the name is fitted to the whole line and they take
      // what is left, so what is left of them is not counted -- counted,
      // each fitting would leave them more and the next cut the name
      // shorter. A screen reader's copy takes no room.
      let room = lineWidth;
      for (const other of Array.from(line.children)) {
        if (other === paragraph || other.hasAttribute("data-row-note")) continue;
        if (getComputedStyle(other).position === "absolute") continue;
        room -= other.getBoundingClientRect().width + parseFloat(getComputedStyle(other).marginLeft || "0");
      }
      const shown = middleCut(name, Math.floor(room), measure, NAME_TAIL);
      fitted = shown === name ? null : shown;
    }
    // Set only when it changes: a state set to what it already is can
    // still draw the name once more.
    if (fitted !== cut) setCut(fitted);
  });
  if (cut === null) {
    return (
      <p ref={ref} title={name} lang={lang} data-cut-middle="" className={NAME_CLASS}>
        {name}
      </p>
    );
  }
  return (
    <p ref={ref} title={name} lang={lang} data-cut-middle="" className={NAME_CLASS}>
      <span aria-hidden="true">{cut}</span>
      <span className="sr-only">{name}</span>
    </p>
  );
}

/**
 * The version an update brings, alone, after its arrow ("→ 7.2", for a
 * version column narrower than "7.1 → 7.2"), the whole change for a screen
 * reader. A component of its own, the only words on a row the window's
 * language gives: only a row that says it asks for them (`useTranslation`,
 * which each asker pays for as it is drawn).
 */
function ToNewVersion({ version, newVersion }: { version: ReactNode; newVersion: string }) {
  const { t } = useTranslation();
  return (
    <>
      <span aria-hidden="true">{t("updates.versionTo", { target: newVersion })}</span>
      <span className="sr-only">{version}</span>
    </>
  );
}

/**
 * A row's accessible name, where it takes the focus (`ToolRow`): the
 * tool's name, then its status word and an update's change of version
 * where it has them, a comma between, which a screen reader pauses at in
 * either language. A version said as anything but words (none is, yet)
 * is left out; a column that says something else gives its words
 * (`versionText`), last.
 */
function rowName(
  name: string,
  statusText: string | undefined,
  change: ReactNode,
  versionText: string | undefined,
): string {
  const parts = [name];
  if (statusText !== undefined && statusText !== "") parts.push(statusText);
  if (typeof change === "string" && change !== "") parts.push(change);
  if (versionText !== undefined && versionText !== "") parts.push(versionText);
  return parts.join(", ");
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
 * in one 24 wide. The status word's column is on every row of a list any
 * of whose rows has a word, so the words line up down it, and on none of
 * a list with no word at all, whose names and descriptions take its room
 * (`StatusColumnContext`); the others are drawn whenever their prop is
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
 * does -- and Enter starts nothing: it does what `onEnter` says, where the
 * page says (the Installed page's details), and else nothing.
 * Such a row is a group named for a screen reader as it is scanned:
 * the tool's name, its status word and the change of version an update
 * brings, where it has them -- "git, 2.55.0 → 2.55.1", "gh, Skipped
 * 2.102.0" (`rowName`) -- its controls still reached inside it.
 *
 * `selected` fills it as a Mac list's selection (index.css), with no
 * hairline under it or over it, and says so (`aria-current`) on the row
 * that takes the focus.
 */
export function ToolRow({
  adapterId,
  sourceLabel,
  iconKey,
  avatar,
  name,
  namePath,
  showSource = false,
  description,
  selectableDescription = false,
  descriptionNote,
  selectable,
  status,
  statusText,
  version,
  versionText,
  newVersion,
  action,
  menu,
  onOpen,
  openLabel,
  onEnter,
  selected = false,
}: ToolRowProps) {
  // What fits, from the list's width, measured once for the whole list
  // (`VirtualList`): no row observes or measures its own box to choose it,
  // and none is drawn again for a new width that fits the same.
  const fit = useRowFit();
  const listHasStatus = useStatusColumn();
  const roving = useRovingRow();
  // The note is the row's description when the row takes the focus: why a
  // search found it (「命令：rg」), where an Unknown program points -- said
  // after the row's name, as the line shows it under the name.
  const noteId = useId();
  // The ⋯ menu's way to open at the pointer, which it leaves here (`Menu`).
  const openMenuAt = useRef<OpenMenuAt | null>(null);
  const openButton = useRef<HTMLButtonElement>(null);
  // The description's line, and whether the description has room on it
  // after what goes before it (below).
  const descriptionLine = useRef<HTMLDivElement>(null);
  const [descriptionFits, setDescriptionFits] = useState(true);
  // The name and description's block, and the version column: whether an
  // update's whole change has room in the column where the fit says only
  // its new version (below).
  const textBlock = useRef<HTMLDivElement>(null);
  const versionCell = useRef<HTMLDivElement>(null);
  const [changeFits, setChangeFits] = useState(false);

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
      // page's, one that cannot be ticked among rows that can) -- what
      // pressing it does, as Space shows a Finder selection in Quick Look.
      if (selectable !== undefined && selectable !== null) selectable.onToggle();
      else onOpen?.();
    } else if (event.key === "Enter") {
      event.preventDefault();
      onEnter?.();
    }
  };

  // The source's name, where the avatar's mark alone says it: after the
  // tool's name, for a screen reader (the avatar is decorative) or in
  // sight (`showSource`), and in the avatar's tooltip. Not for a tool with
  // its own installer, its own source: its name would only come twice.
  const source = sourceLabel !== undefined && sourceLabel !== name ? sourceLabel : undefined;
  const hasStatus = status !== undefined && status !== null;
  // The status word's column, or -- narrower, however narrow -- its place
  // at the start of the description's line, which gives way to it: the
  // word is what a row is scanned for, and never dropped for want of
  // room. No column in a list with no word on any row.
  const wide = fit === "full" || fit === "compact";
  const statusColumn = wide && (listHasStatus || hasStatus);
  const statusInline = !wide && hasStatus;
  const hasUpdate = newVersion !== undefined && version !== undefined && version !== null;
  // Where the version column has gone (`minimal`): an update's versions on
  // the description's line, and nothing for a row with no update; past
  // that, nothing either way.
  const versionColumn = version !== undefined && (wide || fit === "narrow");
  const versionInline = fit === "minimal" && hasUpdate;
  const actionColumn = action !== undefined && fit !== "tiny";
  // Where the fit says only the new version (`compact`, `narrow`), the
  // whole change all the same when the row has room for it with its name
  // whole: measured after every layout, as only the text knows how wide
  // it is. The room is the name's block and the version column together,
  // less the column at its widest -- the same whichever the column shows,
  // so the choice does not flip as it is made. Nothing measured (a line
  // not laid out, or jsdom): the new version alone, as the fit says.
  const changeMeasured = (fit === "compact" || fit === "narrow") && hasUpdate && typeof version === "string";
  useLayoutEffect(() => {
    if (!changeMeasured) return;
    const block = textBlock.current;
    const cell = versionCell.current;
    const nameLine = block?.firstElementChild;
    const nameText = nameLine?.firstElementChild;
    const blockWidth = block?.getBoundingClientRect().width ?? 0;
    const cellWidth = cell?.getBoundingClientRect().width ?? 0;
    const measureCell = cell == null || blockWidth === 0 ? null : textMeasurer(cell);
    const measureName = nameText instanceof HTMLElement ? textMeasurer(nameText) : null;
    let fits = false;
    if (measureCell !== null && measureName !== null && cell != null && nameLine != null) {
      const column = Math.max(Math.ceil(measureCell(version)), parseFloat(getComputedStyle(cell).minWidth) || 0);
      let nameNeeds = Math.ceil(measureName(namePath?.name ?? name)) + 1;
      // What else stands on the name's line in sight: the source's words
      // (R3), whole -- measured by their text, as their box is whatever
      // the name has left them (`NOTE_CLASS`), and the choice would turn on
      // itself.
      for (const other of Array.from(nameLine.children)) {
        if (other === nameText || getComputedStyle(other).position === "absolute") continue;
        const measureNote = other.hasAttribute("data-row-note") && other instanceof HTMLElement ? textMeasurer(other) : null;
        const width =
          measureNote !== null
            ? Math.ceil(measureNote(other.getAttribute("title") ?? other.textContent ?? ""))
            : other.getBoundingClientRect().width;
        nameNeeds += width + parseFloat(getComputedStyle(other).marginLeft || "0");
      }
      fits = blockWidth + cellWidth - column >= nameNeeds;
    }
    // Set only when it changes: a state set to what it already is can
    // still draw the row once more.
    if (fits !== changeFits) setChangeFits(fits);
  });
  const shownVersion =
    fit !== "full" && newVersion !== undefined && !(changeMeasured && changeFits) ? (
      <ToNewVersion version={version} newVersion={newVersion} />
    ) : (
      version
    );
  // What the description's line says before the description, in order,
  // set apart from it and from each other by a dot: the status word, then
  // an update's versions (beside the inspector).
  const hasLeading = statusInline || versionInline;
  // Whether the description keeps its place on the line (measured below).
  const descriptionShown = !hasLeading || descriptionFits;
  const leading: Array<{ key: string; node: ReactNode }> = [];
  if (statusInline) {
    leading.push({
      key: "status",
      // Whole while the description is there to give way to it. Once the
      // description has left the line, a word still wider than the line
      // -- "Updates when run", "Not Found in Terminal" beside the inspector
      // at 800 -- is cut short, its ⓘ kept, rather than run on under the
      // button after it (walk-4 W4-2): the word's tooltip and the ⓘ's
      // name say it whole. Alone on the line, it may take all but 1 of
      // the 16 between the line and the button's column, as the words
      // that fitted before ("Installed twice ⓘ") did, so only the words
      // that ran under the button are cut.
      node: (
        <span
          data-status=""
          className={`relative z-10 flex items-center ${
            descriptionShown
              ? "shrink-0"
              : `min-w-0 [&>*]:min-w-0 ${versionInline || descriptionNote !== undefined ? "" : "-mr-[15px]"}`
          }`}
        >
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
          {version}
        </span>
      ),
    });
  }
  // What goes before the description leaves it the rest of the line; with
  // less than its first few characters' room (`DESCRIPTION_MIN_CHARACTERS`)
  // it leaves the line, and its dot with it -- still said to a screen
  // reader. Measured after every layout, as nothing but the text measures
  // what the words before it take: the room is the line's width less
  // everything on it but the description and its dot, the same whether it
  // is in sight or not. Nothing measured (a line not laid out, or jsdom):
  // it stays, for its box to cut. A line with nothing before its
  // description -- every row but in a narrow list -- is not measured at
  // all: nothing on it can crowd the description out.
  // A model's path, before its description (`namePath`): what it says in
  // sight; a screen reader has heard it in the name.
  const lineText = namePath ? `${namePath.from} · ${description}` : description;
  useLayoutEffect(() => {
    if (!hasLeading) return;
    const line = descriptionLine.current;
    const width = line?.getBoundingClientRect().width ?? 0;
    const measure = line == null || width === 0 ? null : textMeasurer(line);
    let fits = true;
    if (measure !== null && line != null) {
      let room = width;
      for (const part of Array.from(line.children)) {
        if (part.hasAttribute("data-description") || part.hasAttribute("data-description-dot")) continue;
        room -= part.getBoundingClientRect().width;
      }
      fits = room >= measure(` · ${lineText.slice(0, DESCRIPTION_MIN_CHARACTERS)}`);
    }
    // Set only when it changes: a state set to what it already is can
    // still draw the row once more.
    if (fits !== descriptionFits) setDescriptionFits(fits);
  });
  const descriptionText = (
    <span
      title={lineText}
      data-description=""
      className={
        descriptionShown
          ? `${descriptionNote === undefined ? "min-w-0" : "max-w-[70%] shrink-0"} truncate ${versionInline ? "flex-1" : ""} ${selectableDescription ? "select-text" : ""}`
          : "sr-only"
      }
    >
      {namePath ? (
        <span aria-hidden="true" data-name-from="">
          {namePath.from}
          {" · "}
        </span>
      ) : null}
      {description}
    </span>
  );

  return (
    <RowMenuContext.Provider value={openMenuAt}>
      <div
        data-tool-row=""
        data-row-focus={roving === null ? undefined : ""}
        // Only a row that takes the focus is named: elsewhere its name and
        // controls are read in turn, as any other text.
        role={roving === null ? undefined : "group"}
        aria-label={roving === null ? undefined : rowName(name, statusText, hasUpdate ? version : undefined, versionText)}
        aria-describedby={roving === null || descriptionNote === undefined ? undefined : noteId}
        aria-current={selected ? "true" : undefined}
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
        <div ref={textBlock} className="ml-3 min-w-0 flex-1">
          <div className="flex min-w-0 items-baseline">
            <RowName name={name} shown={namePath?.name} />
            {source !== undefined ? (
              showSource ? (
                <RowNote note={source} />
              ) : (
                <span className="sr-only">{source}</span>
              )
            ) : null}
          </div>
          <div ref={descriptionLine} className="mt-0.5 flex min-w-0 items-center text-small text-muted">
            {leading.map((part, index) => (
              <Fragment key={part.key}>
                {index > 0 ? (
                  <span aria-hidden="true" data-line-dot="" className="shrink-0 whitespace-pre">
                    {" · "}
                  </span>
                ) : null}
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
              // The note gives way to the description, cut short first: the
              // path keeps its room, up to 70% of the line (walk-3 W3-8),
              // and the note's whole words are its tooltip, as the path's
              // are (review 3.2). Space sets the two apart, not a dot
              // between them.
              <span id={noteId} title={descriptionNote} className="min-w-0 truncate pl-3">
                {descriptionNote}
              </span>
            ) : null}
          </div>
        </div>
        {statusColumn ? (
          // 120 wide on every row of a list with a word, empty or not, the
          // word at its left: the words start in one line down the list
          // (wider only for a word that needs it).
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
            ref={versionCell}
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
