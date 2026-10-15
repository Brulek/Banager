import { existsSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import i18n from "../i18n";

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../..");

/**
 * The keys Tauri merges into Banager.app's Info.plist: src-tauri/Info.plist,
 * beside tauri.conf.json, where Tauri looks for it. Each value as a string,
 * or an array of strings, the only kinds the file holds.
 */
function infoPlist(): Record<string, string | string[]> {
  const xml = readFileSync(path.join(ROOT, "src-tauri/Info.plist"), "utf-8");
  const document = new DOMParser().parseFromString(xml, "application/xml");
  expect(document.getElementsByTagName("parsererror")).toHaveLength(0);
  expect(document.documentElement.tagName).toBe("plist");
  const dict = document.documentElement.children[0];
  expect(dict.tagName).toBe("dict");
  const entries: Record<string, string | string[]> = {};
  const children = [...dict.children];
  for (let index = 0; index < children.length; index += 2) {
    const [key, value] = [children[index], children[index + 1]];
    expect(key.tagName).toBe("key");
    entries[key.textContent ?? ""] =
      value.tagName === "array" ? [...value.children].map((item) => item.textContent ?? "") : (value.textContent ?? "");
  }
  return entries;
}

describe("the app bundle's Info.plist", () => {
  it("is where Tauri looks for one to merge, beside tauri.conf.json", () => {
    expect(existsSync(path.join(ROOT, "src-tauri/tauri.conf.json"))).toBe(true);
    expect(existsSync(path.join(ROOT, "src-tauri/Info.plist"))).toBe(true);
  });

  it("declares the app in English, Simplified Chinese and Traditional Chinese, so macOS's own menu items follow a Mac in Chinese", () => {
    // Without CFBundleLocalizations macOS takes the app for English only:
    // Edit's Start Dictation and Emoji & Symbols, the Window menu's tiling
    // items and the About panel stayed in English on a Mac in Chinese.
    const plist = infoPlist();
    expect(plist.CFBundleDevelopmentRegion).toBe("en");
    expect(plist.CFBundleLocalizations).toEqual(["en", "zh-Hans", "zh-Hant"]);
  });

  it("declares one localization for each language of the window, by macOS's name for it", () => {
    // i18next's zh-CN is Simplified Chinese: zh-Hans to macOS.
    const macOSName: Record<string, string> = { en: "en", "zh-CN": "zh-Hans", "zh-Hant": "zh-Hant" };
    const windowLanguages = Object.keys(i18n.options.resources ?? {});
    expect(windowLanguages.map((language) => macOSName[language])).toEqual(infoPlist().CFBundleLocalizations);
  });
});
