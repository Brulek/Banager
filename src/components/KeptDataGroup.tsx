import { useTranslation } from "react-i18next";
import { useCopyCommand } from "../lib/clipboard";
import { keptDataOf } from "../lib/keptData";
import { sizeText } from "../lib/sizes";
import type { Warning } from "../lib/types";
import { KEPT_DATA_KEYS } from "../lib/warnings";
import { SheetSection } from "./SheetParts";
import { BUTTON } from "./ui/controls";
import { SMALL_WRAPPING } from "./ui/group";

/**
 * The uninstall confirmation's 「卸载后会保留」 group (advantages round,
 * item 6): each folder or file a tool keeps its own data in that the
 * uninstall leaves where it is -- `~/.claude`, Ollama's models -- with
 * about how much it takes where that is known, what it holds in a few
 * plain words, and Copy Path, which copies the path as shown, `~` and all.
 * These are hidden folders, so Show in Finder would show nothing; and
 * there is no button, menu or command here that deletes one. Nothing when
 * the plan names none.
 */
export function KeptDataGroup({ warnings }: { warnings: readonly Warning[] }) {
  const { t } = useTranslation();
  const { status, copy } = useCopyCommand();
  const items = keptDataOf(warnings);
  if (items.length === 0) return null;
  return (
    <SheetSection title={t("keepsData.title")}>
      <ul className="flex flex-col gap-1.5">
        {items.map((item) => (
          <li key={item.path} data-kept-data="" className="flex items-start gap-2">
            <div className="min-w-0 flex-1">
              <p className={`break-words text-foreground ${SMALL_WRAPPING}`}>
                <span data-kept-path="">{item.path}</span>
                {item.size !== null ? (
                  <span data-kept-size="" className="text-muted">
                    {" · "}
                    {sizeText(t, item.size)}
                  </span>
                ) : null}
              </p>
              <p className={`break-words text-muted ${SMALL_WRAPPING}`}>{t(KEPT_DATA_KEYS[item.what])}</p>
            </div>
            <button
              type="button"
              aria-label={t("keepsData.copyPathLabel", { path: item.path })}
              onClick={() => copy(item.path)}
              className={BUTTON.small.grey}
            >
              {t("keepsData.copyPath")}
            </button>
          </li>
        ))}
      </ul>
      <p role="status" className="mt-1 text-small text-muted empty:hidden">
        {status === "copied" ? t("common.copied") : status === "failed" ? t("common.copyFailed") : null}
      </p>
    </SheetSection>
  );
}
