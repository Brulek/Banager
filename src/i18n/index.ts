import i18n from "i18next";
import LanguageDetector from "i18next-browser-languagedetector";
import { initReactI18next } from "react-i18next";
import { autospacePostProcessor, lacksTextAutospace } from "./autospace";
import en from "./en.json";
import zhCN from "./zh-CN.json";
import zhHant from "./zh-Hant.json";
import { systemLanguage } from "./language";

// <html lang> follows the language in use, so VoiceOver and the system's
// text services read Chinese as Chinese (index.html ships lang="en").
// Registered before `init` so the first detected language counts too.
i18n.on("languageChanged", (lng) => {
  document.documentElement.lang = i18n.resolvedLanguage ?? lng;
});

void i18n
  .use(LanguageDetector)
  .use(initReactI18next)
  .use(autospacePostProcessor)
  .init({
    resources: {
      en: { translation: en },
      "zh-CN": { translation: zhCN },
      "zh-Hant": { translation: zhHant },
    },
    fallbackLng: "en",
    supportedLngs: ["en", "zh-CN", "zh-Hant"],
    detection: { order: ["navigator"], caches: [], convertDetectedLanguage: systemLanguage },
    interpolation: { escapeValue: false },
    // The Chinese strings put no space between Chinese and Latin; the web
    // view draws the gap (`text-autospace` in index.css). One too old to
    // draw it gets a narrow space typed in instead (`./autospace.ts`).
    postProcess: lacksTextAutospace() ? [autospacePostProcessor.name] : false,
  });

export default i18n;
