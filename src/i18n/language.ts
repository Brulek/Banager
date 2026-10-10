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
