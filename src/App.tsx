import { useTranslation } from "react-i18next";
import { useLanguageSync } from "./i18n/useLanguageSync";
import { PAGE_LABEL_KEYS, Sidebar } from "./components/Sidebar";
import { PageHeader } from "./components/PageHeader";
import { OverviewPage } from "./pages/OverviewPage";
import { InstalledPage } from "./pages/InstalledPage";
import { UpdatesPage } from "./pages/UpdatesPage";
import { UnknownPage } from "./pages/UnknownPage";
import { SettingsPage } from "./pages/SettingsPage";
import { OperationBar } from "./components/OperationBar";
import { LogDrawer } from "./components/LogDrawer";
import { SnapshotStatus } from "./components/SnapshotStatus";
import { useOperationEvents, useStartupRefresh } from "./lib/events";
import { useUiStore } from "./store/ui";

function App() {
  useLanguageSync();
  const { t } = useTranslation();
  const page = useUiStore((s) => s.page);
  const setPage = useUiStore((s) => s.setPage);
  useOperationEvents();
  useStartupRefresh();

  return (
    <div className="flex h-screen bg-[var(--color-content)] text-[var(--color-foreground)]">
      <Sidebar page={page} onSelectPage={setPage} />
      <div className="flex min-w-0 flex-1 flex-col">
        <main className="flex min-h-0 flex-1 flex-col">
          {/* Outside `SnapshotStatus`, so the title and Check again stay
              put whatever the page below shows -- "Loading…", a failed
              first check, an empty Mac. */}
          <PageHeader title={t(PAGE_LABEL_KEYS[page])} />
          {/* The page's own box. The Installed and Updates pages size
              their lists to its height (`h-full`) and scroll inside them;
              the other pages scroll here. */}
          <div className="min-h-0 flex-1 overflow-y-auto">
            {/* Settings and Unknown are not snapshot pages: Settings never
                was, and the unknown-source scan is judged against the
                snapshot but is not part of it -- on a Mac with no source at
                all, SnapshotStatus would replace it with "Nothing for
                Canager to manage yet", the one case where every program on
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
        <footer
          aria-label={t("app.operationBarRegion")}
          className="h-12 shrink-0 border-t border-[var(--color-border)]"
        >
          <OperationBar />
        </footer>
      </div>
      <LogDrawer />
    </div>
  );
}

export default App;
