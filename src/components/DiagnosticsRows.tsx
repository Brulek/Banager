import { useId, useState } from "react";
import { useTranslation } from "react-i18next";
import { useCopyDiagnostics, useDiagnosticsStatus } from "../lib/diagnostics";
import { BUTTON } from "./ui/controls";
import { GROUP_ROW } from "./ui/group";

/**
 * Settings' 「拷贝诊断信息」, in its About group (`SettingsPage`): a row
 * named 「诊断信息」 with the button on the right -- 「已拷贝」 or
 * 「无法拷贝」 beside it for a moment after, whether the copy was asked for
 * here or from Help's item of the same name (`useDiagnosticsStatus`) --
 * and under it the checkbox that adds each source's tools to the text,
 * off whenever Settings opens: a private tap's or scope's name can say
 * where someone works, so the list goes only where it is asked for.
 * Two rows of the group they stand in; what the text holds is the group's
 * footnote.
 */
export function DiagnosticsRows() {
  const { t } = useTranslation();
  const copy = useCopyDiagnostics();
  const status = useDiagnosticsStatus((s) => s.status);
  const [includeTools, setIncludeTools] = useState(false);
  const checkboxId = useId();
  const words = status === "copied" ? t("common.copied") : status === "failed" ? t("common.copyFailed") : null;
  return (
    <>
      <div className={GROUP_ROW}>
        <span className="block min-w-0 text-body text-foreground">{t("diagnostics.label")}</span>
        <div className="flex shrink-0 items-center gap-2">
          {/* Read out as it changes, as a status is; not `role="status"`,
              which Settings keeps for why notifications are off. */}
          <span aria-live="polite" data-diagnostics-status="" className="text-small text-muted">
            {words}
          </span>
          <button
            type="button"
            data-copy-diagnostics=""
            onClick={() => copy(includeTools)}
            className={BUTTON.regular.grey}
          >
            {t("diagnostics.copy")}
          </button>
        </div>
      </div>
      <div className={GROUP_ROW}>
        <label htmlFor={checkboxId} className="flex min-w-0 items-center gap-2 text-body text-foreground">
          <input
            id={checkboxId}
            type="checkbox"
            checked={includeTools}
            onChange={(event) => setIncludeTools(event.target.checked)}
            className="h-4 w-4 shrink-0"
          />
          {t("diagnostics.includeTools")}
        </label>
      </div>
    </>
  );
}
