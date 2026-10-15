import { useTranslation } from "react-i18next";
import { useSettings } from "../lib/queries";
import { settingsSaveSentence } from "../lib/sources";
import type { Settings } from "../lib/types";
import { useWelcomeAgain } from "../lib/welcome";
import { useUiStore, type Page } from "../store/ui";
import { SourceNoticeLine } from "./SourceNotice";

/**
 * Whether these are the defaults standing in for a settings file that could
 * not be read when Banager opened (`load_status`, r24 R74-3): for the rest
 * of the launch nothing is saved over it (`AppState::set_settings`), so the
 * Settings page changes none of them and the welcome sheet neither opens by
 * itself nor saves.
 */
export function settingsUnreadable(settings: Settings | undefined): boolean {
  return settings?.load_status === "unreadable";
}

/**
 * The window's lines about its settings, over the page and outside
 * `SnapshotStatus` (src/App.tsx), so they are said whatever the page below
 * shows. Settings that could not be read, on every page but Settings,
 * which says it in full (`SettingsPage`): the defaults standing in for them
 * turn the automatic check off and list hidden updates again, which every
 * page shows. Its button opens Settings, as ⌘, does, by that shortcut's
 * name. And a welcome sheet whose close was not saved (`WelcomeSheet`),
 * until a later close is: the sheet has gone by the time the save fails.
 */
export function SettingsNotice({ page }: { page: Page }) {
  const { t } = useTranslation();
  const { data: settings } = useSettings();
  const welcomeSaveError = useWelcomeAgain((state) => state.saveError);
  const openPage = useUiStore((state) => state.openPage);
  const unreadable = settingsUnreadable(settings) && page !== "settings";
  if (!unreadable && welcomeSaveError === null) return null;
  const title = t("persistence.settingsUnreadable");
  return (
    <div data-settings-notice="" role="status" className="px-5 pb-1">
      {unreadable ? (
        <SourceNoticeLine
          variant="warning"
          grid="checkbox"
          title={title}
          description={t("persistence.settingsLoadEffects")}
          details={<p className="mt-1">{t("persistence.settingsRecovery")}</p>}
          detailsAriaLabel={t("common.detailsLabel", { title })}
          action={{ label: t("shortcuts.rows.settings"), onClick: () => openPage("settings") }}
        />
      ) : null}
      {welcomeSaveError !== null ? (
        <SourceNoticeLine
          variant="warning"
          grid="checkbox"
          title={settingsSaveSentence(
            t,
            "welcome.saveError",
            welcomeSaveError,
            settings?.show_technical_details ?? false,
          )}
          detailsAriaLabel=""
        />
      ) : null}
    </div>
  );
}
