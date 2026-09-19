import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Sidebar, type SidebarPage } from "./components/Sidebar";

function App() {
  const { t } = useTranslation();
  const [page, setPage] = useState<SidebarPage>("installed");

  return (
    <div className="flex h-screen flex-col bg-[var(--color-background)] text-[var(--color-foreground)]">
      <div className="flex flex-1 overflow-hidden">
        <Sidebar page={page} onSelectPage={setPage} />
        <main className="flex-1 overflow-y-auto p-6">
          <h1 className="text-lg font-semibold">{t(`nav.${page}`)}</h1>
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
