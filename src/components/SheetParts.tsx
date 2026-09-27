/**
 * What the two confirmation sheets are made of (src/components/ui/Dialog.tsx):
 * the tools a sheet is about, its notes in named groups, and a refusal
 * with its why. One set, so the update and the uninstall confirmation read
 * alike.
 */
import { useId, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { WarningLine } from "../lib/warnings";
import { InfoDetail } from "./InfoDetail";
import { SourceAvatar } from "./SourceAvatar";

export interface SheetToolProps {
  /** The source's adapter id and name, for the avatar -- the one a row has (`ToolRow`). */
  adapterId: string;
  sourceLabel: string;
  name: string;
  /** On the right: the version it has, or the one it moves to. */
  aside?: ReactNode;
  /** Under the name: what became of it -- started, or why not. */
  children?: ReactNode;
}

/**
 * One tool a sheet is about, as its row shows it: the source's avatar, the
 * name -- with the source's name under it, unless the tool is its own
 * source -- and a version on the right. A list item: the sheet lists one
 * for an uninstall, and one per tool for an update.
 */
export function SheetTool({ adapterId, sourceLabel, name, aside, children }: SheetToolProps) {
  return (
    <li data-sheet-tool="" className="flex items-start gap-3 py-1.5">
      <SourceAvatar adapterId={adapterId} label={sourceLabel} size="md" />
      <div className="min-w-0 flex-1">
        <div className="flex min-h-8 items-center gap-3">
          <div className="min-w-0 flex-1">
            <p title={name} className="truncate text-name font-semibold text-foreground">
              {name}
            </p>
            {sourceLabel !== name ? <p className="truncate text-small text-muted">{sourceLabel}</p> : null}
          </div>
          {aside !== undefined && aside !== null ? (
            <span className="shrink-0 whitespace-nowrap text-small tabular-nums text-muted">{aside}</span>
          ) : null}
        </div>
        {children}
      </div>
    </li>
  );
}

export interface SheetSectionProps {
  /** Its heading: 「移到废纸篓」, 「保留不动」, 「请注意」. */
  title: string;
  /** Before the heading, such as the warning sign on 「请注意」. */
  icon?: ReactNode;
  children: ReactNode;
}

/** One named group of a sheet's notes (the copy table's C4 premise), under a quiet heading. */
export function SheetSection({ title, icon, children }: SheetSectionProps) {
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} className="mt-5">
      <h3 id={headingId} className="flex items-center gap-1.5 text-small font-semibold text-muted">
        {icon}
        {title}
      </h3>
      <div className="mt-2">{children}</div>
    </section>
  );
}

/**
 * A group's lines, a sentence each, with its longer why behind an ⓘ at
 * the end where it has one. Nothing between a line's words and its ⓘ but
 * `<span>`s, so a line is found by its words from the item alone.
 */
export function SheetLines({ lines }: { lines: WarningLine[] }) {
  const { t } = useTranslation();
  if (lines.length === 0) return null;
  return (
    <ul className="flex flex-col gap-1.5">
      {lines.map((line, index) => (
        <li key={`${index}:${line.text}`} className="flex gap-2 text-body text-foreground">
          <span aria-hidden="true" className="mt-[7px] h-1 w-1 shrink-0 rounded-full bg-muted" />
          <span className="min-w-0 break-words">
            {line.text}
            {line.detail !== null ? (
              <>
                {" "}
                <InfoDetail label={t("common.detailsLabel", { title: line.text })}>{line.detail}</InfoDetail>
              </>
            ) : null}
          </span>
        </li>
      ))}
    </ul>
  );
}

export interface RefusalProps {
  /** What went wrong and what to do, in a sentence or two. */
  text: ReactNode;
  /** Its longer why, behind an ⓘ (`planErrorDetail`), or null. */
  detail: string | null;
  /** The ⓘ's accessible name says which refusal it explains. */
  detailTitle: string;
  className?: string;
}

/**
 * A refusal, said where it happened -- in a sheet, or on the page where
 * Update was pressed and nothing could be planned -- in the danger colour,
 * as an alert: a `<div>`, since the ⓘ's panel is one.
 */
export function Refusal({ text, detail, detailTitle, className = "" }: RefusalProps) {
  const { t } = useTranslation();
  return (
    <div role="alert" className={`break-words text-body text-danger ${className}`}>
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
