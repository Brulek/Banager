import { useTranslation } from "react-i18next";
import { useSettings, useSnapshot, useUnknownScan } from "../lib/queries";
import { isStartupSnapshot } from "../lib/events";
import { ADAPTER_LABEL_KEYS, sourceNoticesFor } from "../lib/sources";
import type { SourceNoticeSpec } from "../lib/sources";
import { updatesSummary } from "../lib/updateState";
import type { UpdatesSummary } from "../lib/updateState";
import type { ManagerInstance } from "../lib/types";
import { useUiStore } from "../store/ui";
import { SourceAvatar } from "../components/SourceAvatar";
import { CheckCircleIcon } from "../components/icons";

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
    case "nothingToUpdate":
      return t("overview.nothingToUpdate");
  }
}

/**
 * The first page: the Mac at a glance, and one thing to do about it.
 *
 * The headline is the Updates page's own verdict (`updatesSummary`): how
 * many updates it offers to install, with one button that opens it with
 * all of them selected; "Everything is up to date" only when that page
 * would say so; and a plain "Nothing to update" when there is nothing to
 * install but that is not the same thing. Before the first check has
 * answered, "Checking…" -- the startup placeholder is not an answer
 * (`isStartupSnapshot`).
 *
 * Below it, quietly: each source with something installed and how much,
 * what the last scan of the Unknown page found, and one line for each
 * source that needs attention -- the title of its first notice about what
 * Canager found this time (`axis: "state"`: not running, not answering,
 * a list it could not download, another copy that runs instead). What a
 * source lets Canager do at all, pip being read-only, is not news here;
 * the Installed and Updates pages say it under the source's name. Each
 * line is a title only; the explanation stays with the source on those
 * pages.
 */
export function OverviewPage() {
  const { t } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const { data: settings } = useSettings();
  const { data: scan } = useUnknownScan();
  const setPage = useUiStore((s) => s.setPage);
  const selectUpdates = useUiStore((s) => s.selectUpdates);

  if (!snapshot || !settings || isStartupSnapshot(snapshot)) {
    return (
      <div className="flex min-h-full flex-col px-6 pb-6">
        <section className="flex flex-1 flex-col items-start justify-center py-6">
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

  const summary = updatesSummary(snapshot, settings);

  const tiles: SourceTile[] = snapshot.instances.flatMap((instance) => {
    const count = installedByInstance.get(instance.id) ?? 0;
    return count === 0
      ? []
      : [{ instanceId: instance.id, adapterId: instance.adapter_id, label: labelOf(instance), count }];
  });

  const attention: SourceNoticeSpec[] = snapshot.instances.flatMap((instance) => {
    const notice = sourceNoticesFor(
      instance,
      labelOf(instance),
      installedByInstance.get(instance.id) ?? 0,
    ).find((spec) => spec.axis === "state");
    return notice === undefined ? [] : [notice];
  });

  const unknownCount = scan?.entries.length ?? 0;

  return (
    <div className="flex min-h-full flex-col px-6 pb-6">
      <section className="flex flex-1 flex-col items-start justify-center gap-5 py-6">
        {summary.kind === "upToDate" ? (
          <CheckCircleIcon size={44} className="text-success" />
        ) : null}
        <h2 className="text-headline text-foreground">{headlineText(t, summary)}</h2>
        {summary.kind === "updates" ? (
          <button
            type="button"
            onClick={() => {
              // Every row the Updates page would tick with Select all, and
              // no other; any row selected earlier stays as it was.
              selectUpdates(summary.actionable.map((candidate) => candidate.key));
              setPage("updates");
            }}
            className="rounded-button bg-accent px-5 py-2.5 text-body font-semibold text-accent-foreground outline-none transition-colors hover:bg-accent-hover focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2 focus-visible:ring-offset-content"
          >
            {t("overview.reviewUpdates")}
          </button>
        ) : null}
      </section>

      <div className="flex flex-col gap-4">
        {tiles.length > 0 ? (
          <ul aria-label={t("nav.installed")} className="flex flex-wrap gap-2">
            {tiles.map((tile) => (
              <li key={tile.instanceId}>
                {/* The Installed page has no filter by source yet, so a
                    tile opens it whole. */}
                <button
                  type="button"
                  onClick={() => setPage("installed")}
                  className="flex items-center gap-2 rounded-row border border-border bg-surface py-1.5 pl-1.5 pr-3 text-body text-foreground outline-none transition-colors hover:bg-hover focus-visible:ring-2 focus-visible:ring-accent"
                >
                  <SourceAvatar adapterId={tile.adapterId} label={tile.label} />
                  <span className="font-medium">{tile.label}</span>{" "}
                  <span className="tabular-nums text-muted">
                    {t("overview.itemCount", { count: tile.count })}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        ) : null}

        {unknownCount > 0 ? (
          <p className="flex items-center gap-2 text-body text-muted">
            <span>{t("overview.unknownCount", { count: unknownCount })}</span>
            <span aria-hidden="true">·</span>
            <button
              type="button"
              onClick={() => setPage("unknown")}
              aria-label={t("overview.viewUnknownLabel")}
              className="rounded-sm font-medium text-accent-text outline-none hover:underline focus-visible:ring-2 focus-visible:ring-accent"
            >
              {t("overview.viewUnknown")}
            </button>
          </p>
        ) : null}

        {attention.length > 0 ? (
          <ul aria-label={t("overview.attentionLabel")} className="flex flex-col gap-1.5">
            {attention.map((notice) => (
              <li key={notice.id} className="flex items-center gap-2 text-body text-foreground">
                <span
                  aria-hidden="true"
                  className={`h-2 w-2 shrink-0 rounded-full ${
                    notice.variant === "warning" ? "bg-warning" : "bg-muted"
                  }`}
                />
                {t(notice.titleKey, notice.values)}
              </li>
            ))}
          </ul>
        ) : null}
      </div>
    </div>
  );
}
