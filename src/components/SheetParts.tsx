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
import { Fragment, useId, useLayoutEffect, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { ArtifactKey } from "../lib/types";
import type { WarningLine } from "../lib/warnings";
import { InfoDetail } from "./InfoDetail";
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
export function SheetText({ children, className = "" }: { children: ReactNode; className?: string }) {
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
  /** On the right: the version it has, or the one it moves to. */
  aside?: ReactNode;
  /** Under the name: what to know about it, what became of it -- started, or why not. */
  children?: ReactNode;
}

/**
 * One tool of a dialog about several, a row of its grouped list
 * (`SheetToolList`): 36 high, its 24 avatar, its name in 13, and the
 * version on the right in 11 muted. What there is to know about it is
 * under its name, and belongs to it alone (spec R6). The source is on the
 * avatar's corner, and said to a screen reader after the name.
 */
export function SheetTool({
  adapterId,
  sourceLabel,
  showSource = false,
  iconKey,
  name,
  aside,
  children,
}: SheetToolProps) {
  const hasChildren = children !== undefined && children !== null && children !== false;
  return (
    <li data-sheet-tool="" className="px-2.5 py-1.5">
      <div className="flex min-h-6 items-center gap-2">
        <ToolAvatar size="sm" adapterId={adapterId} sourceLabel={sourceLabel} iconKey={iconKey} />
        <p className="flex min-w-0 flex-1 items-baseline gap-1.5">
          <span data-sheet-name="" title={name} className="min-w-0 truncate text-body text-foreground">
            {name}
          </span>
          {showSource ? (
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
 * focus.
 */
export function SheetToolList({ label, children }: { label?: string; children: ReactNode }) {
  return (
    <ul
      aria-label={label}
      data-sheet-tools=""
      tabIndex={0}
      className="max-h-80 overflow-y-auto rounded-group bg-group py-1 [&>*+*]:relative [&>*+*]:before:absolute [&>*+*]:before:left-10.5 [&>*+*]:before:right-2.5 [&>*+*]:before:top-0 [&>*+*]:before:h-px [&>*+*]:before:bg-group-separator"
    >
      {children}
    </ul>
  );
}

/**
 * What a dialog says in place of its notes while it is still finding
 * them out -- 「正在检查影响…」 over an uninstall, 「正在准备…」 over an
 * update -- with a spinner, in the quiet colour. One look for both, each
 * up at once with what it is about, and this line where what it has to
 * say will go. Held at the foot of the body should the body scroll.
 */
export function SheetPending({ text }: { text: string }) {
  return (
    <p className="sticky bottom-0 mt-3 flex items-center gap-2 bg-surface text-body text-muted">
      <SpinnerIcon size={16} className="shrink-0" />
      {text}
    </p>
  );
}

export interface SheetSectionProps {
  /** Its heading: 「移到废纸篓」, 「保留」, 「这些软件还要用它」. */
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
 * an ⓘ at the end where it has one. Nothing between a line's words and its
 * ⓘ but `<span>`s, so a line is found by its words from the item alone.
 */
export function SheetLines({ lines }: { lines: WarningLine[] }) {
  const { t } = useTranslation();
  if (lines.length === 0) return null;
  return (
    <ul className="flex flex-col gap-1">
      {lines.map((line, index) => (
        <SheetLine key={`${index}:${line.text}`} caution={line.caution}>
          {line.text}
          {line.detail !== null ? (
            <>
              {" "}
              <InfoDetail label={t("common.detailsLabel", { title: line.text })}>{line.detail}</InfoDetail>
            </>
          ) : null}
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
 * A refusal, said where it happened -- in a sheet, or on the page where
 * Update was pressed and nothing could be planned -- in the red that reads
 * as text (`danger-text`), as an alert: a `<div>`, since the ⓘ's panel is
 * one.
 */
export function Refusal({ text, detail, detailTitle, size = "body", className = "" }: RefusalProps) {
  const { t } = useTranslation();
  return (
    <div
      role="alert"
      className={`break-words text-danger-text ${size === "body" ? "text-body" : SMALL_WRAPPING} ${className}`}
    >
      {text}
      {detail !== null ? (
        <>
          {" "}
          <InfoDetail label={t("common.detailsLabel", { title: detailTitle })}>{detail}</InfoDetail>
        </>
      ) : null}
    </div>
  );
}
