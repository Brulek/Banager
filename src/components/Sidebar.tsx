import { useId, useMemo } from "react";
import type { ComponentType } from "react";
import { useTranslation } from "react-i18next";
import type { Page } from "../store/ui";
import { useSettings, useSnapshot, useUnknownScan } from "../lib/queries";
import { actionableUpdatesOf } from "../lib/updateState";
import { InstalledIcon, SettingsIcon, UnknownIcon, UpdatesIcon } from "./icons";

interface SidebarProps {
  page: Page;
  onSelectPage: (page: Page) => void;
}

/** Each page's name: its entry here, and the title over it (`PageHeader`). */
export const PAGE_LABEL_KEYS: Record<Page, string> = {
  updates: "nav.updates",
  installed: "nav.installed",
  unknown: "nav.unknown",
  settings: "nav.settings",
};

const PAGE_ICONS: Record<Page, ComponentType<{ className?: string }>> = {
  updates: UpdatesIcon,
  installed: InstalledIcon,
  unknown: UnknownIcon,
  settings: SettingsIcon,
};

/** The entries at the top, in order. Settings sits apart, at the bottom. */
const MAIN_PAGES: Page[] = ["updates", "installed", "unknown"];

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
 * - Updates: the updates the Updates page offers to install
 *   (`actionableUpdatesOf`, the list behind its "N updates available").
 * - Installed: everything the Installed page lists, components other
 *   software brought in included.
 * - Unknown: what the last scan found, once one has run. Nothing here
 *   starts one; the Unknown page does, when it is opened.
 *
 * Zero shows nothing, like no count at all.
 */
function useCounts(): Partial<Record<Page, number>> {
  const { data: snapshot } = useSnapshot();
  const { data: settings } = useSettings();
  const { data: scan } = useUnknownScan();
  return useMemo(
    () => ({
      updates: snapshot && settings ? actionableUpdatesOf(snapshot, settings).length : undefined,
      installed: snapshot?.artifacts.length,
      unknown: scan?.entries.length,
    }),
    [snapshot, settings, scan],
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
        className={`flex w-full items-center gap-2.5 rounded-button px-3 py-2 text-left text-body font-medium outline-none transition-colors focus-visible:ring-2 focus-visible:ring-sidebar-text ${
          active ? "bg-sidebar-active text-white" : "text-sidebar-text hover:bg-white/5"
        }`}
      >
        <Icon className={active ? "shrink-0 text-white" : "shrink-0 text-sidebar-muted"} />
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
                  : "shrink-0 text-small tabular-nums text-sidebar-muted"
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
      className="flex w-52 shrink-0 flex-col bg-linear-to-b from-sidebar-top to-sidebar-bottom px-3 pb-3 pt-4 text-sidebar-text"
    >
      <p className="px-3 pb-5 text-small font-semibold text-sidebar-muted">{t("app.name")}</p>
      <ul className="flex flex-col gap-0.5">
        {MAIN_PAGES.map((p) => (
          <li key={p}>{entry(p)}</li>
        ))}
      </ul>
      <div className="mt-auto border-t border-white/10 pt-3">{entry("settings")}</div>
    </nav>
  );
}
