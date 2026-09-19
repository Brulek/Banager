import { useEffect } from "react";
import { useSettings } from "../lib/queries";
import i18n from "./index";

/**
 * Keeps i18next's active language in sync with the user's Settings
 * override (spec §9: the language follows the system unless overridden in
 * Settings). `"System"` re-runs the detector `src/i18n/index.ts` registered;
 * anything else forces the exact language the user chose.
 */
export function useLanguageSync(): void {
  const { data: settings } = useSettings();

  useEffect(() => {
    if (!settings) return;
    if (settings.language === "System") {
      // No argument: i18next re-runs `i18next-browser-languagedetector`.
      // This is what makes "简体中文 → System" take effect immediately
      // instead of at the next launch; `caches: []` in index.ts guarantees
      // the detector has not remembered the previous override.
      void i18n.changeLanguage();
      return;
    }
    const target = settings.language === "ZhCn" ? "zh-CN" : "en";
    if (i18n.language !== target) {
      void i18n.changeLanguage(target);
    }
  }, [settings]);
}
