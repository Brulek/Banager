import { useId, useMemo } from "react";
import { useTranslation } from "react-i18next";
import { useSystemFacts } from "../lib/diagnostics";
import { isStartupSnapshot } from "../lib/events";
import { useSnapshot } from "../lib/queries";
import { openToolSetupSheet, setupAttention } from "../lib/toolSetupCheck";
import { WarningFilledIcon } from "./icons";
import { BUTTON } from "./ui/controls";
import { GROUP_ROW_TWO_LINES, SMALL_WRAPPING } from "./ui/group";

/**
 * The Overview's way to 「检查工具环境」, so that it is found where a
 * person looks first and not only in the Help menu and in Settings'
 * 「诊断」: 「工具环境」 and what the check looks at, with a grey 「查看…」
 * that opens the sheet (`openToolSetupSheet`), as Settings' row does. A
 * row of a group -- the caller's -- not a group of its own.
 *
 * Where the sheet would open on lines with the ⚠︎, the line under the
 * title says how many, in the words of the sheet's own summary, with the
 * orange ⚠︎ a status word has (`StatusChip`): 「2项需要查看」 (decision I4),
 * counted without building the sheet (`setupAttention`). Otherwise -- and
 * while the first check since launch has not finished, when the sheet
 * gives no count either -- what the check looks at. The button's
 * description is that line.
 */
export function ToolSetupRow() {
  const { t } = useTranslation();
  const lineId = useId();
  const { data: snapshot } = useSnapshot();
  const { data: facts } = useSystemFacts();
  const answered = snapshot !== undefined && !isStartupSnapshot(snapshot);
  const attention = useMemo(
    () => (answered ? setupAttention({ snapshot, pending: false, facts }) : 0),
    [answered, snapshot, facts],
  );
  return (
    <div data-overview-tool-setup="" className={GROUP_ROW_TWO_LINES}>
      <div className="min-w-0">
        <p className="text-body text-foreground">{t("setupCheck.label")}</p>
        {attention > 0 ? (
          <p className={`mt-0.5 flex items-center gap-1 ${SMALL_WRAPPING} text-muted`}>
            <WarningFilledIcon size={12} className="shrink-0 text-warning" />
            <span id={lineId} data-setup-row-line="">
              {t("reviewFixes.setupAttention", { count: attention })}
            </span>
          </p>
        ) : (
          <p id={lineId} data-setup-row-line="" className={`mt-0.5 ${SMALL_WRAPPING} text-muted`}>
            {t("overviewMore.toolSetupLine")}
          </p>
        )}
      </div>
      <button
        type="button"
        aria-label={t("setupCheck.menu")}
        aria-describedby={lineId}
        onClick={openToolSetupSheet}
        className={`${BUTTON.regular.grey} shrink-0`}
      >
        {t("setupCheck.open")}
      </button>
    </div>
  );
}
