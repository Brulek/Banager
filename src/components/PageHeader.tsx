import { useEffect, useState, type ReactNode, type Ref } from "react";
import { useTranslation } from "react-i18next";
import { useCheckAgain, useSnapshot } from "../lib/queries";
import { elapsedSince, type Elapsed } from "../lib/format";
import { useUiStore } from "../store/ui";
import { RefreshIcon, SpinnerIcon } from "./icons";
import { ICON_BUTTON, ICON_BUTTON_BUSY } from "./ui/controls";

/** Whatever `useTranslation()`'s `t` needs here; the same convention as `Translate` in src/lib/sources.ts. */
type Translate = (key: string, options?: Record<string, string | number>) => string;

/**
 * The words for each unit of `Elapsed`, for a "… ago" in a toolbar
 * button's tooltip: "Checked 3 min ago", "Scanned 3 min ago". A `Record`
 * over the units, so a unit added to `Elapsed` without words here fails
 * `tsc`.
 */
export type ElapsedKeys = Record<Elapsed["unit"], string>;

/**
 * "Checked 3 min ago", 「上次检查：3分钟前」: when the sources were last
 * checked -- in Check again's tooltip, and under the Overview's status.
 */
export const CHECKED_KEYS: ElapsedKeys = {
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

export interface HeaderActionProps {
  /** Its name, as a screen reader says it: "Check again", "Scan again". */
  label: string;
  /**
   * What the pointer shows over it: its name with its shortcut, and when
   * it last looked -- 「重新检查（⌘R）· 上次检查：3分钟前」.
   */
  tooltip: string;
  onPress: () => void;
  /**
   * It is running now, whoever started it: the button is off meanwhile, a
   * spinner in its place -- off as `aria-disabled` says it, a press
   * ignored, and the focus kept on it (`ICON_BUTTON_BUSY`).
   */
  busy: boolean;
}

/**
 * The one look of a toolbar's way to look again, whatever it looks at --
 * the sources (`CheckAgain`), or the Unknown page's scan -- so that every
 * page's toolbar reads the same: a ⟳ with no words (`ICON_BUTTON`), its
 * name and when it last looked in its tooltip, as a Mac toolbar's item
 * keeps them. While it runs, a spinner in the same box.
 */
export function HeaderAction({ label, tooltip, onPress, busy }: HeaderActionProps) {
  return (
    <button
      type="button"
      aria-label={label}
      title={tooltip}
      aria-disabled={busy ? true : undefined}
      onClick={busy ? undefined : onPress}
      className={busy ? ICON_BUTTON_BUSY : ICON_BUTTON}
    >
      {/* The muted grey, not the tertiary of a button that is off: it is
          working, not unavailable. */}
      {busy ? <SpinnerIcon size={16} className="text-muted" /> : <RefreshIcon size={16} />}
    </button>
  );
}

/**
 * Check again (`useCheckAgain`), on the toolbar of every page about the
 * sources -- the Overview, Updates, Installed. While a check runs,
 * whoever started it, it is off and turning. Its tooltip says when the
 * sources were last checked, or that the last check failed:
 *
 * `startupRefreshError` is not only the startup's: every refresh sets it
 * when it fails and clears it when it works (`refreshIntoCache` in
 * src/lib/events.ts), so it says whether the last check got anywhere. A
 * check that failed leaves the snapshot as it was, and the time would go
 * on naming the one before it as if nothing had been tried. The pages
 * with a subtitle say so there as well (`usePageSubtitle` in src/App.tsx),
 * and the Overview in its status row (`OverviewPage`).
 */
export function CheckAgain() {
  const { t } = useTranslation();
  const { data: snapshot } = useSnapshot();
  const { checkAgain, checking } = useCheckAgain();
  const lastCheckFailed = useUiStore((s) => s.startupRefreshError !== null);
  const refreshedAt = snapshot?.refreshed_at ?? null;
  const now = useMinuteClock(refreshedAt);

  const label = t("header.checkAgain");
  let status: string | null = null;
  if (checking) {
    status = t("common.checking");
  } else if (lastCheckFailed) {
    status = t("header.checkFailed");
  } else if (refreshedAt !== null) {
    status = elapsedText(t, CHECKED_KEYS, elapsedSince(refreshedAt, now));
  }
  const tooltip =
    status === null ? t("toolbar.checkAgainShortcut", { label }) : t("toolbar.checkAgainTip", { label, status });

  return <HeaderAction label={label} tooltip={tooltip} onPress={checkAgain} busy={checking} />;
}

/** A page's subtitle: how many it lists, or where its check stands -- in the danger colour, as an alert, when that failed. */
export interface PageSubtitle {
  text: string;
  /**
   * What follows the count after a 「·」 -- what the tools take, 「约10.6
   * GB」 -- shown whole on the line, or, where the toolbar is too narrow
   * for it, not at all: never cut in the middle of a number or a unit
   * (walk-3 W3-2). A screen reader hears it either way.
   */
  rest?: string;
  failed: boolean;
  /** What the words leave out, as the subtitle's tooltip: what a size total counts. */
  note?: string;
}

export interface PageHeaderProps {
  title: string;
  /**
   * The line under the title, or nothing (the Overview, Settings):
   * 「10个可更新」, 「51个工具」, 「正在检查…」 (`usePageSubtitle` in
   * src/App.tsx).
   */
  subtitle?: PageSubtitle | null;
  /**
   * The page's own way to look again, at the toolbar's right end: left
   * out, the sources' (`CheckAgain`); the Unknown page's scan, for that
   * page; null for a page with nothing to look again at, such as
   * Settings. One control, never two side by side.
   */
  actions?: ReactNode;
  /**
   * Handed the box before it, where the page below puts its own actions
   * (`ToolbarItems` in ./Toolbar.tsx). Empty, it takes no room.
   */
  slotRef?: Ref<HTMLDivElement>;
  /** The page has scrolled from its top (`useScrollEdge`): a hairline along the toolbar's foot. */
  scrolled?: boolean;
}

/**
 * The window's toolbar, over every page: its title, with a line under it
 * where the page has one, and on the right the page's own actions, then
 * the page's way to look again, rightmost. 52 high whatever it holds, as a Mac
 * window's toolbar is, on the window's own background, and its title 20
 * in from the sidebar, as AppKit places a toolbar's title (measured on
 * macOS 27: docs/superpowers/2026-09-29-aesthetics-spec.md §3.2): 13/16
 * bold, the subtitle 11/14 under it, the two centred together. The title takes the focus
 * when what should get it back is gone (`focusOrFallback`).
 *
 * It is the top of the window too. The title bar is an overlay
 * (src-tauri/tauri.conf.json), so the toolbar's row is centred 26px
 * down, on the traffic lights' centre (the Sidebar's first row), and the
 * lights, the title and Check again read as one bar. Like a toolbar, it
 * moves the window from anywhere but its controls: with `deep`, a press
 * anywhere inside it -- on the title, the subtitle, the space between --
 * starts a drag, except on a button, a link or a field, which Tauri's
 * drag script (`src/window/scripts/drag.js` in the tauri crate) leaves to
 * the page. A double-click zooms the window, and selects nothing: on
 * macOS the script lets a double-click's press through to the page, where
 * it would select the word under it.
 */
export function PageHeader({ title, subtitle = null, actions, slotRef, scrolled = false }: PageHeaderProps) {
  // What the status says: the subtitle, but for a failure, which the alert says.
  const shown = subtitle === null || subtitle.failed ? null : subtitle.text;
  return (
    <header
      data-tauri-drag-region="deep"
      className="relative flex h-13 shrink-0 select-none items-center justify-between gap-4 px-5"
    >
      <div className="min-w-0">
        <h1 tabIndex={-1} data-focus-fallback="" className="truncate text-title text-foreground outline-none">
          {title}
        </h1>
        {/* The subtitle is a status, one node for as long as the toolbar is
            there -- in sight, or out of it and empty where the page has
            none -- so a screen reader hears it change: 「正在检查…」, then
            the count. A node put in the page with its words, or given a
            role as they change, is one it may never read. A check that
            failed is an alert of its own, which is heard as it appears. */}
        {/* One line 14 high whose second part, what follows the count,
            wraps out of sight below it where the line has no room for it
            whole: the count alone is cut short only where it alone does
            not fit. */}
        <p
          role="status"
          data-subtitle=""
          title={shown === null ? undefined : subtitle?.note}
          className={shown === null ? "sr-only" : "flex h-3.5 flex-wrap overflow-hidden text-small text-muted"}
        >
          {shown === null ? null : (
            <>
              <span className="min-w-0 truncate">{shown}</span>
              {subtitle?.rest === undefined ? null : (
                <span data-subtitle-rest="" className="whitespace-pre">{` · ${subtitle.rest}`}</span>
              )}
            </>
          )}
        </p>
        {subtitle?.failed ? (
          <p role="alert" className="truncate text-small text-danger-text">
            {subtitle.text}
          </p>
        ) : null}
      </div>
      {/* The page's own actions, then its way to look again, last: the ⟳
          stands at the toolbar's right end on every page, and a button
          before it whose words change -- Update All turning into Update
          Selected (3) -- grows to its left at its own width, moving
          nothing but its own left edge. */}
      <div className="flex shrink-0 items-center gap-2">
        <div ref={slotRef} data-toolbar-slot="" className="flex items-center gap-2 empty:hidden" />
        {actions === undefined ? <CheckAgain /> : actions}
      </div>
      {scrolled ? (
        <span aria-hidden="true" data-scroll-edge="" className="pointer-events-none absolute inset-x-0 bottom-0 h-px bg-separator" />
      ) : null}
    </header>
  );
}
