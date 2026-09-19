import { useTranslation } from "react-i18next";
import { Sidebar } from "./components/Sidebar";
import { InstalledPage } from "./pages/InstalledPage";
import { useUiStore } from "./store/ui";

function App() {
  const { t } = useTranslation();
  const page = useUiStore((s) => s.page);
  const setPage = useUiStore((s) => s.setPage);

  return (
    <div className="flex h-screen flex-col bg-[var(--color-background)] text-[var(--color-foreground)]">
      <div className="flex flex-1 overflow-hidden">
        <Sidebar page={page} onSelectPage={setPage} />
        <main className="flex-1 overflow-y-auto">
          {page === "installed" ? (
            <InstalledPage />
          ) : (
            <h1 className="p-6 text-lg font-semibold">{t(`nav.${page}`)}</h1>
          )}
        </main>
      </div>
      <footer
        aria-label={t("app.operationBarRegion")}
        className="h-12 shrink-0 border-t border-[var(--color-border)]"
      />
    </div>
  );
}

export default App;
