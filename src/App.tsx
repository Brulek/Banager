import { useRef, useState, type ReactNode, type Ref, type RefObject } from "react";
import { useTranslation } from "react-i18next";
import { useLanguageSync } from "./i18n/useLanguageSync";
import { PAGE_LABEL_KEYS, Sidebar } from "./components/Sidebar";
import { CheckAgain, PageHeader, type PageSubtitle } from "./components/PageHeader";
import { ToolbarSlotProvider, useScrollEdge } from "./components/Toolbar";
import { OverviewPage } from "./pages/OverviewPage";
import { InstalledPage } from "./pages/InstalledPage";
import { UpdatesPage, useUpdatesHeadline } from "./pages/UpdatesPage";
import { ScanAgain, UnknownPage } from "./pages/UnknownPage";
import { SettingsPage } from "./pages/SettingsPage";
import { OperationBar } from "./components/OperationBar";
import { LogDrawer } from "./components/LogDrawer";
import { SnapshotStatus } from "./components/SnapshotStatus";
import { QuitQuestion } from "./components/QuitQuestion";
import { useOperationEvents, useRefreshInFlight, useStartupRefresh } from "./lib/events";
import { useInventoryPreview } from "./lib/inventoryPreview";
import { useSizes, useSnapshot, useUnknownScan } from "./lib/queries";
import { modelsTotalText } from "./lib/sizes";
import { sizeTotalsOf, viewTotalText } from "./lib/sizeTotals";
import { instanceLabels } from "./lib/sources";
import { useNoBrowserContextMenu } from "./lib/contextMenu";
import { useMenuCommands } from "./lib/menu";
import { useDockBadge } from "./lib/dockBadge";
import { useUpdateNotification } from "./lib/updateNotification";
import { useUiStore, type Page } from "./store/ui";

/**
 * What each page's toolbar has on the right: its own way to look again,
 * or nothing. The pages about the sources check them again; Other
 * Programs (`UnknownPage`) scans again -- only that, never two refresh
 * buttons stacked; and Settings looks at nothing. A `switch` with no
 * default, so a page added to `Page` without an answer here fails `tsc`.
 */
function headerActions(page: Page): ReactNode {
  switch (page) {
    case "overview":
    case "updates":
    case "installed":
      return <CheckAgain />;
    case "unknown":
      return <ScanAgain />;
    case "settings":
      return null;
  }
}

/**
 * The line under each page's title in the toolbar (spec §3.2): what the
 * page lists -- 「10个可更新」, the Updates page's own headline
 * (`useUpdatesHeadline`); 「51个工具」, everything installed, as the
 * sidebar counts it; 「5个程序」, what the last scan found; nothing for
 * nothing, which the page says in a sentence of its own -- or, on the
 * pages about the sources, 「正在检查…」 while a check runs -- on the
 * Installed page, while the first check lists what it found before its
 * update checks are done (`useInventoryPreview`), 「已找到51个工具 · 正在
 * 检查更新…」 -- and, as an
 * alert, 「无法完成检查」 once one has failed (`startupRefreshError`, which
 * every refresh sets or clears), in place of a count the check could not
 * bring up to date. On one source alone, the Installed page counts that
 * source's tools, 「30个工具」, under its name (`useShownSource`) -- and on
 * Ollama's, what its models take together, 「2个工具 · Ollama模型共约6.2 GB」
 * (`modelsTotalText`); on any other, or on all of them, what the tools
 * counted take, 「55个工具 · 共约9.8 GB」 (`viewTotalText`). Other Programs
 * says 「正在扫描…」 while it scans.
 * The Overview has a status row of its own, which says all of that, and
 * Settings nothing to count: no subtitle (spec §3.2). A `switch` with no
 * default, so a page added to `Page` without an answer here fails `tsc`.
 */
