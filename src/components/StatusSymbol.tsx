import type { ReactNode } from "react";
import { SpinnerIcon } from "./icons";

/**
 * What the Overview's status row shows at its left (spec R1, after System
 * Settings > General > Software Update), in a 48 slot whose left edge is
 * the group's inset:
 *
 * - `updates`: updates to install -- an arrow down, white in an accent
 *   disc as wide as the slot (SF Symbols' arrow.down.circle.fill, drawn to
 *   its proportions).
 * - `upToDate`: everything up to date -- a white check in a green disc.
 * - `quiet`: nothing to install, but not up to date either -- hidden, or
 *   not updatable here, or a source not checked -- a check in a circle,
 *   drawn in outline in the muted colour: a mark, not news, and not the
 *   look of something disabled; the words beside it say what it is.
 * - `failed`: the last check did not finish -- a white "!" in an orange
 *   triangle, 32, in the middle of the slot: said, not shouted.
 * - `busy`: the first check, or updates installing -- the 32 spinner in
 *   the middle of the slot, so the words beside it do not move.
 */
export type StatusSymbolKind = "updates" | "upToDate" | "quiet" | "failed" | "busy";

/**
 * A glyph in a 24-unit square, `box` the part of it drawn: the whole
 * square, or only the disc's 20 units, so that a disc fills its size.
 */
function Glyph({
  size,
  className,
  box = "0 0 24 24",
  children,
}: {
  size: number;
  className: string;
  box?: string;
  children: ReactNode;
}) {
  return (
    <svg width={size} height={size} viewBox={box} aria-hidden="true" className={className}>
      {children}
    </svg>
  );
}

/** A disc's 20 units: centred on 12, radius 10. */
const DISC_BOX = "2 2 20 20";

const MARK = {
  fill: "none",
  stroke: "#fff",
  strokeWidth: 1.75,
  strokeLinecap: "round",
  strokeLinejoin: "round",
} as const;

/** A white "!" in a filled triangle, in the current colour: SF Symbols' exclamationmark.triangle.fill. */
export function FilledWarningIcon({ size, className = "" }: { size: number; className?: string }) {
  return (
    <Glyph size={size} className={`shrink-0 ${className}`}>
      <path
        d="M10.27 3.5a2 2 0 0 1 3.46 0l8.2 14.2a2 2 0 0 1-1.73 3H3.8a2 2 0 0 1-1.73-3z"
        fill="currentColor"
      />
      <path d="M12 8.75v5M12 17.25h.01" {...MARK} strokeWidth={2.25} />
    </Glyph>
  );
}

/**
 * The 48 symbol for `kind`, decorative: the title beside it says the same
 * in words. `data-symbol` names its kind.
 */
export function StatusSymbol({ kind }: { kind: StatusSymbolKind }) {
  return (
    <span data-symbol={kind} aria-hidden="true" className="inline-flex size-12 shrink-0 items-center justify-center">
      {kind === "busy" ? (
        <SpinnerIcon size={32} className="text-muted" />
      ) : kind === "failed" ? (
        <FilledWarningIcon size={32} className="text-warning" />
      ) : kind === "updates" ? (
        <Glyph size={48} box={DISC_BOX} className="text-accent">
          <circle cx="12" cy="12" r="10" fill="currentColor" />
          <path d="M12 6.75v10M7.75 12.5L12 16.75L16.25 12.5" {...MARK} />
        </Glyph>
      ) : kind === "upToDate" ? (
        <Glyph size={48} box={DISC_BOX} className="text-success">
          <circle cx="12" cy="12" r="10" fill="currentColor" />
          <path d="M7.5 12.25L10.75 15.5L16.5 9" {...MARK} />
        </Glyph>
      ) : (
        <Glyph size={48} box={DISC_BOX} className="text-muted">
          <g fill="none" stroke="currentColor" strokeWidth={1.25} strokeLinecap="round" strokeLinejoin="round">
            <circle cx="12" cy="12" r="9.375" />
            <path d="M7.75 12.25L10.75 15.25L16.25 9.25" />
          </g>
        </Glyph>
      )}
    </span>
  );
}
