import { describe, expect, it, vi, beforeEach } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { menuLanguageOf, useLanguageSync } from "./useLanguageSync";
import i18n from "./index";
import { systemLanguage } from "./language";
import { queryKeys, useSettings } from "../lib/queries";
import type { Language, Settings } from "../lib/types";

function baseSettings(overrides: Partial<Settings> = {}): Settings {
  return {
    language: "System",
    show_technical_details: false,
    ignored_updates: [],
    skipped_versions: [],
    include_self_updating: false,
    auto_check: false,
    notify_updates: false,
    ...overrides,
  };
}

// Renders the language the settings query actually delivered, so a test can
// wait until the settings have *arrived* and the effect has run, instead of
// asserting the moment `invoke` happens to be called (which is before the
// query resolves and proves nothing).
function Probe() {
  useLanguageSync();
  const { data } = useSettings();
  return <span>{data?.language ?? ""}</span>;
}

beforeEach(async () => {
  vi.mocked(invoke).mockReset();
  await i18n.changeLanguage("en");
});

/** The languages the menu bar was told, in order (`set_menu_language`). */
function menuLanguagesSent(): unknown[] {
  return vi
    .mocked(invoke)
    .mock.calls.filter(([cmd]) => cmd === "set_menu_language")
    .map(([, args]) => (args as { language: unknown }).language);
}

describe("useLanguageSync", () => {
  it.each([
    ["zh-TW", "zh-Hant"], ["zh-HK", "zh-Hant"], ["zh-MO", "zh-Hant"],
    ["zh-Hant", "zh-Hant"], ["zh-Hant-TW", "zh-Hant"], ["zh-Hant-CN", "zh-Hant"],
    ["ZH_tw", "zh-Hant"], ["zh-CN", "zh-CN"], ["zh-SG", "zh-CN"],
    ["zh-Hans", "zh-CN"], ["zh-Hans-HK", "zh-CN"], ["zh-Hans-CN", "zh-CN"],
    ["zh", "zh-CN"], ["en-US", "en"], ["ja-JP", "en"], ["zha", "en"],
  ])("detects %s as %s through the registered detector", async (system, expected) => {
    expect(systemLanguage(system)).toBe(expected);
    const languages = vi.spyOn(navigator, "languages", "get").mockReturnValue([system, "en-US"]);
    try {
      await i18n.changeLanguage();
      expect(i18n.resolvedLanguage).toBe(expected);
      expect(document.documentElement.lang).toBe(expected);
    } finally {
      languages.mockRestore();
    }
  });

  it("loads a Traditional Chinese override, tells the native menu and returns to the system language", async () => {
    let language: Language = "ZhHant";
    vi.mocked(invoke).mockImplementation(async (cmd: string) =>
      cmd === "get_settings" ? baseSettings({ language }) : undefined,
    );
    const { queryClient } = renderWithProviders(<Probe />);
    await waitFor(() => expect(i18n.t("nav.settings")).toBe("設定"));
    expect(document.documentElement.lang).toBe("zh-Hant");
    await waitFor(() => expect(menuLanguagesSent()).toEqual(["en", "zh-Hant"]));
    language = "System";
    await queryClient.invalidateQueries({ queryKey: queryKeys.settings });
    await waitFor(() => expect(i18n.resolvedLanguage).toBe("en"));
    await waitFor(() => expect(menuLanguagesSent()).toEqual(["en", "zh-Hant", "en"]));
  });

  it("switches i18next to Simplified Chinese, resources included, when Settings overrides the language", async () => {
    vi.mocked(invoke).mockResolvedValue(baseSettings({ language: "ZhCn" }));

    renderWithProviders(<Probe />);

    await waitFor(() => expect(i18n.language).toBe("zh-CN"));
    // i18next sets `language` to whatever was requested even when no bundle
    // is registered for it; only a translated string proves that
    // zh-CN.json is wired into `resources` in src/i18n/index.ts.
    await waitFor(() => expect(i18n.t("nav.settings")).toBe("设置"));
  });

  it("re-runs system detection when Settings says 'System'", async () => {
    vi.mocked(invoke).mockResolvedValue(baseSettings({ language: "System" }));

    renderWithProviders(<Probe />);

    await screen.findByText("System");
    // jsdom reports navigator.language "en-US"; supportedLngs resolves it to "en".
    await waitFor(() => expect(i18n.resolvedLanguage).toBe("en"));
  });

  it("keeps <html lang> on the language in use, override and system alike", async () => {
    let language: Language = "ZhCn";
    vi.mocked(invoke).mockImplementation(async () => baseSettings({ language }));

    const { queryClient } = renderWithProviders(<Probe />);
    await waitFor(() => expect(document.documentElement.lang).toBe("zh-CN"));

    language = "System";
    await queryClient.invalidateQueries({ queryKey: queryKeys.settings });

    await screen.findByText("System");
    await waitFor(() => expect(document.documentElement.lang).toBe("en"));
  });

  it("falls back to the detected system language once the override is switched off again", async () => {
    let language: Language = "ZhCn";
    vi.mocked(invoke).mockImplementation(async () => baseSettings({ language }));

    const { queryClient } = renderWithProviders(<Probe />);
    await waitFor(() => expect(i18n.t("nav.settings")).toBe("设置"));

    language = "System";
    await queryClient.invalidateQueries({ queryKey: queryKeys.settings });

    await screen.findByText("System");
    await waitFor(() => expect(i18n.resolvedLanguage).toBe("en"));
    expect(i18n.t("nav.settings")).toBe("Settings");
  });

  it("tells the menu bar the language in use at startup, then each language it changes to", async () => {
    let language: Language = "ZhCn";
    vi.mocked(invoke).mockImplementation(async (cmd: string) =>
      cmd === "get_settings" ? baseSettings({ language }) : undefined,
    );

    const { queryClient } = renderWithProviders(<Probe />);
    // First the language i18next detected, before the settings arrive;
    // then Settings' override.
    await waitFor(() => expect(menuLanguagesSent()).toEqual(["en", "zh-CN"]));

    language = "System";
    await queryClient.invalidateQueries({ queryKey: queryKeys.settings });

    // Following the system again: jsdom's en-US.
    await waitFor(() => expect(menuLanguagesSent()).toEqual(["en", "zh-CN", "en"]));
  });

  it("tells it nothing more while the language stays as it is", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) =>
      cmd === "get_settings" ? baseSettings({ language: "En" }) : undefined,
    );

    const { queryClient } = renderWithProviders(<Probe />);
    await screen.findByText("En");
    await queryClient.invalidateQueries({ queryKey: queryKeys.settings });
    await screen.findByText("En");

    expect(menuLanguagesSent()).toEqual(["en"]);
  });

  it("leaves the page as it is when the menu bar cannot be told", async () => {
    const logged = vi.spyOn(console, "error").mockImplementation(() => {});
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "set_menu_language") throw "menu failed";
      return baseSettings({ language: "ZhCn" });
    });

    renderWithProviders(<Probe />);

    await waitFor(() => expect(i18n.language).toBe("zh-CN"));
    await waitFor(() => expect(logged).toHaveBeenCalledWith("set_menu_language failed", expect.any(Error)));
    logged.mockRestore();
  });

  it("names the menu bar's language as the window's three", () => {
    expect(menuLanguageOf("zh-CN")).toBe("zh-CN");
    expect(menuLanguageOf("zh-Hant")).toBe("zh-Hant");
    expect(menuLanguageOf("en")).toBe("en");
    // Before i18next has resolved one, English, as it falls back to.
    expect(menuLanguageOf(undefined)).toBe("en");
  });
});
