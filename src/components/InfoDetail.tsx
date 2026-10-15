import type { ReactNode } from "react";
import { InfoIcon } from "./icons";
import { Popover } from "./ui/Popover";

export interface InfoDetailProps {
  /** Its accessible name, which says what it explains: 「详情：…」/"Details: …". */
  label: string;
  /** The longer why: one or two short sentences. */
  children: ReactNode;
}

/**
 * An ⓘ at the end of a line, and the line's longer why in a popover
 * pointing at it (`Popover`: 13/18, 12 in): the redesign's rule that a
 * line says what matters and keeps the explanation one click away (原则
 * 2). Set after the line's last word, inside it, so it wraps with the
 * text. The ⓘ is 12, in the muted grey (spec R4: never the tertiary, it
 * carries something), darker only while its popover is open -- nothing
 * changes under the pointer. The popover's text is its own, whatever the
 * size and colour of the line it came from.
 */
export function InfoDetail({ label, children }: InfoDetailProps) {
  return (
    <Popover
      trigger={<InfoIcon size={12} />}
      triggerLabel={label}
      triggerClassName="-my-1 inline-flex h-5 w-5 items-center justify-center rounded-full align-middle text-muted aria-expanded:text-foreground"
    >
      {children}
    </Popover>
  );
}

/**
 * The end of `text` that an ⓘ after it keeps with it (`TextWithInfo`):
 * its last word -- a run of letters, digits and marks up to the end -- or,
 * in Chinese, which wraps between any two characters, its last character,
 * with the punctuation after either.
 */
const LAST_WORD = /(?:\p{Script=Han}\p{P}*|[^\s\p{Script=Han}]+)$/u;

/**
 * `text` and an ⓘ after it (`InfoDetail`) with its longer why, the ⓘ held
 * on one line with the text's last word (`LAST_WORD`), so it never wraps
 * onto a line of its own.
 */
export function TextWithInfo({ text, label, children }: { text: string; label: string; children: ReactNode }) {
  const match = LAST_WORD.exec(text);
  const head = match === null ? text : text.slice(0, match.index);
  const tail = match === null ? "" : match[0];
  return (
    <>
      {head}
      <span data-info-tail="" className="whitespace-nowrap">
        {tail}{" "}
        <InfoDetail label={label}>{children}</InfoDetail>
      </span>
    </>
  );
}
