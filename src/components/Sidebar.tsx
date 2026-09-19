import { useTranslation } from "react-i18next";

export type SidebarPage = "installed" | "updates" | "settings";

interface SidebarProps {
  page: SidebarPage;
  onSelectPage: (page: SidebarPage) => void;
}

const PAGES: SidebarPage[] = ["installed", "updates", "settings"];

export function Sidebar({ page, onSelectPage }: SidebarProps) {
  const { t } = useTranslation();

  return (
    <nav
      aria-label={t("nav.label")}
      className="flex w-56 shrink-0 flex-col gap-1 border-r border-[var(--color-border)] bg-[var(--color-sidebar-bg)] p-2"
    >
      {PAGES.map((p) => (
        <button
          key={p}
          type="button"
          aria-current={page === p ? "page" : undefined}
          onClick={() => onSelectPage(p)}
          className={
            page === p
              ? "rounded-md bg-[var(--color-accent)] px-3 py-2 text-left text-sm font-medium text-[var(--color-accent-foreground)]"
              : "rounded-md px-3 py-2 text-left text-sm font-medium text-[var(--color-foreground)] hover:bg-[var(--color-hover)]"
          }
        >
          {t(`nav.${p}`)}
        </button>
      ))}
    </nav>
  );
}
