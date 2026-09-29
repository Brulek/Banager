import { useId, useMemo } from "react";
import type { ComponentType, ReactNode } from "react";
import { useTranslation } from "react-i18next";
import type { Page } from "../store/ui";
import { useSnapshot, useUnknownScan } from "../lib/queries";
import { instanceLabels, instanceNames, sourceWarningOf } from "../lib/sources";
import { useUpdateCount } from "./UpdateProgress";
import { SourceAvatar } from "./SourceAvatar";
import { InstalledIcon, OverviewIcon, SettingsIcon, UnknownIcon, UpdatesIcon, WarningFilledIcon } from "./icons";

interface SidebarProps {
  page: Page;
  onSelectPage: (page: Page) => void;
  /**
   * The source the Installed page shows alone, by instance id, or null
   * while it shows every one (the store's `installedFilter`, only while
   * that page is open): its row under 「来源」 is the one selected then,
   * not 「已安装」's -- one row at a time is (spec R8).
   */
  source?: string | null;
  /** A source's row pressed: the Installed page on that source alone (`openInstalled`). */
  onSelectSource?: (instanceId: string) => void;
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

/** One source's row under 「来源」: its name, how much it has installed, and its first warning, if any. */
interface SourceRow {
  id: string;
  adapterId: string;
  /** Its whole name, 「Homebrew（/usr/local）」: what a screen reader says, and the tooltip. */
  label: string;
  /** Its kind's name, and where it is where another of its kind is on the Mac (`instanceNames`). */
  source: string;
  place: string | null;
  count: number;
  /** The notice's title (`sourceWarningOf`), for the ⚠︎'s tooltip and a screen reader. */
  warning: string | null;
}

/**
 * The sources under 「来源」, one row per source on this Mac, in the
 * snapshot's order -- the order the Installed page groups them in --
 * including one with nothing installed and one that did not answer, which
 * say so when opened (spec R8). None before the first snapshot.
 */
function useSourceRows(): SourceRow[] {
  const { t } = useTranslation();
  const { data: snapshot } = useSnapshot();
  return useMemo(() => {
    if (snapshot === undefined) return [];
    const labels = instanceLabels(t, snapshot.instances);
    const names = instanceNames(t, snapshot.instances);
    const counts = new Map<string, number>();
    for (const artifact of snapshot.artifacts) {
      const id = artifact.key.instance_id;
      counts.set(id, (counts.get(id) ?? 0) + 1);
    }
    return snapshot.instances.map((instance) => {
      const label = labels.get(instance.id) ?? instance.id;
      const count = counts.get(instance.id) ?? 0;
      const warning = sourceWarningOf(instance, label, count);
      return {
        id: instance.id,
        adapterId: instance.adapter_id,
        label,
        source: names.get(instance.id)?.source ?? label,
        place: names.get(instance.id)?.place ?? null,
        count,
        warning: warning === null ? null : t(warning.titleKey, warning.values),
      };
    });
  }, [snapshot, t]);
}

/**
 * A row of the sidebar, as AppKit's medium sidebar draws one: 32 high and
 * inset 10 from either side of the sidebar, so its glyph's 20px box starts
 * 20 in and its words 46 in, and its count ends 20 from the sidebar's
 * edge. Nothing under the pointer; selected, the system fill behind it and
 * its words as they were. `description` is what a screen reader says after
 * its name: what the number means, and a source's warning.
 */
function SidebarRow({
  glyph,
  label,
  place = null,
  active,
  count,
  warning,
  description,
  descriptionId,
  onPress,
}: {
  glyph: ReactNode;
  label: string;
  /**
   * Where a source is, after its name, 11 and in the secondary colour --
   * as Mail sets an account's name after a mailbox's -- which gives way
   * first; the whole name in the tooltip and to a screen reader.
   */
  place?: { text: string; whole: string } | null;
  active: boolean;
  count?: number;
  warning?: string | null;
  description: string | null;
  descriptionId: string;
  onPress: () => void;
}) {
  const counted = count !== undefined && count > 0;
  return (
    <button
      type="button"
      aria-current={active ? "page" : undefined}
      aria-label={place === null ? undefined : place.whole}
      title={place === null ? undefined : place.whole}
      aria-describedby={description !== null ? descriptionId : undefined}
      onClick={onPress}
      className={`flex h-8 w-full items-center gap-1.5 rounded-control px-2.5 text-left text-body ${
        active ? "bg-sidebar-active" : ""
      }`}
    >
      <span className="flex h-5 w-5 shrink-0 items-center justify-center text-accent">{glyph}</span>
      {place === null ? (
        <span className="min-w-0 flex-1 truncate">{label}</span>
      ) : (
        <span className="flex min-w-0 flex-1 items-baseline gap-1">
          <span className="shrink-0">{label}</span>
          <span className="min-w-0 truncate text-small text-muted">{place.text}</span>
        </span>
      )}
      {warning !== undefined && warning !== null ? (
        // Before the count, filled and orange, as Mail marks an account
        // it could not reach; the notice's title under the pointer, and
        // to a screen reader after the name (`description`).
        <span aria-hidden="true" title={warning} className="flex shrink-0">
          <WarningFilledIcon size={12} className="text-warning" />
        </span>
      ) : null}
      {/* A plain number, as a Mac's sidebar counts: none of them is a
          badge. The Dock's shows the updates. */}
      {counted ? (
        <span aria-hidden="true" className="shrink-0 text-small tabular-nums text-muted">
          {count}
        </span>
      ) : null}
      {description !== null ? (
        <span id={descriptionId} hidden>
          {description}
        </span>
      ) : null}
    </button>
  );
}

export function Sidebar({ page, onSelectPage, source = null, onSelectSource }: SidebarProps) {
  const { t } = useTranslation();
  const counts = useCounts();
  const sources = useSourceRows();
  const idPrefix = useId();
  // A source's row stands for the Installed page on that source alone:
  // while it is selected, 「已安装」 is not.
  const sourceShown = page === "installed" ? source : null;

  const entry = (p: Page) => {
    const active = page === p && !(p === "installed" && sourceShown !== null);
    const Icon = PAGE_ICONS[p];
    const count = counts[p];
    const descriptionKey = COUNT_DESCRIPTION_KEYS[p];
    const described = count !== undefined && count > 0 && descriptionKey !== undefined;
    return (
      <SidebarRow
        glyph={<Icon size={20} />}
        label={t(PAGE_LABEL_KEYS[p])}
        active={active}
        count={described ? count : undefined}
        description={described ? t(descriptionKey, { count }) : null}
        descriptionId={`${idPrefix}-${p}-count`}
        onPress={() => onSelectPage(p)}
      />
    );
  };

  const sourceEntry = (row: SourceRow, index: number) => {
    const said = [
      ...(row.count > 0 ? [t("nav.count.installed", { count: row.count })] : []),
      ...(row.warning !== null ? [row.warning] : []),
    ];
    return (
      <SidebarRow
        // The source's own mark, 16, in the 20px box the pages' glyphs
        // have, so the two groups' words start at one x.
        glyph={<SourceAvatar adapterId={row.adapterId} label={row.label} size="xs" />}
        label={row.source}
        place={row.place === null ? null : { text: row.place, whole: row.label }}
        active={sourceShown === row.id}
        count={row.count}
        warning={row.warning}
        description={said.length > 0 ? said.join(t("common.listSeparator")) : null}
        descriptionId={`${idPrefix}-source-${index}`}
        onPress={() => onSelectSource?.(row.id)}
      />
    );
  };

  return (
    <nav
      aria-label={t("nav.label")}
      className="flex w-52 shrink-0 flex-col border-r border-separator bg-sidebar text-foreground"
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
      {/* Everything under the lights' row scrolls as one when the window
          is too short for it, 「来源」 with the pages: nothing folds. */}
      <div data-sidebar-scroller="" className="min-h-0 flex-1 overflow-y-auto pb-3">
        {/* The first row 8 below the lights' row, 60 from the window's top. */}
        <ul className="flex flex-col px-2.5 pt-2">
          {PAGES.map((p) => (
            <li key={p}>{entry(p)}</li>
          ))}
        </ul>
        {sources.length > 0 ? (
          <>
            {/* A group's title, as a Mac sidebar sets one: 11 bold in the
                secondary colour, 14 from the edge, in a 28-high row whose
                words sit at its foot, just over the rows they name. */}
            <p
              id={`${idPrefix}-sources`}
              className="mt-1 flex h-7 items-end px-3.5 pb-0.5 text-small font-bold text-muted"
            >
              {t("nav.sources")}
            </p>
            <ul aria-labelledby={`${idPrefix}-sources`} className="flex flex-col px-2.5">
              {sources.map((row, index) => (
                <li key={row.id}>{sourceEntry(row, index)}</li>
              ))}
            </ul>
          </>
        ) : null}
      </div>
    </nav>
  );
}
