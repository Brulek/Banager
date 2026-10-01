import { useTranslation } from "react-i18next";
import { openToolSetupSheet } from "../lib/toolSetupCheck";
import { BUTTON } from "./ui/controls";
import { GROUP_ROW_TWO_LINES, SMALL_WRAPPING } from "./ui/group";

/**
 * The Overview's way to 「检查工具环境」, so that it is found where a
 * person looks first and not only in the Help menu and at the end of
 * Settings' 「关于」: 「工具环境」 and what the check looks at, with a grey
 * 「检查…」 that opens the sheet (`openToolSetupSheet`), as Settings' row
 * does. A row of a group -- the caller's -- not a group of its own.
 */
export function ToolSetupRow() {
  const { t } = useTranslation();
  return (
    <div data-overview-tool-setup="" className={GROUP_ROW_TWO_LINES}>
      <div className="min-w-0">
        <p className="text-body text-foreground">{t("setupCheck.label")}</p>
        <p className={`mt-0.5 ${SMALL_WRAPPING} text-muted`}>{t("overviewMore.toolSetupLine")}</p>
      </div>
      <button
        type="button"
        aria-label={t("setupCheck.menu")}
        onClick={openToolSetupSheet}
        className={`${BUTTON.regular.grey} shrink-0`}
      >
        {t("setupCheck.open")}
      </button>
    </div>
  );
}
