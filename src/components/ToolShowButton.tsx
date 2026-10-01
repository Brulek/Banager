import { useTranslation } from "react-i18next";
import type { DiscoverCounts, InstalledShow, ToolShow } from "../lib/families";
import { ToolbarPopupButton } from "./ui/PopupButton";

/**
 * The toolbar's 「显示」 popup on the Installed and Updates pages: 「所有工具」
 * or 「AI工具」 -- the AI coding tools whichever source installed them
 * (`facts.family`) -- and, on the Installed page alone (`twins`),
 * 「装了不止一份」: the tools another source installed a copy of too, the
 * rows that carry the 「装了两份」 word; then 「终端里找不到」 and
 * 「Homebrew已停用或弃用」 and 「保留了其他版本」 (`DiscoverShow`), each with how many it shows
 * while there are some (`counts`): 「终端里找不到（2）」. With none, the
 * choice stays, without a number, and the list it shows says none was
 * found -- a menu whose items come and go is harder to learn, and the
 * Mac's own menu, which this is, has no way to grey one out here. A grey
 * toolbar popup button, as the sort beside it is, named 「显示」 for a
 * screen reader.
 */
export function ToolShowButton(
  props:
    | { twins?: false; value: ToolShow; onChange: (value: ToolShow) => void }
    | { twins: true; value: InstalledShow; onChange: (value: InstalledShow) => void; counts?: DiscoverCounts },
) {
  const { t } = useTranslation();
  const counted = (labelKey: string, countedKey: string, count: number | undefined) =>
    count === undefined || count === 0 ? t(labelKey) : t(countedKey, { number: count });
  const counts = props.twins ? props.counts : undefined;
  const options: { value: InstalledShow; label: string }[] = [
    { value: "all", label: t("families.showAll") },
    { value: "ai", label: t("families.showAi") },
    ...(props.twins
      ? [
          { value: "twins" as const, label: t("twinsFilter.show") },
          {
            value: "notOnPath" as const,
            label: counted("families.showNotOnPath", "families.showNotOnPathCount", counts?.notOnPath),
          },
          {
            value: "brewRetired" as const,
            label: counted("families.showBrewRetired", "families.showBrewRetiredCount", counts?.brewRetired),
          },
          {
            value: "otherVersions" as const,
            label: counted("otherVersionsShow.show", "otherVersionsShow.showCount", counts?.otherVersions),
          },
        ]
      : []),
  ];
  return (
    <ToolbarPopupButton<InstalledShow>
      label={t("families.showLabel")}
      value={props.value}
      options={options}
      // Only `twins` offers the Installed page's own, so a `ToolShow` page never gets them.
      onChange={(value) => (props.onChange as (value: InstalledShow) => void)(value)}
    />
  );
}
