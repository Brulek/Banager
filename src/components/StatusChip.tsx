import type { ReactNode } from "react";
import { InfoIcon, WarningFilledIcon } from "./icons";
import { Popover } from "./ui/Popover";

export interface StatusChipProps {
  /** One or two words, six Chinese characters at most: 「已固定」, "View only". */
  label: string;
  /**
   * Why, in at most two short sentences, behind the word's ⓘ. A word with
   * no detail is plain text.
   */
  detail?: ReactNode;
  /**
   * The button's accessible name, where the word alone would be the same
   * on many rows: 「暂时不能卸载git」 for 「暂时不能卸载」, the word first.
   * Only a word with a detail is a button.
   */
  ariaLabel?: string;
  /** Which edge of the word the detail lines up with (`Popover`'s `align`). */
  align?: "start" | "end";
  /**
   * `neutral`: what a row is, and why it can't do something -- the muted
   * colour, as every other quiet word on a row. `danger`: something that
   * failed, in the red that reads as text (`danger-text`). `warning`:
   * something to look at, the word still muted, with an orange ⚠︎ before
   * it, as macOS marks one: the colour is the symbol's, never the text's.
   */
  tone?: "neutral" | "danger" | "warning";
}

/** The word's look: 11/14 in the regular weight, on one line, no fill and no outline (spec §3.4). */
const WORD_BASE = "inline-flex items-center gap-1 whitespace-nowrap text-small font-normal";
const WORD_TONES = {
  neutral: "text-muted",
  warning: "text-muted",
  danger: "text-danger-text",
} as const;

/**
 * A row's status in a word, in place of a sentence about it -- 「已固定」,
 * 「无法检查」 -- the way Cork and Latest mark a package: the word alone,
 * muted, and an ⓘ after it where there is a why, which shows the why
 * under it when pressed (the word and the ⓘ are one button, named by the
 * word). No pill: a Mac list says a row's state in words, not badges.
 * A row has one at most; a normal state -- up to date, an update to be
 * had -- has none (the version column says the second).
 */
export function StatusChip({ label, detail, ariaLabel, align = "end", tone = "neutral" }: StatusChipProps) {
  const WORD = `${WORD_BASE} ${WORD_TONES[tone]}`;
  const content = (
    <>
      {tone === "warning" ? <WarningFilledIcon size={12} className="shrink-0 text-warning" /> : null}
      {label}
    </>
  );
  if (detail === undefined) {
    return (
      <span data-status-word="" className={WORD}>
        {content}
      </span>
    );
  }
  return (
    <Popover
      trigger={
        <>
          {content}
          {/* Muted, as the word: the ⓘ is the way to the why, which is
              information, and the tertiary grey carries none (R4). */}
          <InfoIcon size={12} className="shrink-0" />
        </>
      }
      triggerLabel={ariaLabel}
      triggerClassName={`${WORD} rounded-sm hover:text-foreground aria-expanded:text-foreground`}
      align={align}
    >
      {detail}
    </Popover>
  );
}
