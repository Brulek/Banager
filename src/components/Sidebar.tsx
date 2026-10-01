import { useEffect, useId, useMemo, useRef, useState } from "react";
import type { ComponentType, FocusEvent, KeyboardEvent, ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useUiStore, type Page } from "../store/ui";
import { useSnapshot } from "../lib/queries";
import { isStartupSnapshot } from "../lib/events";
import { useInventoryPreview } from "../lib/inventoryPreview";
import { instanceLabels, instanceNames, sourceWarningOf } from "../lib/sources";
import { useUpdateCount } from "./UpdateProgress";
import { SourceAvatar } from "./SourceAvatar";
import { InstalledIcon, OverviewIcon, SettingsIcon, TerminalIcon, UpdatesIcon, WarningFilledIcon } from "./icons";

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

/**
 * Each page's name: its row here, and the title over it (`PageHeader`).
 * Other Programs' is `nav.unknown`: the key keeps the name the code
 * gives the page (`UnknownPage`), and only the words are the user's.
 */
export const PAGE_LABEL_KEYS: Record<Page, string> = {
  overview: "nav.overview",
  updates: "nav.updates",
  installed: "nav.installed",
  unknown: "nav.unknown",
  settings: "nav.settings",
};

/** The pages in the list at the top: every page but Other Programs, whose row is under 「来源」. */
type ListedPage = Exclude<Page, "unknown">;

const PAGE_ICONS: Record<ListedPage, ComponentType<{ size?: number; className?: string }>> = {
  overview: OverviewIcon,
  updates: UpdatesIcon,
  installed: InstalledIcon,
  settings: SettingsIcon,
};

/**
 * The entries, in order: the Overview, the pages about the Mac, and
 * Settings fourth, in the same group (spec §3.1) -- until it has a window
 * of its own, opened with ⌘, as a Mac app's settings are. Not Other
 * Programs: what no source installed is the last row under 「来源」,
 * after every source (`OtherProgramsMark`'s row), as CleanMyMac lists the
 * apps it cannot place last, as "Other", under where the rest came from.
 */
const PAGES: ListedPage[] = ["overview", "updates", "installed", "settings"];

/**
 * What an entry's count means, as a screen reader says it after the
 * entry's name. The number on screen is only a number.
 */
