import { useId } from "react";
import { useTranslation } from "react-i18next";
import type { InstallProgress } from "../lib/catalog";
import { useUiStore } from "../store/ui";
import { InfoIcon, WarningFilledIcon } from "./icons";

/**
 * How an install that ended without the tool says so, in its row, where
 * the state was: a word that opens its log -- 「未能安装」 in the red for
 * text; 「需要查看」 with ⚠︎ where Banager found it not there though
 * Homebrew said it was, 「结果未确认」 where it could not tell -- as an
 * update's row does (`UpdateProgress`).
 */
export function EndedWithout({ progress, name }: { progress: InstallProgress; name: string }) {
  const { t } = useTranslation();
  const wordId = useId();
  const setFocusedOpId = useUiStore((s) => s.setFocusedOpId);
  const setDrawerOpen = useUiStore((s) => s.setDrawerOpen);
  if (progress.kind !== "failed" && progress.kind !== "check" && progress.kind !== "unconfirmed") return null;
  const word =
    progress.kind === "failed"
      ? t("discover.progress.failed")
      : progress.kind === "check"
        ? t("updates.progress.check")
        : t("updates.progress.unconfirmed");
  return (
    <button
      type="button"
      onClick={() => {
        setFocusedOpId(progress.opId);
        setDrawerOpen(true);
      }}
      aria-label={t("updates.progress.viewLogLabel", { name })}
      aria-describedby={wordId}
      title={t("common.viewLog")}
      className={`inline-flex items-center gap-1 whitespace-nowrap rounded-sm text-small ${
        progress.kind === "failed" ? "text-danger-text" : "text-foreground"
      }`}
    >
      {progress.kind === "failed" ? null : <WarningFilledIcon size={12} className="shrink-0 text-warning" />}
      <span id={wordId}>{word}</span>
      <InfoIcon size={12} className="shrink-0" />
    </button>
  );
}

