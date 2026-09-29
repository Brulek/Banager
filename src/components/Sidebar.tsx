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

const PAGE_ICONS: Record<Page, ComponentType<{ className?: string }>> = {
  overview: OverviewIcon,
  updates: UpdatesIcon,
  installed: InstalledIcon,
  unknown: UnknownIcon,
  settings: SettingsIcon,
};

/** The entries at the top, in order. Settings sits apart, at the bottom. */
const MAIN_PAGES: Page[] = ["overview", "updates", "installed", "unknown"];

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
      <button
        type="button"
        aria-current={active ? "page" : undefined}
        aria-describedby={described ? descriptionId : undefined}
        onClick={() => onSelectPage(p)}
        className={`flex w-full items-center gap-2.5 rounded-button px-3 py-2 text-left text-body font-medium ${
          active ? "bg-sidebar-active" : ""
        }`}
      >
        <Icon className="shrink-0 text-accent" />
        <span className="min-w-0 flex-1 truncate">{t(PAGE_LABEL_KEYS[p])}</span>
        {described ? (
          <>
            {/* The updates count is the one that asks for something, so
                it is the one in the accent colour. */}
            <span
              aria-hidden="true"
              className={
                p === "updates"
                  ? "shrink-0 rounded-full bg-accent px-1.5 text-small font-semibold tabular-nums text-accent-foreground"
                  : "shrink-0 text-small tabular-nums text-muted"
              }
            >
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
          19px of dark shows above, below and to the left of them; their
          centre, 26px down, is the line the page header's title and
          buttons are centred on (`PageHeader`). A drag region, as the rest
          of a title bar is: dragging it moves the window, and a
          double-click zooms it. */}
      <div data-tauri-drag-region="" className="h-13 shrink-0" />
      <p className="px-6 pb-5 text-small font-semibold text-muted">{t("app.name")}</p>
      <ul className="flex flex-col gap-0.5 px-3">
        {MAIN_PAGES.map((p) => (
          <li key={p}>{entry(p)}</li>
        ))}
      </ul>
      <div className="mx-3 mt-auto border-t border-separator pt-3">{entry("settings")}</div>
    </nav>
  );
}
