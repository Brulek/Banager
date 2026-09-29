import type { ReactNode } from "react";
import { SpinnerIcon } from "./icons";

/**
 * What the Overview's status row shows at its left (spec R1, after System
 * Settings > General > Software Update):
 *
 * - `updates`: updates to install -- an arrow down, white in an accent
 *   disc (SF Symbols' arrow.down.circle.fill, drawn to its proportions).
 * - `upToDate`: everything up to date -- a white check in a green disc.
 * - `quiet`: nothing to install, but not up to date either -- hidden, or
 *   not updatable here, or a source not checked -- the same check in the
 *   tertiary grey: a mark, not news; the words beside it say what it is.
 * - `failed`: the last check did not finish -- a white "!" in an orange
 *   triangle.
 * - `busy`: a check or an update running -- the 32 spinner, in the same
 *   48 box, so the words beside it do not move.
 */
export type StatusSymbolKind = "updates" | "upToDate" | "quiet" | "failed" | "busy";

/** A filled glyph in a 24-unit square: the shape in the current colour, its mark in white over it. */
function Filled({ size, className, children }: { size: number; className: string; children: ReactNode }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" aria-hidden="true" className={className}>
      {children}
    </svg>
  );
}

const MARK = {
  fill: "none",
  stroke: "#fff",
  strokeWidth: 2,
  strokeLinecap: "round",
  strokeLinejoin: "round",
} as const;

/** A white "!" in a filled triangle, in the current colour: SF Symbols' exclamationmark.triangle.fill. */
export function FilledWarningIcon({ size, className = "" }: { size: number; className?: string }) {
  return (
    <Filled size={size} className={`shrink-0 ${className}`}>
      <path
        d="M10.27 3.5a2 2 0 0 1 3.46 0l8.2 14.2a2 2 0 0 1-1.73 3H3.8a2 2 0 0 1-1.73-3z"
        fill="currentColor"
      />
      <path d="M12 8.75v5M12 17.25h.01" {...MARK} strokeWidth={2.25} />
    </Filled>
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
        <FilledWarningIcon size={48} className="text-warning" />
      ) : kind === "updates" ? (
        <Filled size={48} className="text-accent">
          <circle cx="12" cy="12" r="10" fill="currentColor" />
          <path d="M12 6.75v10M7.75 12.5L12 16.75L16.25 12.5" {...MARK} />
        </Filled>
      ) : (
        <Filled size={48} className={kind === "upToDate" ? "text-success" : "text-tertiary"}>
          <circle cx="12" cy="12" r="10" fill="currentColor" />
          <path d="M7.5 12.25L10.75 15.5L16.5 9" {...MARK} />
        </Filled>
      )}
    </span>
  );
}