function usePageSubtitle(page: Page): PageSubtitle | null {
  const { t } = useTranslation();
  const shownSource = useShownSource();
  const checking = useRefreshInFlight();
  const lastCheckFailed = useUiStore((s) => s.startupRefreshError !== null);
  const { data: snapshot } = useSnapshot();
  const { data: sizes } = useSizes();
  const updatesHeadline = useUpdatesHeadline();
  const scan = useUnknownScan();
  // The first check's list, while that check is still checking for updates.
  const preview = useInventoryPreview();
  const said = (text: string | null): PageSubtitle | null => (text === null ? null : { text, failed: false });
  // None for none, as the sidebar shows no 0: the page says it has
  // nothing in a sentence of its own.
  const counted = (key: string, count: number | undefined) =>
    count === undefined || count === 0 ? null : t(key, { count });
  // What the tools counted take together, once measured: 「共约9.8 GB」, or
  // 「共至少约…」 when some of them have no size (`sizeTotalsOf`).
  const viewTotal = (source: string | null): string | null => {
    const totals = sizeTotalsOf(sizes, snapshot);
    const total = source === null ? totals.all : (totals.bySource.get(source) ?? null);
    return total === null ? null : viewTotalText(t, total);
  };
  switch (page) {
    case "overview":
    case "settings":
      return null;
    case "updates":
    case "installed":
      // 「已找到51个工具 · 正在检查更新…」 over the first check's list.
      if (checking && page === "installed" && preview !== null) {
        return said(t("inventoryPreview.subtitle", { count: preview.artifacts.length }));
      }
      if (checking) return said(t("common.checking"));
      if (lastCheckFailed) return { text: t("header.checkFailed"), failed: true };
      if (page === "updates") return said(updatesHeadline);
      // On one source alone, that source's: its name is the title. On
      // Ollama's, what its models take together after it, once measured;
      // on any other, or all, what the tools counted take together.
      return said(
        [
          counted(
            "toolbar.toolCount",
            shownSource === null
              ? snapshot?.artifacts.length
              : snapshot?.artifacts.filter((artifact) => artifact.key.instance_id === shownSource).length,
          ),
          (shownSource === null ? null : modelsTotalText(t, sizes, shownSource)) ?? viewTotal(shownSource),
        ]
          .filter((part): part is string => part !== null)
          .join(" · ") || null,
      );
    case "unknown":
      if (scan.isFetching) return said(t("unknown.scanning"));
      return said(counted("toolbar.programCount", scan.data?.entries.length));
  }
}

/**
 * The source the Installed page shows alone, while it is open on one
 * (`installedFilter`) that the snapshot lists, or null: what its toolbar
 * titles and counts, as Mail titles its window with the mailbox it shows.
 */
function useShownSource(): string | null {
  const page = useUiStore((s) => s.page);
  const filter = useUiStore((s) => s.installedFilter);
  const { data: snapshot } = useSnapshot();
  if (page !== "installed" || filter === null) return null;
  return snapshot?.instances.some((instance) => instance.id === filter) ? filter : null;
}

/**
 * The window's toolbar for `page` (`PageHeader`): its title, its subtitle
 * and its way to look again. A component of its own, so that what the
 * subtitle reads -- the operations, which move at every step of every
 * update (`useUpdatesHeadline`) -- redraws the toolbar alone, as
 * `UpdateWatchers` keeps the Dock's badge from redrawing the window. So
 * does its hairline, once `pageBox` has scrolled from its top
 * (`useScrollEdge`): in `App`, the first step of every scroll drew the
 * page again, and every row of its list in sight with it.
 */
function PageToolbar({
  page,
  slotRef,
  pageBox,
}: {
  page: Page;
  slotRef: Ref<HTMLDivElement>;
  pageBox: RefObject<HTMLDivElement | null>;
}) {
  const { t } = useTranslation();
  const scrolled = useScrollEdge(pageBox, page);
  const subtitle = usePageSubtitle(page);
  const shownSource = useShownSource();
  const { data: snapshot } = useSnapshot();
  // The Installed page on one source is titled with that source's name,
  // the one its row in the sidebar has (`instanceLabels`).
  const title =
    shownSource !== null && snapshot !== undefined
      ? (instanceLabels(t, snapshot.instances).get(shownSource) ?? t(PAGE_LABEL_KEYS[page]))
      : t(PAGE_LABEL_KEYS[page]);
  return (
    <PageHeader
      title={title}
      subtitle={subtitle}
      actions={headerActions(page)}
      slotRef={slotRef}
      scrolled={scrolled}
    />
  );
}

