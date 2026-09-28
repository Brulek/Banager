import { useEffect, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useRefresh, useSnapshot } from "../lib/queries";
import { useRefreshInFlight } from "../lib/events";
import { elapsedSince, type Elapsed } from "../lib/format";
import { useUiStore } from "../store/ui";
import { RefreshIcon } from "./icons";

/** Whatever `useTranslation()`'s `t` needs here; the same convention as `Translate` in src/lib/sources.ts. */
type Translate = (key: string, options?: Record<string, string | number>) => string;

/**
 * The words for each unit of `Elapsed`, for a "… ago" beside a header's
 * button: "Checked 3 min ago", "Scanned 3 min ago". A `Record` over the
 * units, so a unit added to `Elapsed` without words here fails `tsc`.
 */
export type ElapsedKeys = Record<Elapsed["unit"], string>;

/** "Checked 3 min ago", 「上次检查：3 分钟前」: when the sources were last checked. */
const CHECKED_KEYS: ElapsedKeys = {
  justNow: "header.checkedJustNow",
  minutes: "header.checkedMinutesAgo",
  hours: "header.checkedHoursAgo",
  days: "header.checkedDaysAgo",
};

/** `elapsed` in `keys`' words, with its count where it has one. */
export function elapsedText(t: Translate, keys: ElapsedKeys, elapsed: Elapsed): string {
  return elapsed.unit === "justNow" ? t(keys.justNow) : t(keys[elapsed.unit], { count: elapsed.count });
}

/**
 * The time "Checked … ago" is measured to: the time now, moved on once a
 * minute. The minute starts over whenever `since` changes, so the words
 * turn from "just now" to "1 min ago" a minute after a check finishes,
 * not up to a minute late. Until the first tick after a check, the clock
 * is behind the check's own time, which `elapsedSince` reads as "just
 * now" -- as it is.
 */
export function useMinuteClock(since: number | null): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 60_000);
    return () => clearInterval(id);
  }, [since]);
  return now;
}

/** Where a header's look again stands: its words, and whether they say it failed. */
export interface HeaderStatus {
  text: string;
  failed: boolean;
}

export interface HeaderActionProps {
  /**
   * What stands before the button: "Checked 3 min ago", "Checking…", or
   * -- in the danger colour, as an alert -- that the last one failed.
   * Null says nothing.
   */
  status: HeaderStatus | null;
  /** The button's words: "Check again", "Scan again". */
  label: string;
  onPress: () => void;
  /** It is running now, whoever started it: the button is off meanwhile. */
  busy: boolean;
}

/**
 * The one look of a header's way to look again, whatever it looks at --
 * the sources (`CheckAgain`), or the Unknown page's scan -- so that every
 * page's header reads the same: when, then the button.
 */
export function HeaderAction({ status, label, onPress, busy }: HeaderActionProps) {
  return (
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
        onClick={onPress}
        disabled={busy}
        className="inline-flex items-center gap-1.5 rounded-button border border-border bg-surface px-3 py-1.5 text-body font-medium text-foreground outline-none transition-colors hover:bg-hover focus-visible:ring-2 focus-visible:ring-accent disabled:opacity-50 disabled:hover:bg-surface"
      >
        <RefreshIcon size={16} />
        {label}
      </button>
    </div>
  );
}

/**
 * When Canager last checked its sources, and Check again: the header of
 * every page about them -- the Overview, Updates, Installed.
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
export function CheckAgain() {
  const { t } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const refresh = useRefresh();
  const refreshing = useRefreshInFlight();
  const lastCheckFailed = useUiStore((s) => s.startupRefreshError !== null);
  const refreshedAt = snapshot?.refreshed_at ?? null;
  const now = useMinuteClock(refreshedAt);

  let status: HeaderStatus | null = null;
  if (refreshing) {
    status = { text: t("common.checking"), failed: false };
  } else if (lastCheckFailed) {
    status = { text: t("header.checkFailed"), failed: true };
  } else if (refreshedAt !== null) {
    status = { text: elapsedText(t, CHECKED_KEYS, elapsedSince(refreshedAt, now)), failed: false };
  }

  return (
    <HeaderAction status={status} label={t("header.checkAgain")} onPress={() => refresh.mutate()} busy={refreshing} />
  );
}

export interface PageHeaderProps {
  title: string;
  /**
   * The page's own way to look again, on the right: left out, the
   * sources' (`CheckAgain`); the Unknown page's scan, for that page; null
   * for a page with nothing to look again at, such as Settings. One
   * control, never two stacked: a page with its own passes it here
   * rather than drawing it under the header.
   */
  actions?: ReactNode;
}

/**
 * The strip over every page: its title, and the page's own way to look
 * again. As tall with nothing on the right as with a button, so the
 * pages under it start at one height. The title takes the focus when what
 * should get it back is gone (`focusOrFallback`).
 *
 * It is the top of the window too. The title bar is an overlay
 * (src-tauri/tauri.conf.json), so the header's row is centred 26px down,
 * on the traffic lights' centre (the Sidebar's first row), and the
 * lights, the title and Check again read as one bar, as a Mac window's
 * toolbar does. Like a toolbar, it moves the window from anywhere but its
 * controls: with `deep`, a press anywhere inside it -- on the title, the
 * time, the space between -- starts a drag, except on a button, a link or
 * a field, which Tauri's drag script (`src/window/scripts/drag.js` in the
 * tauri crate) leaves to the page. A double-click zooms the window, and
 * selects nothing: on macOS the script lets a double-click's press through
 * to the page, where it would select the word under it.
 */
export function PageHeader({ title, actions }: PageHeaderProps) {
  return (
    <header
      data-tauri-drag-region="deep"
      className="flex shrink-0 select-none items-center justify-between gap-4 px-6 pb-3 pt-2.5"
    >
      <h1 tabIndex={-1} data-focus-fallback="" className="min-w-0 truncate text-title text-foreground outline-none">
        {title}
      </h1>
      <div className="flex min-h-8 shrink-0 items-center">{actions === undefined ? <CheckAgain /> : actions}</div>
    </header>
  );
}
