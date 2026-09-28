import { useId } from "react";
import { useTranslation } from "react-i18next";
import { useSettings, useSnapshot, useUnknownScan } from "../lib/queries";
import { isStartupSnapshot } from "../lib/events";
import { ADAPTER_LABEL_KEYS, sourceNoticesFor } from "../lib/sources";
import type { SourceNoticeSpec } from "../lib/sources";
import { updatesSummary } from "../lib/updateState";
import type { UpdatesSummary } from "../lib/updateState";
import type { ManagerInstance } from "../lib/types";
import { useUiStore } from "../store/ui";
import { holdsRow, isUnderway, useUpdateOperationFor } from "../components/UpdateProgress";
import { SourceAvatar } from "../components/SourceAvatar";
import { CheckIcon, DashIcon, InfoIcon, UnknownIcon, WarningIcon } from "../components/icons";

/** Whatever `useTranslation()`'s `t` needs here; the same convention as `Translate` in src/lib/sources.ts. */
type Translate = (key: string, options?: Record<string, string | number>) => string;

/** One source with something installed: its avatar, its name and how many. */
interface SourceTile {
  instanceId: string;
  adapterId: string;
  label: string;
  count: number;
}

/** The headline. A `switch` with no default, so a new summary without words here fails `tsc`. */
function headlineText(t: Translate, summary: UpdatesSummary): string {
  switch (summary.kind) {
    case "updates":
      return t("overview.updatesAvailable", { count: summary.actionable.length });
    case "upToDate":
      return t("overview.upToDate");
    case "updating":
      return t("overview.updating", { count: summary.count });
    case "nothingToUpdate":
      return summary.everyChecked ? t("overview.nothingToUpdate") : t("overview.nothingToUpdateChecked");
  }
}

/**
 * The line under "Nothing to update": what there is instead, in the
 * Updates page's own numbers (`updatesSummary`) -- the updates the user
 * hid, the ones under its "Can't update here", the checks that did not
 * finish -- or null when there is none of that. A source not checked in
 * full says so under "Needs attention" instead.
 */
function nothingToUpdateLine(
  t: Translate,
  summary: Extract<UpdatesSummary, { kind: "nothingToUpdate" }>,
): string | null {
  const parts: string[] = [];
  if (summary.hidden > 0) parts.push(t("overview.hiddenCount", { count: summary.hidden }));
  if (summary.cantUpdateHere > 0) {
    parts.push(t("overview.cantUpdateHereCount", { count: summary.cantUpdateHere }));
  }
  if (summary.checksUnfinished > 0) {
    parts.push(t("overview.checksUnfinished", { count: summary.checksUnfinished }));
  }
  return parts.length === 0 ? null : parts.join(t("overview.listSeparator"));
}

