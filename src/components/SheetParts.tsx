/**
 * What the confirmation dialogs are made of (src/components/ui/Dialog.tsx),
 * in the manner of macOS's alerts (spec §3.6, R6): the icon of the one
 * tool a dialog is about, and its source and version under the question;
 * the question's text; the tools of a dialog about several, in a grouped
 * list with what to know about each under its own name; the line it shows
 * while it is still finding out what to say; its notes in named groups;
 * and a refusal with its why. One set, so the update and the uninstall
 * confirmation read alike.
 */
import { Fragment, startTransition, useEffect, useId, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { ArtifactKey } from "../lib/types";
import type { WarningLine } from "../lib/warnings";
import { InfoDetail, TextWithInfo } from "./InfoDetail";
import { SpinnerIcon, WarningFilledIcon } from "./icons";
import { ToolAvatar } from "./ToolAvatar";
import { SMALL_WRAPPING } from "./ui/group";

/**
 * The 48 icon over the question of a dialog about one tool, where NSAlert
 * puts an app's: the tool's own, as its row has it (`ToolAvatar`), with
 * its source's mark on its corner.
 */
export function SheetIcon({
  adapterId,
  sourceLabel,
  iconKey,
}: {
  adapterId: string;
  sourceLabel: string;
  iconKey?: ArtifactKey;
}) {
  return <ToolAvatar size="lg" adapterId={adapterId} sourceLabel={sourceLabel} iconKey={iconKey} />;
}

/**
 * The line under the question of a dialog about one tool: its source --
 * unless the tool is its own source, such as Claude Code -- and the
 * version it has or moves to, 「Homebrew · 1.8.1」, each a span of its own;
 * or null with neither to say.
 */
export function sheetMeta(name: string, sourceLabel: string, version: string | null): ReactNode {
  const parts = [sourceLabel !== name ? sourceLabel : null, version].filter(
    (part): part is string => part !== null && part !== "",
  );
  if (parts.length === 0) return null;
  return parts.map((part, index) => (
    <Fragment key={part}>
      {index > 0 ? " · " : null}
      <span>{part}</span>
    </Fragment>
  ));
}

/**
 * What a dialog about one tool says under its question, as an alert's
 * text: 13 in the label colour, its lines 16 apart, or 18 once it runs to
 * three lines or more, where Chinese set at 16 is cramped
 * (`--text-body-long`). Measured once drawn, before it is painted, and
 * again whenever what it says changes.
 */
export function SheetText({ children, className = "", id }: { children: ReactNode; className?: string; id?: string }) {
  const ref = useRef<HTMLParagraphElement>(null);
  const [long, setLong] = useState(false);
  useLayoutEffect(() => {
    const element = ref.current;
    if (element === null) return;
    const lineHeight = parseFloat(getComputedStyle(element).lineHeight);
    if (!(lineHeight > 0)) return;
    const lines = Math.round(element.offsetHeight / lineHeight);
    // Once long, its lines are taller, and it is still as many of them.
    if (lines > 0) setLong(lines >= 3);
  });
  return (
    <p
      ref={ref}
      id={id}
      data-sheet-text=""
      className={`break-words text-foreground ${long ? "text-body-long" : "text-body"} ${className}`}
    >
      {children}
    </p>
  );
}

export interface SheetToolProps {
  /** The source's adapter id and name, for the avatar -- the one a row has (`ToolRow`). */
  adapterId: string;
  sourceLabel: string;
  /**
   * The source's name after the tool's, 11 muted: only where the list has
   * the same name from two sources (spec R3), as Mail names an account.
   */
  showSource?: boolean;
  /** The tool's key, for its own icon -- a cask's app's, or its logo -- as on its row. */
  iconKey?: ArtifactKey;
  name: string;
  /**
   * The name as the list shows it, where that is shorter than `name`: a
   * model's last path segment (`modelPath`), its tag and quantisation
   * with it, as its row names it. The whole name stays its tooltip and
   * what a screen reader hears.
   */
  shownName?: string;
  /** On the right: the version it has, or the one it moves to. */
  aside?: ReactNode;
  /** Under the name: what to know about it, what became of it -- started, or why not. */
  children?: ReactNode;
  /**
   * The hairline over it that parts it from the tool before, for a list
   * whose tools draw their own (`SheetToolList`'s `rowsSeparate`): every
   * tool but the first.
   */
  separated?: boolean;
}

/**
 * The hairline over a tool of the list, from where its name starts: what
 * `SheetToolList` draws over every tool but its first, drawn by the tool.
 */
const TOOL_SEPARATOR =
  "relative before:absolute before:left-10.5 before:right-2.5 before:top-0 before:h-px before:bg-group-separator";

/**
 * One tool of a dialog about several, a row of its grouped list
 * (`SheetToolList`): 36 high, its 24 avatar, its name in 13, and the
 * version on the right in 11 muted. What there is to know about it is
 * under its name, and belongs to it alone (spec R6). The source is on the
 * avatar's corner, and said to a screen reader after the name -- unless
 * the tool is its own source, such as rustup.
 */
export function SheetTool({
  adapterId,
  sourceLabel,
  showSource = false,
  iconKey,
  name,
  shownName,
  aside,
  children,
  separated = false,
}: SheetToolProps) {
  const hasChildren = children !== undefined && children !== null && children !== false;
  return (
    <li data-sheet-tool="" className={separated ? `px-2.5 py-1.5 ${TOOL_SEPARATOR}` : "px-2.5 py-1.5"}>
      <div className="flex min-h-6 items-center gap-2">
        <ToolAvatar size="sm" adapterId={adapterId} sourceLabel={sourceLabel} iconKey={iconKey} />
        <p className="flex min-w-0 flex-1 items-baseline gap-1.5">
          <span data-sheet-name="" title={name} className="min-w-0 truncate text-body text-foreground">
            {shownName === undefined ? (
              name
            ) : (
              <>
                <span aria-hidden="true">{shownName}</span>
                <span className="sr-only">{name}</span>
              </>
            )}
          </span>
          {sourceLabel === name ? null : showSource ? (
            <span className="shrink-0 text-small text-muted">{sourceLabel}</span>
          ) : (
            <span className="sr-only">{sourceLabel}</span>
          )}
        </p>
        {aside !== undefined && aside !== null ? (
          <span className="shrink-0 whitespace-nowrap text-small tabular-nums text-muted">{aside}</span>
        ) : null}
      </div>
      {hasChildren ? <div className="mb-0.5 mt-1 flex flex-col gap-1 pl-8">{children}</div> : null}
    </li>
  );
}

/**
 * The tools of a dialog about several: a grouped container (the group
 * fill, corners of 10), as high as 320 and scrolling inside past that,
 * its rows parted by hairlines from where their names start. Its own Tab
 * stop, so the keyboard can scroll it: nothing in it but an ⓘ takes the
 * focus. `contained={false}`: as high as its rows, scrolled with the rest
 * of the dialog's body -- for a list with notes under every tool (a batch
 * uninstall's, at most 20), where a box scrolling inside the body would
 * hide a tool's only caution below its edge.
 *
 * `rowsSeparate`: the tools draw the hairlines themselves (`SheetTool`'s
 * `separated`), the same lines in the same places, for a long list whose
 * tools come and go other than at its end. A tool put in or taken out
 * between others of a list that draws them over "every tool after
 * another" has the browser work out the look of every tool after it
 * again: about 10,000 elements for Update all over 754 tools, 35-40 ms
 * in Chrome with the CPU slowed 4x, at each of its turns
 * (`useToolsInTurn`); drawn by the tools, about 2-5 ms.
 *
 * `busy`: not all of its tools are drawn yet, or not as they are now
 * (`useToolsInTurn`), said to a screen reader as the list being busy, so
 * that one reading it then is not told it has fewer tools than the
 * dialog's question and its Update count say.
 */
export function SheetToolList({
  label,
  contained = true,
  rowsSeparate = false,
  busy = false,
  children,
}: {
  label?: string;
  contained?: boolean;
  rowsSeparate?: boolean;
  busy?: boolean;
  children: ReactNode;
}) {
  const separators = rowsSeparate
    ? ""
    : " [&>*+*]:relative [&>*+*]:before:absolute [&>*+*]:before:left-10.5 [&>*+*]:before:right-2.5 [&>*+*]:before:top-0 [&>*+*]:before:h-px [&>*+*]:before:bg-group-separator";
  return (
    <ul
      aria-label={label}
      aria-busy={busy ? true : undefined}
      data-sheet-tools=""
      tabIndex={contained ? 0 : undefined}
      className={`${contained ? "max-h-80 overflow-y-auto " : ""}rounded-group bg-group py-1${separators}`}
    >
      {children}
    </ul>
  );
}

/**
 * How many of a dialog's tools its list draws as it opens
 * (`useToolsInTurn`): as many as its 320 high box can show, 36 a tool, so
 * what is in sight is whole from the first.
 */
export const TOOLS_DRAWN_FIRST = 9;

/**
 * How many more tools a dialog's list draws at each turn after its first
 * few (`useToolsInTurn`): what a frame lays out with room to spare on a
 * slow Mac.
 */
export const TOOLS_DRAWN_PER_TURN = 60;

/** Which of a dialog's tools its list draws now (`useToolsInTurn`). */
export interface ToolsInTurn {
  /** How many of the list, from its first, are drawn as they are now. */
  drawn: number;
  /**
   * How many of the list as the batch's first stage drew it, from its
   * first, are still drawn as they were -- those of them not among the
   * `drawn` -- because this stage has not reached them yet. None in the
   * first stage, and none once every tool is drawn as it is now.
   */
  held: number;
}

interface Turn {
  /** The batch and stage the turns are for. */
  key: string | null;
  batch: number | null;
  /** How many are drawn as they are now, before `count` caps it. */
  drawn: number;
  held: number;
}

/**
 * Which of a dialog's `count` tools its list (`SheetToolList`) draws: the
 * first `TOOLS_DRAWN_FIRST` as the dialog opens, then
 * `TOOLS_DRAWN_PER_TURN` more at each turn -- a transition once the last
 * is drawn, which gives way to the pointer and the keyboard -- so that no
 * one task lays out the whole list. Update all over 754 tools used to lay
 * out all of them at once, as it opened and again as its plans came back.
 *
 * `batch` names what the dialog is about and `stage` what its tools look
 * like. A new batch starts over from the first few. A new stage of the
 * same batch -- its plans back: notes under its tools, those with
 * something to say first -- draws its first few as they are now at once,
 * and keeps the others as the first stage drew them (`held`) until a turn
 * reaches them. Once every turn has run, the list is the same whatever
 * the turns were.
 *
 * `urgent`: the turns are not transitions but ordinary updates, for a
 * dialog drawn again and again by what it is doing -- Update all
 * starting its updates one after another. A transition gives way to each
 * of those drawings and would wait for them all to end, the list staying
 * as far as it had got until the last update started.
 */
export function useToolsInTurn(count: number, batch: number | null, stage: string, urgent = false): ToolsInTurn {
  const key = batch === null ? null : `${batch} ${stage}`;
  const [turn, setTurn] = useState<Turn>({ key: null, batch: null, drawn: 0, held: 0 });
  // A new batch or stage, until its first turn is kept: worked out from the
  // last one's as it is drawn, so that no drawing shows it with the last
  // one's turns. What the batch's first stage drew is held: a stage whose
  // own `held` is none.
  const current: Turn =
    turn.key === key
      ? turn
      : {
          key,
          batch,
          drawn: TOOLS_DRAWN_FIRST,
          held: batch !== null && turn.batch === batch && turn.held === 0 ? turn.drawn : 0,
        };
  const drawn = Math.min(count, current.drawn);
  const { held } = current;
  useEffect(() => {
    if (key === null || drawn >= count) return;
    // The next turn as a value, from what this drawing showed: run twice,
    // it is the same turn; one for a batch or stage gone since is only
    // worked out from again, as above.
    const next = () => setTurn({ key, batch, drawn: drawn + TOOLS_DRAWN_PER_TURN, held });
    if (urgent) next();
    else startTransition(next);
  }, [key, batch, drawn, held, count, urgent]);
  return { drawn, held: drawn >= count ? 0 : Math.min(count, held) };
}

/**
 * What a dialog says in place of its notes while it is still finding
 * them out -- 「正在检查影响…」 over an uninstall, 「正在准备…」 over an
 * update -- with a spinner, in the quiet colour. One look for both, each
 * up at once with what it is about, and this line where what it has to
 * say will go. Held at the foot of the body should the body scroll.
 */
export function SheetPending({ text }: { text: string }) {
  // A status: a screen reader says it as the dialog opens on it.
  return (
    <p role="status" className="sticky bottom-0 mt-3 flex items-center gap-2 bg-surface text-body text-muted">
      <SpinnerIcon size={16} className="shrink-0" />
      {text}
    </p>
  );
}

export interface SheetSectionProps {
  /** Its heading: 「移到废纸篓」, 「卸载后会保留」, 「不会卸载」. */
  title: string;
  /**
   * The heading is its name only, for a screen reader, and not drawn: the
   * uninstall's notes, which stand under its text as lines of their own
   * (spec §3.6: no 「请注意」 block).
   */
  titleHidden?: boolean;
  children: ReactNode;
}

/**
 * One named group of a dialog's notes (the copy table's C4 premise): its
 * heading in 11 bold and muted, its lines under it, 12 below what is above.
 */
export function SheetSection({ title, titleHidden = false, children }: SheetSectionProps) {
  const headingId = useId();
  if (titleHidden) {
    return (
      <section aria-label={title} className="mt-3">
        {children}
      </section>
    );
  }
  return (
    <section aria-labelledby={headingId} className="mt-3">
      <h3 id={headingId} className="text-small font-bold text-muted">
        {title}
      </h3>
      <div className="mt-1">{children}</div>
    </section>
  );
}

/**
 * A group's lines, a sentence each in 11 -- 14 apart on one line, 16 when
 * it wraps -- in the label colour: an alert's small print is text to read,
 * not a caption (spec R6). No bullets; a caution (`isCaution`) has a 12
 * filled ⚠︎ before its words, in systemOrange; its longer why is behind
 * an ⓘ at the end where it has one, held on one line with the last word
 * (`TextWithInfo`). Nothing between a line's words and its ⓘ but
 * `<span>`s, so a line is found by its words from the item alone.
 */
export function SheetLines({ lines }: { lines: WarningLine[] }) {
  const { t } = useTranslation();
  if (lines.length === 0) return null;
  return (
    <ul className="flex flex-col gap-1">
      {lines.map((line, index) => (
        <SheetLine key={`${index}:${line.text}`} caution={line.caution}>
          {line.detail !== null ? (
            <TextWithInfo text={line.text} label={t("common.detailsLabel", { title: line.text })}>
              {line.detail}
            </TextWithInfo>
          ) : (
            line.text
          )}
        </SheetLine>
      ))}
    </ul>
  );
}

/** One line of `SheetLines`: 11 in the label colour, a 12 ⚠︎ before a caution's words. */
export function SheetLine({ caution, children }: { caution: boolean; children: ReactNode }) {
  return (
    <li data-caution={caution ? "" : undefined} className={`flex gap-1 text-foreground ${SMALL_WRAPPING}`}>
      {caution ? <WarningFilledIcon size={12} className="mt-0.5 shrink-0 text-warning" /> : null}
      <span className="min-w-0 break-words">{children}</span>
    </li>
  );
}

export interface RefusalProps {
  /** What went wrong and what to do, in a sentence or two. */
  text: ReactNode;
  /** Its longer why, behind an ⓘ (`planErrorDetail`), or null. */
  detail: string | null;
  /** The ⓘ's accessible name says which refusal it explains. */
  detailTitle: string;
  /** 13 unless said; 11 under a tool's name in a list of several. */
  size?: "body" | "small";
  className?: string;
}

/**
 * Why a tool in a dialog about several is left out or held back -- an
 * explanation, not an error: in the secondary colour at 11, as macOS says
 * what a disabled choice needs, never in the refusal's red (round 3: an
 * alert is not marked red). `caution` puts the ⚠︎ of `SheetLine` before
 * it, for what the person should look at.
 */
export function SheetReason({ text, caution = false }: { text: string; caution?: boolean }) {
  return (
    <p data-sheet-reason="" className={`flex gap-1 text-muted ${SMALL_WRAPPING}`}>
      {caution ? <WarningFilledIcon size={12} className="mt-0.5 shrink-0 text-warning" /> : null}
      <span className="min-w-0 break-words">{text}</span>
    </p>
  );
}

/**
 * A refusal, said where it happened -- in a sheet, or on the page where
 * Update was pressed and nothing could be planned -- in the red that reads
 * as text (`danger-text`), as an alert: a `<div>`, since the ⓘ's panel is
 * one. Its ⓘ is held on one line with a sentence's last word
 * (`TextWithInfo`).
 */
export function Refusal({ text, detail, detailTitle, size = "body", className = "" }: RefusalProps) {
  const { t } = useTranslation();
  return (
    <div
      role="alert"
      className={`break-words text-danger-text ${size === "body" ? "text-body" : SMALL_WRAPPING} ${className}`}
    >
      {detail === null ? (
        text
      ) : typeof text === "string" ? (
        <TextWithInfo text={text} label={t("common.detailsLabel", { title: detailTitle })}>
          {detail}
        </TextWithInfo>
      ) : (
        <>
          {text}{" "}
          <InfoDetail label={t("common.detailsLabel", { title: detailTitle })}>{detail}</InfoDetail>
        </>
      )}
    </div>
  );
}
