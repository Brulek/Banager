import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, screen, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { SettingsPage } from "./SettingsPage";
import i18n from "../i18n";
import { useToolSetupSheet } from "../lib/toolSetupCheck";
import type { Settings } from "../lib/types";
import tauriConfig from "../../src-tauri/tauri.conf.json";

/**
 * I4 (decisions round, 2026-10-06): Settings' 「工具环境」 sits in a group of
 * its own, 「诊断」, with 「拷贝诊断信息」 -- both are about how this Mac is
 * set up -- and 「关于」 keeps the app's version and its icon credits.
 */

const settings: Settings = {
  language: "System",
  show_technical_details: false,
  ignored_updates: [],
  skipped_versions: [],
  include_self_updating: false,
  auto_check: false,
  notify_updates: false,
  auto_check_every: "Day",
};

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockImplementation(async (cmd: string) => (cmd === "get_settings" ? settings : undefined));
});

function texts(region: HTMLElement): string[] {
  return within(region).getAllByText(/./).map((node) => node.textContent ?? "");
}

describe("Settings' 诊断 group", () => {
  it("comes before About, last but one", async () => {
    renderWithProviders(<SettingsPage />);
    await screen.findByRole("region", { name: "Diagnostics" });
    expect(screen.getAllByRole("heading", { level: 2 }).map((heading) => heading.textContent)).toEqual([
      "General",
      "Updates",
      "Skipped versions",
      "Tools with reminders paused",
      "Tools with reminders off",
      "Diagnostics",
      "About",
    ]);
  });

  it("holds Tool setup, then the diagnostic info, with what the text holds under it", async () => {
    renderWithProviders(<SettingsPage />);
    const diagnostics = await screen.findByRole("region", { name: "Diagnostics" });
    expect(texts(diagnostics)).toEqual([
      "Diagnostics",
      "Tool setup",
      "Check Tool Setup…",
      "Diagnostic info",
      "Include the list of tools",
      "Copy Diagnostic Info",
      "Paste it to whoever is helping you. It lists your tools only when the checkbox is selected, and includes the error details of any source that isn't responding. ",
      "responding. ",
    ]);
    fireEvent.click(within(diagnostics).getByRole("button", { name: "Check Tool Setup…" }));
    expect(useToolSetupSheet.getState().open).toBe(true);
    useToolSetupSheet.setState({ open: false });
  });

  it("leaves About the version and the icon credits", async () => {
    renderWithProviders(<SettingsPage />);
    const about = await screen.findByRole("region", { name: "About" });
    expect(texts(about)).toEqual(["About", "Version", tauriConfig.version, "Icon credits", "View Icon Credits…"]);
    expect(within(about).queryByRole("button", { name: "Copy Diagnostic Info" })).toBeNull();
    expect(within(about).queryByRole("button", { name: "Check Tool Setup…" })).toBeNull();
  });

  it("is called 诊断, and 診斷 in Traditional Chinese", async () => {
    await i18n.changeLanguage("zh-CN");
    try {
      renderWithProviders(<SettingsPage />);
      const diagnostics = await screen.findByRole("region", { name: "诊断" });
      expect(within(diagnostics).getByRole("button", { name: "拷贝诊断信息" })).toBeInTheDocument();
      await i18n.changeLanguage("zh-Hant");
      expect(await screen.findByRole("region", { name: "診斷" })).toBeInTheDocument();
    } finally {
      await i18n.changeLanguage("en");
    }
  });
});
