import { useId, useMemo } from "react";
import type { ComponentType } from "react";
import { useTranslation } from "react-i18next";
import type { Page } from "../store/ui";
import { useSnapshot, useUnknownScan } from "../lib/queries";
import { useUpdateCount } from "./UpdateProgress";
import { InstalledIcon, OverviewIcon, SettingsIcon, UnknownIcon, UpdatesIcon } from "./icons";

interface SidebarProps {
  page: Page;
  onSelectPage: (page: Page) => void;
}

/** Each page's name: its entry here, and the title over it (`PageHeader`). */
export const PAGE_LABEL_KEYS: Record<Page, string> = {
  overview: "nav.overview",
  updates: "nav.updates",
  installed: "nav.installed",
  unknown: "nav.unknown",
  settings: "nav.settings",
};

const PAGE_ICONS: Record<Page, ComponentType<{ size?: number; className?: string }>> = {
  overview: OverviewIcon,
  updates: UpdatesIcon,
  installed: InstalledIcon,
  unknown: UnknownIcon,
  settings: SettingsIcon,
};

/**
 * The entries, in order: the Overview, the pages about the Mac, and
 * Settings fifth, in the same group (spec §3.1) -- until it has a window
 * of its own, opened with ⌘, as a Mac app's settings are.
 */
const PAGES: Page[] = ["overview", "updates", "installed", "unknown", "settings"];

/**
 * What an entry's count means, as a screen reader says it after the
 * entry's name. The number on screen is only a number.
 */
const COUNT_DESCRIPTION_KEYS: Partial<Record<Page, string>> = {
  updates: "nav.count.updates",
  installed: "nav.count.installed",
  unknown: "nav.count.unknown",
};

/**
 * The number beside each entry, or nothing:
 *
 * - Updates: the updates the Updates page offers to start now
 *   (`useUpdateCount`), its 「N 个可更新」: those an update is installing
 *   now are not counted, as the page's header does not count them. The
 *   Dock's badge shows the same number (`useDockBadge`).
 * - Installed: everything the Installed page lists, components other
 *   software brought in included.
 * - Unknown: what the last scan found, once one has run. Nothing here
 *   starts one; the Unknown page does, when it is opened.
 *
 * Zero shows nothing, like no count at all.
 */
function useCounts(): Partial<Record<Page, number>> {
  const updates = useUpdateCount();
  const { data: snapshot } = useSnapshot();
  const { data: scan } = useUnknownScan();
  return useMemo(
    () => ({
      updates,
      installed: snapshot?.artifacts.length,
      unknown: scan?.entries.length,
    }),
    [updates, snapshot, scan],
  );
}

export function Sidebar({ page, onSelectPage }: SidebarProps) {
  const { t } = useTranslation();
  const counts = useCounts();
  const idPrefix = useId();

  const entry = (p: Page) => {
    const active = page === p;
    const Icon = PAGE_ICONS[p];
    const count = counts[p];
    const descriptionKey = COUNT_DESCRIPTION_KEYS[p];
    const described = count !== undefined && count > 0 && descriptionKey !== undefined;
    const descriptionId = `${idPrefix}-${p}-count`;
    return (
      // A source list's row, as AppKit's medium sidebar draws one: 32 high
      // and inset 10 from either side of the sidebar, so its icon's 20px
      // box starts 20 in and its words 46 in, and its count ends 20 from
      // the sidebar's edge. Nothing under the pointer; selected, the
      // system fill behind it and its words as they were.
      <button
        type="button"
        aria-current={active ? "page" : undefined}
        aria-describedby={described ? descriptionId : undefined}
        onClick={() => onSelectPage(p)}
        className={`flex h-8 w-full items-center gap-1.5 rounded-control px-2.5 text-left text-body ${
          active ? "bg-sidebar-active" : ""
        }`}
      >
        <span className="flex h-5 w-5 shrink-0 items-center justify-center text-accent">
          <Icon size={20} />
        </span>
        <span className="min-w-0 flex-1 truncate">{t(PAGE_LABEL_KEYS[p])}</span>
        {described ? (
          <>
            {/* A plain number, as a Mac's sidebar counts: none of them
                is a badge. The Dock's shows the updates. */}
            <span aria-hidden="true" className="shrink-0 text-small tabular-nums text-muted">
              {count}
            </span>
            <span id={descriptionId} hidden>
              {t(descriptionKey, { count })}
            </span>
          </>
        ) : null}
      </button>
    );
  };

  return (
    <nav
      aria-label={t("nav.label")}
      className="flex w-52 shrink-0 flex-col border-r border-separator bg-sidebar pb-3 text-foreground"
    >
      {/* The window's title bar is an overlay (src-tauri/tauri.conf.json),
          and this is the row of it the sidebar keeps for the traffic
          lights macOS draws there: 52px tall, the sidebar's full width,
          with nothing in it. `trafficLightPosition` puts the 14px lights
          19px in from the left and from the top, where macOS 27 draws them
          in a window with a toolbar (src/test/windowChrome.test.ts), so
          19px of the sidebar shows above, below and to the left of them;
          their centre, 26px down, is the line the page header's title and
          buttons are centred on (`PageHeader`). A drag region, as the rest
          of a title bar is: dragging it moves the window, and a
          double-click zooms it. No name of the app under it: the window
          is the app's, as a Mac app's sidebar says. */}
      <div data-tauri-drag-region="" className="h-13 shrink-0" />
      {/* The first row 8 below the lights' row, 60 from the window's top. */}
      <ul className="flex flex-col px-2.5 pt-2">
        {PAGES.map((p) => (
          <li key={p}>{entry(p)}</li>
        ))}
      </ul>
    </nav>
  );
}
