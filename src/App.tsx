import { useTranslation } from "react-i18next";
import { Sidebar } from "./components/Sidebar";
import { InstalledPage } from "./pages/InstalledPage";
import { UpdatesPage } from "./pages/UpdatesPage";
import { SettingsPage } from "./pages/SettingsPage";
import { OperationBar } from "./components/OperationBar";
import { LogDrawer } from "./components/LogDrawer";
import { useOperationEvents, useStartupRefresh } from "./lib/events";
import { useUiStore } from "./store/ui";

function App() {
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
          {page === "installed" ? (
            <InstalledPage />
          ) : page === "updates" ? (
            <UpdatesPage />
          ) : (
            <SettingsPage />
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