const COUNT_DESCRIPTION_KEYS: Partial<Record<ListedPage, string>> = {
  updates: "nav.count.updates",
  installed: "nav.count.installed",
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
 *
 * Zero shows nothing, like no count at all. Other Programs has none: its
 * row stands with the sources', which have none either (`SourceRow`), and
 * its page says how many in its header's subtitle (「5个程序」).
 */
function useCounts(): Partial<Record<ListedPage, number>> {
  const updates = useUpdateCount();
  const { data: snapshot } = useSnapshot();
  // While the first check still checks for updates, what it has found
  // installed: the list the Installed page shows meanwhile.
  const preview = useInventoryPreview();
  return useMemo(
    () => ({
      updates,
      installed: preview?.artifacts.length ?? snapshot?.artifacts.length,
    }),
    [updates, snapshot, preview],
  );
}

/**
 * One source's row under 「来源」: its name and its first warning, if any.
 * Not how much it has installed: beside the Updates page's 「更新 10」 a
 * number by each source read as that source's updates, and the page it
 * opens says it in its header's subtitle (「30个工具」).
 */
interface SourceRow {
  id: string;
  adapterId: string;
  /** Its whole name, 「Homebrew（Intel）」: what a screen reader says, and the tooltip. */
  label: string;
  /** Its kind's name, and which one it is where another of its kind is on the Mac (`instanceNames`): 「Intel」. */
  source: string;
  place: string | null;
  /** The notice's title (`sourceWarningOf`), for the ⚠︎'s tooltip and a screen reader. */
  warning: string | null;
}

/**
 * The sources under 「来源」, one row per source on this Mac, in the
 * snapshot's order -- the order the Installed page groups them in --
 * including one with nothing installed and one that did not answer, which
 * say so when opened (spec R8). None before the first snapshot. Other
 * Programs' row follows them, whatever there is, while 「来源」 is drawn
 * (`useSourcesShown`).
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
      // Its rows as the page it opens draws them (`rowsOnScreen`), though
      // only the notice's sentence turns on them, not the title said here.
      const warning = sourceWarningOf(instance, label, counts.get(instance.id) ?? 0);
      return {
        id: instance.id,
        adapterId: instance.adapter_id,
        label,
        source: names.get(instance.id)?.source ?? label,
        place: names.get(instance.id)?.place ?? null,
        warning: warning === null ? null : t(warning.titleKey, warning.values),
      };
    });
  }, [snapshot, t]);
}

/**
 * Whether 「来源」 is drawn, Other Programs' row with it: once the first
 * check has answered -- with the sources, or with none, where every
 * program is one of Other Programs' -- or failed, and then that row is
 * the one way to its page there is in the sidebar; and whenever that page
 * is open, so its row is there, selected. Not while the first check runs,
 * the placeholder the backend starts from in hand (`isStartupSnapshot`):
 * for the minute or two that can take, 「来源」 with Other Programs alone
 * under it read as "no source found", and the sources then arriving above
 * it moved the row down from under the pointer. The View menu (⌘4) opens
 * the page meanwhile.
 */
function useSourcesShown(page: Page): boolean {
  const { data: snapshot, isError } = useSnapshot();
  const checkFailed = useUiStore((s) => s.startupRefreshError !== null);
  if (page === "unknown" || isError || checkFailed) return true;
  return snapshot !== undefined && !isStartupSnapshot(snapshot);
}

/**
 * A row of the sidebar, as AppKit's medium sidebar draws one: 32 high and
 * inset 10 from either side of the sidebar, so its glyph's 20px box starts
 * 20 in and its words 46 in, and what stands at its right -- a page's
 * count, a source's ⚠︎ -- ends 20 from the sidebar's edge. Nothing under
 * the pointer; selected, the system fill behind it and its words as they
 * were. `description` is what a screen reader says after its name: what a
 * page's number means, or a source's warning. Brought into view as the
 * row selected (`Sidebar`), it keeps the room the lists keep around their
 * rows: 8 above, as over the first row, and 12 below, as under the last --
 * not flush with the lights' row or the window's foot.
 */
function SidebarRow({
  glyph,
  label,
  place = null,
  active,
  count,
  reserveSlot = false,
  warning,
  description,
  descriptionId,
  onPress,
  tabIndex,
  onFocus,
}: {
  glyph: ReactNode;
  label: string;
  /**
   * Which of two sources of one kind this is -- 「Apple芯片」, 「Intel」 --
   * on a line of its own under the name, 11 and in the secondary colour, as
   * System Settings sets what an account is under its name: the row 40
   * high for it. After the name, on its line, 「Apple silicon」 would be cut
   * short in the sidebar's width, and it is the part that tells the two
   * rows apart. The whole name in the tooltip and to a screen reader.
   */
  place?: { text: string; whole: string } | null;
  active: boolean;
  count?: number;
  /**
   * The place at its right is kept, 20 wide, with nothing in it: a
   * source's row, whose ⚠︎ stands there, at its right, where a page's
   * count ends -- so its name has the same room on every source's row,
   * with a ⚠︎ or without.
   */
  reserveSlot?: boolean;
  /** A source's warning: a ⚠︎ in the place at its right (`reserveSlot`). */
  warning?: string | null;
  description: string | null;
  descriptionId: string;
  onPress: () => void;
  /** Its place in the sidebar's roving tabindex (`Sidebar`): 0 for the one row Tab reaches, -1 for the rest. */
  tabIndex: 0 | -1;
  /** It has taken the focus: the row Tab comes back to, until the focus leaves the sidebar. */
  onFocus: () => void;
}) {
  const counted = count !== undefined && count > 0;
  const warned = warning !== undefined && warning !== null;
  return (
    <button
      type="button"
      data-sidebar-row=""
      tabIndex={tabIndex}
      onFocus={onFocus}
      aria-current={active ? "page" : undefined}
      aria-label={place === null ? undefined : place.whole}
      title={place === null ? undefined : place.whole}
      aria-describedby={description !== null ? descriptionId : undefined}
      onClick={onPress}
      className={`flex ${place === null ? "h-8" : "h-10"} w-full scroll-mt-2 scroll-mb-3 items-center gap-1.5 rounded-control px-2.5 text-left text-body ${
        active ? "bg-sidebar-active" : ""
      }`}
    >
      <span className="flex h-5 w-5 shrink-0 items-center justify-center text-accent">{glyph}</span>
      {place === null ? (
        <span className="min-w-0 flex-1 truncate">{label}</span>
      ) : (
        <span className="flex min-w-0 flex-1 flex-col">
          <span className="truncate">{label}</span>
          <span className="truncate text-small text-muted">{place.text}</span>
        </span>
      )}
      {/* At the row's right, in a column 20 wide at least -- three digits --
          whatever is in it at its right edge, 20 from the sidebar's: a
          page's plain number, as a Mac's sidebar counts, none of them a
          badge (the Dock's shows the updates); or a source's ⚠︎, filled
          and orange, where Mail puts one on an account it could not reach
          -- the notice's title under the pointer, and to a screen reader
          after the name (`description`). */}
      {counted || warned || reserveSlot ? (
        <span
          aria-hidden="true"
          data-trailing=""
          className="flex min-w-5 shrink-0 justify-end"
        >
          {warned ? (
            <span title={warning} className="flex">
              <WarningFilledIcon size={12} className="text-warning" />
            </span>
          ) : counted ? (
            <span data-count="" className="text-small tabular-nums text-muted">
              {count}
            </span>
          ) : null}
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

/**
 * Other Programs' mark, where a source's is on its row: the tile each of
 * its programs has on the page (`ProgramAvatar` in
 * src/pages/UnknownPage.tsx) -- a prompt, white on the neutral grey -- at
 * a source's 16 (`SourceAvatar`'s `xs`, its corners 4), in the 20px box
 * every row's glyph has. Neutral, not the accent the pages' glyphs are
 * drawn in: it is the last of the sources, and has no colour of its own.
 * In dark mode, the 1px edge of 12% white a source's logo has there
 * (`PackLogo`): without it, dark mode's grey tile all but vanished into
 * the selected row's fill.
 */
function OtherProgramsMark() {
  return (
    <span
      aria-hidden="true"
      data-other-programs-mark=""
      className="inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-[4px] bg-neutral-avatar text-white dark:inset-ring dark:inset-ring-white/12"
    >
      <TerminalIcon size={12} />
    </span>
  );
}

/** The keys that move the focus between the sidebar's rows (`Sidebar`), and where each takes it. */
const ROW_KEYS: Record<string, (at: number, count: number) => number> = {
  ArrowDown: (at, count) => Math.min(at + 1, count - 1),
  ArrowUp: (at) => Math.max(at - 1, 0),
  Home: () => 0,
  End: (_, count) => count - 1,
};

export function Sidebar({ page, onSelectPage, source = null, onSelectSource }: SidebarProps) {
  const { t } = useTranslation();
  const counts = useCounts();
  const sources = useSourceRows();
  const sourcesShown = useSourcesShown(page);
  const idPrefix = useId();
  // A source's row stands for the Installed page on that source alone:
  // while it is selected, 「已安装」 is not.
  const sourceShown = page === "installed" ? source : null;

  // One Tab stop for the whole sidebar, as a Mac's source list is one
  // control (a roving tabindex): Tab comes in on the row selected -- or,
  // while the focus is in the sidebar, on the row it was last on -- and
  // the next Tab leaves for the toolbar and the page. ↑ and ↓ move from
  // row to row, the pages' and the sources' -- Other Programs last -- as
  // one list, Home and End to its ends; Return and Space open a row, as a
  // click does.
  const currentKey = sourceShown !== null ? `source:${sourceShown}` : `page:${page}`;
  const [focusedKey, setFocusedKey] = useState<string | null>(null);
  const rowKeys = [
    ...PAGES.map((p) => `page:${p}`),
    ...(sourcesShown ? [...sources.map((row) => `source:${row.id}`), "page:unknown"] : []),
  ];
  const tabKey = [focusedKey, currentKey].find((key) => key !== null && rowKeys.includes(key)) ?? rowKeys[0];
  const roving = (key: string) => ({
    tabIndex: key === tabKey ? (0 as const) : (-1 as const),
    onFocus: () => setFocusedKey(key),
  });
  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const move = ROW_KEYS[event.key];
    if (move === undefined || event.altKey || event.metaKey || event.ctrlKey) return;
    const rows = Array.from(event.currentTarget.querySelectorAll<HTMLElement>("[data-sidebar-row]"));
    const at = rows.indexOf(event.target as HTMLElement);
    if (at < 0) return;
    event.preventDefault();
    rows[move(at, rows.length)].focus();
  };
  // The focus gone from the sidebar: the next Tab into it comes in on the
  // row selected again.
  const onBlur = (event: FocusEvent<HTMLDivElement>) => {
    if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setFocusedKey(null);
  };

  // The row selected, brought into view when it changes: Other Programs,
  // the last row, opened from the View menu (⌘4) in a short window on a
  // Mac with many sources, is otherwise below the fold, the page open and
  // no row seen selected. And when rows come or go above it -- the first
  // snapshot's sources, arriving over a row already selected. `nearest`:
  // nothing moves when it is in view already, as after a click; else as
  // little as shows it, and the margin it has (`SidebarRow`). Not on every
  // render: scrolled by hand, the sidebar stays where it was left. jsdom
  // has no `scrollIntoView`.
  const scroller = useRef<HTMLDivElement>(null);
  const rowCount = rowKeys.length;
  useEffect(() => {
    scroller.current?.querySelector<HTMLElement>('[aria-current="page"]')?.scrollIntoView?.({ block: "nearest" });
  }, [currentKey, rowCount]);

  const entry = (p: ListedPage) => {
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
        {...roving(`page:${p}`)}
      />
    );
  };

  // A source's row says its ⚠︎'s words to a screen reader, as the pointer
  // shows them, and no count: none is on it to explain, and the page it
  // opens says how many in its header's subtitle, which is read there too.
  const sourceEntry = (row: SourceRow, index: number) => {
    return (
      <SidebarRow
        // The source's own mark, 16, in the 20px box the pages' glyphs
        // have, so the two groups' words start at one x.
        glyph={<SourceAvatar adapterId={row.adapterId} label={row.label} size="xs" />}
        label={row.source}
        place={row.place === null ? null : { text: row.place, whole: row.label }}
        active={sourceShown === row.id}
        reserveSlot
        warning={row.warning}
        description={row.warning}
        descriptionId={`${idPrefix}-source-${index}`}
        onPress={() => onSelectSource?.(row.id)}
        {...roving(`source:${row.id}`)}
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
          is too short for it, 「来源」 with the pages: nothing folds. 12
          under the last row, inside what scrolls -- an engine that leaves a
          scroller's own bottom padding out of what it scrolls to would
          leave the last row flush with the window's edge. */}
      <div ref={scroller} data-sidebar-scroller="" onKeyDown={onKeyDown} onBlur={onBlur} className="min-h-0 flex-1 overflow-y-auto">
        <div className="pb-3">
          {/* The first row 8 below the lights' row, 60 from the window's top. */}
          <ul className="flex flex-col px-2.5 pt-2">
            {PAGES.map((p) => (
              <li key={p}>{entry(p)}</li>
            ))}
          </ul>
          {/* A group's title, as a Mac sidebar sets one: 11 bold in the
              secondary colour, 14 from the edge, in a 28-high row whose
              words sit at its foot, just over the rows they name. Drawn
              once the first check has answered or failed, or with Other
              Programs' page open (`useSourcesShown`) -- on a Mac with no
              source at all too, Other Programs alone under it, where
              every program is one of those. */}
          {sourcesShown ? (
            <>
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
                {/* The last row: the programs no source installed, whose page
                    it opens -- its own, with its own list, not the Installed
                    page on a source, as the rows above it do. Selected while
                    that page is open, as a source's row is while it shows that
                    source; no count, as none is by a source. */}
                <li>
                  <SidebarRow
                    glyph={<OtherProgramsMark />}
                    label={t(PAGE_LABEL_KEYS.unknown)}
                    active={page === "unknown"}
                    description={null}
                    descriptionId={`${idPrefix}-unknown`}
                    onPress={() => onSelectPage("unknown")}
                    {...roving("page:unknown")}
                  />
                </li>
              </ul>
            </>
          ) : null}
        </div>
      </div>
    </nav>
  );
}
