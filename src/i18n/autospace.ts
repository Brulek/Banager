import type { PostProcessorModule } from "i18next";

/**
 * U+2006 SIX-PER-EM SPACE: the gap put between Chinese and a Latin letter
 * or digit where the web view cannot draw one itself. AppKit draws 1/8 em
 * there, and so does `text-autospace: normal` (index.css); 1/6 em is the
 * nearest fixed-width space Unicode has.
 */
export const AUTOSPACE = " ";

const HAN_THEN_LATIN = /(\p{Script=Han})([A-Za-z0-9])/gu;
const LATIN_THEN_HAN = /([A-Za-z0-9])(\p{Script=Han})/gu;

/**
 * `text` with `AUTOSPACE` wherever a Chinese character and a Latin letter
 * or digit touch, in either order: 「有12个工具」 becomes 「有 12 个工具」
 * with two narrow gaps. Nothing else changes -- not the space between two
 * Latin words, not a gap that is already there, not Chinese punctuation
 * (「，」 is not a Chinese character here, as in CSS).
 */
export function autospace(text: string): string {
  return text.replace(HAN_THEN_LATIN, `$1${AUTOSPACE}$2`).replace(LATIN_THEN_HAN, `$1${AUTOSPACE}$2`);
}

/**
 * Whether the web view cannot space Chinese and Latin itself: WebKit
 * before Safari 18.4 (macOS 13.3–15.3) ignores `text-autospace`, and the
 * zh-CN strings are written with no space there, as macOS's own are
 * (the polish-3 spec, R7). Where `CSS.supports` is not there to ask, the
 * answer is no, and nothing is inserted. jsdom answers that it does
 * support it, so the tests see the strings as written.
 */
export function lacksTextAutospace(): boolean {
  return typeof CSS !== "undefined" && typeof CSS.supports === "function" && !CSS.supports("text-autospace", "normal");
}

/**
 * The i18next post-processor that applies `autospace` to what a key
 * translates to in Chinese, and leaves every other language alone. It
 * runs only where `index.ts` turns it on, which is only where
 * `lacksTextAutospace` says so.
 */
export const autospacePostProcessor: PostProcessorModule = {
  type: "postProcessor",
  name: "autospace",
  process(value, _key, options, translator) {
    const lng = (options as { lng?: unknown }).lng ?? (translator as { language?: unknown } | undefined)?.language;
    return typeof lng === "string" && lng.startsWith("zh") ? autospace(value) : value;
  },
};
