import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { useRefresh, useSnapshot } from "../lib/queries";
import { useRefreshInFlight } from "../lib/events";
import { elapsedSince, type Elapsed } from "../lib/format";
import { useUiStore } from "../store/ui";
import { RefreshIcon } from "./icons";

/** Whatever `useTranslation()`'s `t` needs here; the same convention as `Translate` in src/lib/sources.ts. */
type Translate = (key: string, options?: Record<string, string | number>) => string;

/** "Checked 3 min ago". A `switch` with no default, so a unit added to `Elapsed` without words here fails `tsc`. */
function checkedText(t: Translate, elapsed: Elapsed): string {
  switch (elapsed.unit) {
    case "justNow":
      return t("header.checkedJustNow");
    case "minutes":
      return t("header.checkedMinutesAgo", { count: elapsed.count });
    case "hours":
      return t("header.checkedHoursAgo", { count: elapsed.count });
    case "days":
      return t("header.checkedDaysAgo", { count: elapsed.count });
  }
}

/**
 * The time "Checked … ago" is measured to: the time now, moved on once a
 * minute. The minute starts over whenever `since` changes, so the words
 * turn from "just now" to "1 min ago" a minute after a check finishes,
 * not up to a minute late. Until the first tick after a check, the clock
 * is behind the check's own time, which `elapsedSince` reads as "just
 * now" -- as it is.
 */
function useMinuteClock(since: number | null): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 60_000);
    return () => clearInterval(id);
  }, [since]);
  return now;
}

export interface PageHeaderProps {
  title: string;
}

/**
 * The strip over every page: its title, when Canager last checked, and
 * Check again.
 *
 * Check again is the refresh every other trigger runs -- the one at
 * startup, the one after an operation, the Try again of a failed one --
 * through the same `useRefresh`, so a click while another refresh is
 * running would only be folded into it. The button is off meanwhile,
 * whoever started that refresh (`useRefreshInFlight`), and the time
 * gives way to "Checking…".
 *
 * `startupRefreshError` is not only the startup's: every refresh sets it
 * when it fails and clears it when it works (`refreshIntoCache` in
 * src/lib/events.ts), so it says whether the last check got anywhere. A
 * check that failed leaves the snapshot as it was, and the time would go
 * on naming the one before it as if nothing had been tried.
 */
export function PageHeader({ title }: PageHeaderProps) {
  const { t } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const refresh = useRefresh();
  const refreshing = useRefreshInFlight();
  const lastCheckFailed = useUiStore((s) => s.startupRefreshError !== null);
  const refreshedAt = snapshot?.refreshed_at ?? null;
  const now = useMinuteClock(refreshedAt);

  let status: { text: string; failed: boolean } | null = null;
  if (refreshing) {
    status = { text: t("common.checking"), failed: false };
  } else if (lastCheckFailed) {
    status = { text: t("header.checkFailed"), failed: true };
  } else if (refreshedAt !== null) {
    status = { text: checkedText(t, elapsedSince(refreshedAt, now)), failed: false };
  }

  return (
    <header className="flex shrink-0 items-center justify-between gap-4 px-6 pb-3 pt-5">
      <h1 className="min-w-0 truncate text-title text-foreground">{title}</h1>
      <div className="flex shrink-0 items-center gap-3">
        {status !== null ? (
          <p
            role={status.failed ? "alert" : undefined}
            className={`text-small ${status.failed ? "text-danger" : "text-muted"}`}
          >
            {status.text}
          </p>
        ) : null}
        <button
          type="button"
          onClick={() => refresh.mutate()}
          disabled={refreshing}
          className="inline-flex items-center gap-1.5 rounded-button border border-border bg-surface px-3 py-1.5 text-body font-medium text-foreground outline-none transition-colors hover:bg-hover focus-visible:ring-2 focus-visible:ring-accent disabled:opacity-50 disabled:hover:bg-surface"
        >
          <RefreshIcon size={16} />
          {t("header.checkAgain")}
        </button>
      </div>
    </header>
  );
}
