import { useTranslation } from "react-i18next";
import { keptDataOf } from "../lib/keptData";
import { sizeText } from "../lib/sizes";
import type { Warning } from "../lib/types";
import { KEPT_DATA_KEYS } from "../lib/warnings";
import { CopyButton } from "./CopyButton";
import { TextWithInfo } from "./InfoDetail";
import { SheetSection } from "./SheetParts";
import { SMALL_WRAPPING } from "./ui/group";

/**
 * The uninstall confirmation's 「卸载后会保留」 group (advantages round,
 * item 6): each folder or file a tool keeps its own data in that the
 * uninstall leaves where it is -- `~/.claude`, Ollama's models -- with
 * about how much it takes where that is known, what it holds in a few
 * plain words, and Copy Path, which copies the path as shown, `~` and all,
 * and says 「已拷贝」 beside itself (`CopyButton`, as the details' copy
 * buttons do). These are hidden folders, so Show in Finder would show nothing; and
 * there is no button, menu or command here that deletes one. Nothing when
 * the plan names none.
 */
export function KeptDataGroup({ warnings }: { warnings: readonly Warning[] }) {
  const { t } = useTranslation();
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
                    {item.leftOut.length === 0 ? (
                      sizeText(t, item.size)
                    ) : (
                      // What the size does not count: another copy's program
                      // inside the folder (Codex's own install in ~/.codex).
                      <TextWithInfo
                        text={sizeText(t, item.size)}
                        label={t("clarity.keptLeftOutLabel", { path: item.path })}
                      >
                        {item.leftOut.map((path) => t("clarity.keptLeftOut", { path })).join(" ")}
                      </TextWithInfo>
                    )}
                  </span>
                ) : null}
              </p>
              <p className={`break-words text-muted ${SMALL_WRAPPING}`}>{t(KEPT_DATA_KEYS[item.what])}</p>
            </div>
            <CopyButton
              text={item.path}
              label={t("keepsData.copyPath")}
              ariaLabel={t("keepsData.copyPathLabel", { path: item.path })}
            />
          </li>
        ))}
      </ul>
    </SheetSection>
  );
}
