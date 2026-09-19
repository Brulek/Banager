import { describe, expect, it, vi, beforeEach } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { useLanguageSync } from "./useLanguageSync";
import i18n from "./index";
import { queryKeys, useSettings } from "../lib/queries";
import type { Language, Settings } from "../lib/types";

function baseSettings(overrides: Partial<Settings> = {}): Settings {
  return {
    language: "System",
    show_technical_details: false,
    ignored_updates: [],
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

describe("useLanguageSync", () => {
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
});
