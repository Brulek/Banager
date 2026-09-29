import { useTranslation } from "react-i18next";
import type { UpdatesSummary } from "../lib/updateState";
import { CheckIcon, DashIcon } from "./icons";

/** What the ring shows: the headline's verdict, or that the first check is still running. */
export type RingState = UpdatesSummary | { kind: "checking" };

// The ring's circle in its 160-unit box: 10 units of stroke, and the
// length of the whole way round, which the spinning arc is a quarter of.
const RING_RADIUS = 72;
const RING_LENGTH = 2 * Math.PI * RING_RADIUS;

/**
 * The large round mark over the headline (CleanMyMac's one big thing to
 * look at): the accent colour round the number of updates, with the word
 * for them under it; a calm green round a check when everything is up to
 * date; a grey ring with a dash when there is nothing to update but that
 * is not the same as up to date; a faint ring with a quarter of it
 * turning while the first check runs, or while the updates it offered are
 * being installed -- only without "reduce motion".
 * Decorative: the headline under it says the same in words, so it is
 * hidden from screen readers. `data-ring` names its state.
 */
export function StatusRing({ state }: { state: RingState }) {
  const { t } = useTranslation();
  const ring = (className: string) => (
    <circle cx="80" cy="80" r={RING_RADIUS} fill="none" strokeWidth="10" className={className} />
  );
  return (
    <div
      data-ring={state.kind}
      aria-hidden="true"
      className="relative flex h-40 w-40 shrink-0 items-center justify-center"
    >
      <svg viewBox="0 0 160 160" className="absolute inset-0 h-full w-full">
        {state.kind === "updates" ? (
          <>
            <circle cx="80" cy="80" r={RING_RADIUS - 5} className="fill-accent/10" />
            {ring("stroke-accent")}
          </>
        ) : state.kind === "upToDate" ? (
          <>
            <circle cx="80" cy="80" r={RING_RADIUS - 5} className="fill-success/10" />
            {ring("stroke-success")}
          </>
        ) : (
          ring("stroke-hover")
        )}
      </svg>
      {state.kind === "checking" || state.kind === "updating" ? (
        <svg
          viewBox="0 0 160 160"
          className="absolute inset-0 h-full w-full motion-safe:animate-spin motion-safe:[animation-duration:1.6s]"
        >
          <circle
            cx="80"
            cy="80"
            r={RING_RADIUS}
            fill="none"
            strokeWidth="10"
            strokeLinecap="round"
            strokeDasharray={`${RING_LENGTH / 4} ${RING_LENGTH}`}
            className="stroke-accent/70"
          />
        </svg>
      ) : null}
      {state.kind === "updates" ? (
        <span className="relative flex flex-col items-center">
          <span className="text-[44px] font-semibold leading-none tabular-nums text-foreground">
            {state.actionable.length}
          </span>
          <span className="mt-2 text-small text-muted">
            {t("overview.updatesUnit", { count: state.actionable.length })}
          </span>
        </span>
      ) : state.kind === "upToDate" ? (
        <CheckIcon size={60} className="relative text-success" />
      ) : state.kind === "nothingToUpdate" ? (
        <DashIcon size={52} className="relative text-muted" />
      ) : null}
    </div>
  );
}

/**
 * What the Overview, Updates and Installed pages show until the first
 * check since Canager opened has answered: the ring turning, "Checking…"
 * under it, and a line saying why that takes a while. The first check
 * looks up every tool's newest version online, and the snapshot it
 * answers with comes only once every source has answered -- Homebrew's
 * list update alone can hold it for up to two minutes. A turning ring with
 * nothing else said looked like a window that had frozen, and the
 * Updates and Installed pages said less still: a small grey "Loading…"
 * in a corner. The Overview draws it itself (`SnapshotStatus`'s
 * `showsFirstCheck`), `SnapshotStatus` for the other two once the
 * startup snapshot is in, and the two pages themselves before it is.
 */
export function FirstCheck() {
  const { t } = useTranslation();
  return (
    <div className="flex min-h-full flex-col items-center px-5 pb-8">
      <section className="flex flex-1 flex-col items-center justify-center gap-5 py-8 text-center">
        <StatusRing state={{ kind: "checking" }} />
        <div className="flex flex-col items-center gap-1.5">
          <h2 className="text-headline text-muted">{t("common.checking")}</h2>
          <p className="max-w-md text-body text-muted">{t("common.firstCheckDetail")}</p>
        </div>
      </section>
    </div>
  );
}
