import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { setMenuLanguage, type MenuLanguage } from "../lib/api";
import { useSettings } from "../lib/queries";
import i18n from "./index";

/** The menu bar's language for the one i18next resolved: the window's two. */
export function menuLanguageOf(resolved: string | undefined): MenuLanguage {
  return resolved === "zh-CN" ? "zh-CN" : "en";
}

/**
 * Keeps i18next's active language in sync with the user's Settings
 * override (spec §9: the language follows the system unless overridden in
 * Settings). `"System"` re-runs the detector `src/i18n/index.ts` registered;
 * anything else forces the exact language the user chose.
 *
 * And the menu bar in the language in use: Rust builds it
 * (src-tauri/src/menu.rs) in the one this hook names (`setMenuLanguage`)
 * -- at startup, the language i18next detected, before the settings have
 * arrived, and then each language it changes to.
 */
export function useLanguageSync(): void {
  const { data: settings } = useSettings();
  // `useTranslation` renders again at every change of language, so this is
  // always the language in use.
  const menuLanguage = menuLanguageOf(useTranslation().i18n.resolvedLanguage);

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

  useEffect(() => {
    // Rust builds nothing again for the language the menu bar is in
    // already. A failure leaves the menu bar in the language it had, and
    // nothing on the page depends on it.
    setMenuLanguage(menuLanguage).catch((e: unknown) => {
      console.error("set_menu_language failed", e);
    });
  }, [menuLanguage]);
}
