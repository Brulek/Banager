import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { useLanguageSync } from "./i18n/useLanguageSync";
import { PAGE_LABEL_KEYS, Sidebar } from "./components/Sidebar";
import { CheckAgain, PageHeader } from "./components/PageHeader";
import { OverviewPage } from "./pages/OverviewPage";
import { InstalledPage } from "./pages/InstalledPage";
import { UpdatesPage } from "./pages/UpdatesPage";
import { ScanAgain, UnknownPage } from "./pages/UnknownPage";
import { SettingsPage } from "./pages/SettingsPage";
import { OperationBar } from "./components/OperationBar";
import { LogDrawer } from "./components/LogDrawer";
import { SnapshotStatus } from "./components/SnapshotStatus";
import { useOperationEvents, useStartupRefresh } from "./lib/events";
import { useNoBrowserContextMenu } from "./lib/contextMenu";
import { useMenuCommands } from "./lib/menu";
import { useDockBadge } from "./lib/dockBadge";
import { useUpdateNotification } from "./lib/updateNotification";
import { useUiStore, type Page } from "./store/ui";

/**
 * What each page's header has on the right: its own way to look again,
 * or nothing. The pages about the sources check them again; the Unknown
 * page scans again -- only that, never two refresh buttons stacked; and
 * Settings looks at nothing. A `switch` with no default, so a page added
 * to `Page` without an answer here fails `tsc`.
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
  const { t } = useTranslation();
  const page = useUiStore((s) => s.page);
  const setPage = useUiStore((s) => s.setPage);
  const openInstalled = useUiStore((s) => s.openInstalled);
  useOperationEvents();
  useStartupRefresh();
  useMenuCommands();

  return (
    <div className="flex h-screen bg-[var(--color-content)] text-[var(--color-foreground)]">
      <UpdateWatchers />
      {/* The sidebar's Installed opens the page on everything installed,
          which is what its count counts; an Overview tile opens it on one
          source (`openInstalled`). */}
      <Sidebar page={page} onSelectPage={(p) => (p === "installed" ? openInstalled(null) : setPage(p))} />
      <div className="flex min-w-0 flex-1 flex-col">
        <main className="flex min-h-0 flex-1 flex-col">
          {/* Outside `SnapshotStatus`, so the title and Check again stay
              put whatever the page below shows -- "Loading…", a failed
              first check, an empty Mac. */}
          <PageHeader title={t(PAGE_LABEL_KEYS[page])} actions={headerActions(page)} />
          {/* The page's own box. The Installed and Updates pages size
              their lists to its height (`h-full`) and scroll inside them;
              the other pages scroll here. */}
          <div className="min-h-0 flex-1 overflow-y-auto">
            {/* Settings and Unknown are not snapshot pages: Settings never
                was, and the unknown-source scan is judged against the
                snapshot but is not part of it -- on a Mac with no source at
                all, SnapshotStatus would replace it with "Canager found
                nothing it can manage", the one case where every program on
                the machine belongs on it. */}
            {page === "settings" ? (
              <SettingsPage />
            ) : page === "unknown" ? (
              <UnknownPage />
            ) : page === "overview" ? (
              // The Overview says "Checking…" itself while the first check
              // runs; every other state is the snapshot's, as on any page.
              <SnapshotStatus showsFirstCheck>
                <OverviewPage />
              </SnapshotStatus>
            ) : (
              <SnapshotStatus>
                {page === "installed" ? <InstalledPage /> : <UpdatesPage />}
              </SnapshotStatus>
            )}
          </div>
        </main>
        {/* Its own footer, and none at all until something has run. */}
        <OperationBar />
      </div>
      <LogDrawer />
    </div>
  );
}

export default App;