/** What the ring shows: the headline's verdict, or that the first check is still running. */
type RingState = UpdatesSummary | { kind: "checking" };

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
function StatusRing({ state }: { state: RingState }) {
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

const TILE =
  "flex w-full items-center gap-3 rounded-row p-2.5 text-left outline-none transition-colors hover:bg-hover focus-visible:ring-2 focus-visible:ring-accent";

/**
 * The first page: the Mac at a glance, and one thing to do about it.
 *
 * In the middle, the ring and the headline under it: the Updates page's
 * own verdict (`updatesSummary`) -- how many updates it offers to
 * install, with one button that opens it with all of them selected;
 * "Everything is up to date" only when that page would say so; and a
 * plain "Nothing to update" when there is nothing to install but that is
 * not the same thing -- "No updates in the sources Canager could check"
 * where a source was not checked in full -- with a line under it saying what there is instead
 * (`nothingToUpdateLine`) and, when the Updates page lists any of it, a
 * quieter Review updates that opens it. Before the first check has
 * answered, "Checking…" -- the startup placeholder is not an answer
 * (`isStartupSnapshot`).
 *
 * Below it, quietly, two panels. "Your tools": a tile for each source
 * with something installed and how much, which opens the Installed page on
 * that source's tools, and one for the programs the Unknown page's last
 * scan could not place, once a scan has found some (nothing starts one
 * here). "Needs attention": one line for each source that needs it -- the
 * title of its first notice (`sourceNoticesFor`: not running, not
 * answering, a list it could not download, another copy that runs
 * instead). What a source lets Canager do at all, pip being read-only, is
 * not news here; both lists say it on each of its rows. Each line is a
 * title only; the explanation stays with the source on those pages.
 */
export function OverviewPage() {
  const { t } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const { data: settings } = useSettings();
  const { data: scan } = useUnknownScan();
  const setPage = useUiStore((s) => s.setPage);
  const openInstalled = useUiStore((s) => s.openInstalled);
  const selectUpdates = useUiStore((s) => s.selectUpdates);
  const operationFor = useUpdateOperationFor();
  const toolsHeadingId = useId();
  const attentionHeadingId = useId();

  if (!snapshot || !settings || isStartupSnapshot(snapshot)) {
    return (
      <div className="flex min-h-full flex-col items-center px-6 pb-8">
        <section className="flex flex-1 flex-col items-center justify-center gap-5 py-8 text-center">
          <StatusRing state={{ kind: "checking" }} />
          <h2 className="text-headline text-muted">{t("common.checking")}</h2>
        </section>
      </div>
    );
  }

  const labelOf = (instance: ManagerInstance): string => {
    const labelKey = ADAPTER_LABEL_KEYS[instance.adapter_id];
    return labelKey ? t(labelKey) : instance.adapter_id;
  };

  const installedByInstance = new Map<string, number>();
  for (const artifact of snapshot.artifacts) {
    const id = artifact.key.instance_id;
    installedByInstance.set(id, (installedByInstance.get(id) ?? 0) + 1);
  }

  const summary = updatesSummary(
    snapshot,
    settings,
    (candidate) => holdsRow(operationFor(candidate)),
    (candidate) => isUnderway(operationFor(candidate)),
  );
  const whyNothing = summary.kind === "nothingToUpdate" ? nothingToUpdateLine(t, summary) : null;

  const tiles: SourceTile[] = snapshot.instances.flatMap((instance) => {
    const count = installedByInstance.get(instance.id) ?? 0;
    return count === 0
      ? []
      : [{ instanceId: instance.id, adapterId: instance.adapter_id, label: labelOf(instance), count }];
  });

  const attention: SourceNoticeSpec[] = snapshot.instances.flatMap((instance) => {
    const [notice] = sourceNoticesFor(instance, labelOf(instance), installedByInstance.get(instance.id) ?? 0);
    return notice === undefined ? [] : [notice];
  });

  const unknownCount = scan?.entries.length ?? 0;

  return (
    <div className="flex min-h-full flex-col items-center px-6 pb-8">
      <section className="flex flex-1 flex-col items-center justify-center gap-5 pb-10 pt-6 text-center">
        <StatusRing state={summary} />
        <div className="flex flex-col items-center gap-1.5">
          <h2 className="text-headline text-foreground">{headlineText(t, summary)}</h2>
          {whyNothing !== null ? <p className="text-body text-muted">{whyNothing}</p> : null}
        </div>
        {summary.kind === "nothingToUpdate" && summary.cantUpdateHere > 0 ? (
          // The Updates page has rows to show, every one under "Can't
          // update here": nothing to select, only a page to open.
          <button
            type="button"
            onClick={() => setPage("updates")}
            className="h-9 rounded-button border border-border bg-surface px-6 text-body font-medium text-foreground outline-none transition-colors hover:bg-hover focus-visible:ring-2 focus-visible:ring-accent"
          >
            {t("overview.reviewUpdates")}
          </button>
        ) : null}
        {summary.kind === "updating" ? (
          // Where each one's progress is: in its own row.
          <button
            type="button"
            onClick={() => setPage("updates")}
            className="h-9 rounded-button border border-border bg-surface px-6 text-body font-medium text-foreground outline-none transition-colors hover:bg-hover focus-visible:ring-2 focus-visible:ring-accent"
          >
            {t("overview.seeProgress")}
          </button>
        ) : null}
        {summary.kind === "updates" ? (
          <button
            type="button"
            onClick={() => {
              // Every row the Updates page would tick with Select all, and
              // no other; any row selected earlier stays as it was.
              selectUpdates(summary.actionable.map((candidate) => candidate.key));
              setPage("updates");
            }}
            className="h-10 rounded-button bg-accent px-8 text-section font-semibold text-accent-foreground outline-none transition-colors hover:bg-accent-hover focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2 focus-visible:ring-offset-content"
          >
            {t("overview.reviewUpdates")}
          </button>
        ) : null}
      </section>

      <div className="flex w-full max-w-3xl flex-col gap-4">
        {tiles.length > 0 || unknownCount > 0 ? (
          <section
            aria-labelledby={toolsHeadingId}
            className="rounded-panel border border-border bg-surface p-3"
          >
            <h2 id={toolsHeadingId} className="px-2.5 pb-2 pt-1 text-section text-foreground">
              {t("overview.yourTools")}
            </h2>
            <ul
              aria-labelledby={toolsHeadingId}
              className="grid grid-cols-2 gap-1 sm:grid-cols-3 lg:grid-cols-4"
            >
              {tiles.map((tile) => (
                <li key={tile.instanceId}>
                  {/* Opens the Installed page on this source's tools: the
                      ones its count counted. */}
                  <button type="button" onClick={() => openInstalled(tile.instanceId)} className={TILE}>
                    <SourceAvatar adapterId={tile.adapterId} label={tile.label} size="md" />
                    <span className="min-w-0">
                      <span className="block truncate text-body font-semibold text-foreground">
                        {tile.label}
                      </span>{" "}
                      <span className="block text-small tabular-nums text-muted">
                        {t("overview.itemCount", { count: tile.count })}
                      </span>
                    </span>
                  </button>
                </li>
              ))}
              {unknownCount > 0 ? (
                <li>
                  <button type="button" onClick={() => setPage("unknown")} className={TILE}>
                    <span
                      aria-hidden="true"
                      className="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-[9px] bg-muted text-white"
                    >
                      <UnknownIcon size={18} />
                    </span>
                    <span className="min-w-0">
                      <span className="block truncate text-body font-semibold text-foreground">
                        {t("nav.unknown")}
                      </span>{" "}
                      <span className="block text-small tabular-nums text-muted">
                        {t("overview.itemCount", { count: unknownCount })}
                      </span>
                    </span>
                  </button>
                </li>
              ) : null}
            </ul>
          </section>
        ) : null}

        {attention.length > 0 ? (
          <section aria-labelledby={attentionHeadingId} className="rounded-panel bg-hover/60 p-3">
            <h2 id={attentionHeadingId} className="px-2.5 pb-1 pt-1 text-section text-foreground">
              {t("overview.attentionLabel")}
            </h2>
            <ul aria-labelledby={attentionHeadingId} className="flex flex-col">
              {attention.map((notice) => (
                <li
                  key={notice.id}
                  className="flex items-center gap-2.5 px-2.5 py-1.5 text-body text-foreground"
                >
                  {notice.variant === "warning" ? (
                    <WarningIcon size={16} className="shrink-0 text-warning" />
                  ) : (
                    <InfoIcon size={16} className="shrink-0 text-muted" />
                  )}
                  {t(notice.titleKey, notice.values)}
                </li>
              ))}
            </ul>
          </section>
        ) : null}
      </div>
    </div>
  );
}