/**
 * What the window keeps up to date about the updates, out of sight: the
 * Dock's badge (`useDockBadge`) and the update notification's report
 * (`useUpdateNotification`). A component of their own, which draws
 * nothing: both read the operations, which change at every step of every
 * update -- a few hundred times while Update all submits 120 tools -- and
 * in `App` itself each change drew all of the window again, whatever it
 * showed: the header, the log drawer, the page, Settings as much as
 * Updates. What does show the operations reads them itself.
 */
function UpdateWatchers() {
  useDockBadge();
  useUpdateNotification();
  return null;
}

function App() {
  useLanguageSync();
  useNoBrowserContextMenu();
  const page = useUiStore((s) => s.page);
  const openPage = useUiStore((s) => s.openPage);
  const openInstalled = useUiStore((s) => s.openInstalled);
  const installedFilter = useUiStore((s) => s.installedFilter);
  useOperationEvents();
  useStartupRefresh();
  useMenuCommands();
  // The toolbar's box for the page's own actions (`ToolbarItems`), once
  // it is drawn, and the page's own box, whose scroll the toolbar follows.
  const [toolbarSlot, setToolbarSlot] = useState<HTMLDivElement | null>(null);
  const pageBox = useRef<HTMLDivElement>(null);

  return (
    <div className="flex h-screen bg-[var(--color-content)] text-[var(--color-foreground)]">
      <UpdateWatchers />
      {/* The sidebar's Installed opens the page on everything installed,
          which is what its count counts; a source's row, under 「来源」,
          opens it on that source alone, and is the row selected then.
          The last row there, Other Programs, opens that page. */}
      <Sidebar
        page={page}
        source={installedFilter}
        onSelectPage={openPage}
        onSelectSource={openInstalled}
      />
      <div className="flex min-w-0 flex-1 flex-col">
        <main className="flex min-h-0 flex-1 flex-col">
          {/* Outside `SnapshotStatus`, so the title and Check again stay
              put whatever the page below shows -- the first check under
              way, a failed first check, an empty Mac. */}
          <PageToolbar page={page} slotRef={setToolbarSlot} pageBox={pageBox} />
          {/* The page's own box. The Installed and Updates pages size
              their lists to its height (`h-full`) and scroll inside them;
              the other pages scroll here. Either way the toolbar's
              hairline follows (`useScrollEdge`). */}
          <ToolbarSlotProvider value={toolbarSlot}>
            <div ref={pageBox} className="min-h-0 flex-1 overflow-y-auto">
              {/* Settings and Unknown are not snapshot pages: Settings never
                  was, and the unknown-source scan is judged against the
                  snapshot but is not part of it -- on a Mac with no source at
                  all, SnapshotStatus would replace it with "Banager found
                  nothing it can manage", the one case where every program on
                  the machine belongs on it. */}
              {page === "settings" ? (
                <SettingsPage />
              ) : page === "unknown" ? (
                <UnknownPage />
              ) : page === "overview" ? (
                // The Overview shows the first check itself while it runs
                // (`FirstCheck`, which SnapshotStatus shows for the other
                // two), and a check that found nothing, in the status row
                // it says every state in; the rest is the snapshot's, as on
                // any page.
                <SnapshotStatus showsFirstCheck showsNothingFound>
                  <OverviewPage />
                </SnapshotStatus>
              ) : (
                <SnapshotStatus showsInventoryPreview={page === "installed"}>
                  {page === "installed" ? <InstalledPage /> : <UpdatesPage />}
                </SnapshotStatus>
              )}
            </div>
          </ToolbarSlotProvider>
        </main>
        {/* Its own footer, and none at all until something has run. */}
        <OperationBar />
      </div>
      <LogDrawer />
      {/* Asks before a quit while an operation is under way, when Rust
          says one waits on it (src-tauri/src/quit.rs); nothing until then. */}
      <QuitQuestion />
    </div>
  );
}

export default App;
