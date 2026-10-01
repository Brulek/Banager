import { useTranslation } from "react-i18next";
import type { InstalledShow, ToolShow } from "../lib/families";
import { ToolbarPopupButton } from "./ui/PopupButton";

/**
 * The toolbar's 「显示」 popup on the Installed and Updates pages: 「所有工具」
 * or 「AI工具」 -- the AI coding tools whichever source installed them
 * (`facts.family`) -- and, on the Installed page alone (`twins`),
 * 「装了不止一份」: the tools another source installed a copy of too, the
 * rows that carry the 「装了两份」 word. A grey toolbar popup button, as the
 * sort beside it is, named 「显示」 for a screen reader.
 */
export function ToolShowButton(
  props:
    | { twins?: false; value: ToolShow; onChange: (value: ToolShow) => void }
    | { twins: true; value: InstalledShow; onChange: (value: InstalledShow) => void },
) {
  const { t } = useTranslation();
  const options: { value: InstalledShow; label: string }[] = [
    { value: "all", label: t("families.showAll") },
    { value: "ai", label: t("families.showAi") },
    ...(props.twins ? [{ value: "twins" as const, label: t("twinsFilter.show") }] : []),
  ];
  return (
    <ToolbarPopupButton<InstalledShow>
      label={t("families.showLabel")}
      value={props.value}
      options={options}
      // Only `twins` offers "twins", so a `ToolShow` page never gets it.
      onChange={(value) => (props.onChange as (value: InstalledShow) => void)(value)}
    />
  );
}
