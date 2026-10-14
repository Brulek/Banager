import type { Language } from "../lib/types";

/** The interface, native menu and descriptions use these locale ids. */
export type InterfaceLanguage = "en" | "zh-CN" | "zh-Hant";

/** Match macOS's first preferred language; an explicit script wins over region. */
export function systemLanguage(language: string): InterfaceLanguage {
  const [base, ...parts] = language.toLowerCase().replace(/_/g, "-").split("-");
  if (base !== "zh") return "en";
  if (parts.includes("hant")) return "zh-Hant";
  if (parts.includes("hans")) return "zh-CN";
  return parts.some((part) => ["tw", "hk", "mo"].includes(part)) ? "zh-Hant" : "zh-CN";
}

/**
 * The locale dates and times are written in (`src/lib/shortDate.ts`):
 * null for the Mac's own formats -- its region's order and its clock, as
 * the web view's `Intl` gives them with no locale named -- or one of the
 * window's languages. The Mac's own in System (and while the settings
 * load) where the Mac's first language is the window's: English
 * (United Kingdom) reads "13 Oct" and 14:30 in English words. A Mac in a
 * language Banager has no words in keeps the window's, so that no French
 * month name sits among English words; a language chosen in Settings is
 * that language's.
 */
export function formattingLocaleOf(
  setting: Language | undefined,
  windowLanguage: string,
  macLanguage: string,
): string | null {
  if (setting !== undefined && setting !== "System") return windowLanguage;
  const base = (language: string) => language.toLowerCase().split(/[-_]/)[0];
  return base(macLanguage) === base(windowLanguage) ? null : windowLanguage;
}
