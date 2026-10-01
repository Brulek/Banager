import { useTranslation } from "react-i18next";
import type { ToolShow } from "../lib/families";
import { ToolbarPopupButton } from "./ui/PopupButton";

/**
 * The toolbar's 「显示」 popup on the Installed and Updates pages: 「所有工具」
 * or 「AI工具」 -- the AI coding tools whichever source installed them
 * (`facts.family`). A grey toolbar popup button, as the sort beside it is,
 * named 「显示」 for a screen reader.
 */
export function ToolShowButton({ value, onChange }: { value: ToolShow; onChange: (value: ToolShow) => void }) {
  const { t } = useTranslation();
  return (
    <ToolbarPopupButton
      label={t("families.showLabel")}
      value={value}
      options={[
        { value: "all", label: t("families.showAll") },
        { value: "ai", label: t("families.showAi") },
      ]}
      onChange={onChange}
    />
  );
}
