import { useTranslation } from "react-i18next";
import { useSettings } from "../lib/queries";
import type { FormattingLocale } from "../lib/shortDate";
import { formattingLocaleOf } from "./language";

/**
 * The locale for the window's dates and times (`formattingLocaleOf`): the
 * Mac's own formats in System, the chosen language's otherwise. Renders
 * again when the setting changes, even where the window's words stay
 * the same (English → System).
 */
export function useFormattingLocale(): FormattingLocale {
  const { data: settings } = useSettings();
  const { i18n } = useTranslation();
  const macLanguage = navigator.languages?.[0] ?? navigator.language;
  return formattingLocaleOf(settings?.language, i18n.resolvedLanguage ?? i18n.language, macLanguage);
}
