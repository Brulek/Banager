import { useTranslation } from "react-i18next";
import { useLanguageSync } from "./i18n/useLanguageSync";
import { Sidebar } from "./components/Sidebar";
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
    <div className="flex h-screen flex-col bg-[var(--color-background)] text-[var(--color-foreground)]">
      <div className="flex flex-1 overflow-hidden">
        <Sidebar page={page} onSelectPage={setPage} />
        <main className="flex-1 overflow-y-auto">
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
          ) : (
            <SnapshotStatus>
              {page === "installed" ? <InstalledPage /> : <UpdatesPage />}
            </SnapshotStatus>
          )}
        </main>
      </div>
      <footer
        aria-label={t("app.operationBarRegion")}
        className="h-12 shrink-0 border-t border-[var(--color-border)]"
      >
        <OperationBar />
      </footer>
      <LogDrawer />
    </div>
  );
}

export default App;
