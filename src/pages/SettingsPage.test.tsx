import { describe, expect, it, vi, beforeEach } from "vitest";
import { screen, waitFor, fireEvent, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { SettingsPage } from "./SettingsPage";
import zhCN from "../i18n/zh-CN.json";
import type { ArtifactKey, Settings } from "../lib/types";

const jqKey: ArtifactKey = { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "jq" };
const glibKey: ArtifactKey = { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "glib" };
const onyxKey: ArtifactKey = { instance_id: "brew:/opt/homebrew", kind: "Cask", name: "onyx" };
const qwenKey: ArtifactKey = { instance_id: "ollama:127.0.0.1:11434", kind: "Model", name: "qwen3:8b" };

// The saved settings of the last `set_settings` call.
function lastSaved(): Settings {
  const saves = vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "set_settings");
  expect(saves.length).toBeGreaterThan(0);
  return (saves[saves.length - 1][1] as { settings: Settings }).settings;
}

function baseSettings(overrides: Partial<Settings> = {}): Settings {
  return {
    language: "System",
    show_technical_details: false,
    ignored_updates: [],
    skipped_versions: [],
    include_self_updating: false,
    ...overrides,
  };
}

beforeEach(() => {
  vi.mocked(invoke).mockReset();
});

describe("SettingsPage", () => {
  it("renders the settings loaded from the backend", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") {
        return baseSettings({
          show_technical_details: true,
          ignored_updates: [
            { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "jq" },
          ],
        });
      }
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    expect(
      await screen.findByRole("switch", { name: "Show technical details" }),
    ).toBeChecked();
    expect(screen.getByRole("radio", { name: "System" })).toHaveAttribute(
      "aria-checked",
      "true",
    );
    expect(screen.getByText("jq")).toBeInTheDocument();
  });

  it("optimistically applies a toggle and rolls it back when the save fails", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return baseSettings();
      if (cmd === "set_settings") throw new Error("disk full");
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const toggle = await screen.findByRole("switch", { name: "Show technical details" });
    expect(toggle).not.toBeChecked();

    fireEvent.click(toggle);
    expect(toggle).toBeChecked();

    // One sentence: what went wrong, and that the old setting is back.
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent("Couldn't save: disk full. Your previous setting is back."),
    );
    await waitFor(() => expect(toggle).not.toBeChecked());
  });

  it("removes an item from the never-remind list and saves the shorter list", async () => {
    // `args?: unknown`, as in Tasks 11/12/14: `invoke`'s second parameter is
    // `InvokeArgs` (a union that includes `ArrayBuffer`), and under
    // `strictFunctionTypes` a `Record<string, unknown>` parameter does not
    // accept it — vitest would pass, `pnpm build`'s tsc would not.
    vi.mocked(invoke).mockImplementation(async (cmd: string, _args?: unknown) => {
      if (cmd === "get_settings") {
        return baseSettings({
          ignored_updates: [jqKey],
          skipped_versions: [{ key: glibKey, version: "2.90.0" }],
        });
      }
      if (cmd === "set_settings") return undefined;
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const remindButton = await screen.findByRole("button", { name: "Remind me again about jq" });
    fireEvent.click(remindButton);

    await waitFor(() => expect(screen.getByText("No reminders turned off")).toBeInTheDocument());
    // The optimistic draft shows the empty list before the save resolves, so
    // the line above alone cannot tell a correct payload from a wrong one.
    expect(lastSaved().ignored_updates).toEqual([]);
    expect(lastSaved().skipped_versions).toEqual([{ key: glibKey, version: "2.90.0" }]);
  });

  it("lists skipped versions and the software never to be reminded about separately, under their own headings", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") {
        return baseSettings({
          ignored_updates: [jqKey],
          skipped_versions: [{ key: glibKey, version: "2.90.0" }],
        });
      }
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const skipped = await screen.findByRole("region", { name: "Skipped versions" });
    const never = screen.getByRole("region", { name: "Never remind me about" });
    // A skip names the version it hides; that version is what the entry is.
    expect(within(skipped).getByText("glib")).toBeInTheDocument();
    expect(within(skipped).getByText("2.90.0")).toBeInTheDocument();
    expect(within(skipped).getByRole("button", { name: "Stop skipping 2.90.0 of glib" })).toHaveTextContent(
      "Stop skipping",
    );
    expect(within(skipped).queryByText("jq")).toBeNull();
    expect(within(never).getByText("jq")).toBeInTheDocument();
    expect(within(never).getByRole("button", { name: "Remind me again about jq" })).toHaveTextContent(
      "Remind me again",
    );
    expect(within(never).queryByText("glib")).toBeNull();
  });

  it("says each list is empty on its own", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return baseSettings();
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const skipped = await screen.findByRole("region", { name: "Skipped versions" });
    const never = screen.getByRole("region", { name: "Never remind me about" });
    expect(within(skipped).getByText("No skipped versions")).toBeInTheDocument();
    expect(within(never).getByText("No reminders turned off")).toBeInTheDocument();
  });

  it("removes one skipped version and saves the shorter list, leaving every other entry alone", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string, _args?: unknown) => {
      if (cmd === "get_settings") {
        return baseSettings({
          ignored_updates: [jqKey],
          skipped_versions: [
            { key: glibKey, version: "2.90.0" },
            { key: onyxKey, version: "5.1.0" },
          ],
        });
      }
      if (cmd === "set_settings") return undefined;
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    fireEvent.click(
      await screen.findByRole("button", { name: "Stop skipping 2.90.0 of glib" }),
    );

    await waitFor(() => expect(screen.queryByText("glib")).not.toBeInTheDocument());
    expect(lastSaved().skipped_versions).toEqual([{ key: onyxKey, version: "5.1.0" }]);
    expect(lastSaved().ignored_updates).toEqual([jqKey]);
    expect(screen.getByText("onyx")).toBeInTheDocument();
  });

  it("names a model's skipped build without printing its digest", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") {
        return baseSettings({
          skipped_versions: [
            {
              key: qwenKey,
              version: "sha256:9f1c0b6d2e4a7c5b3d1f8a6e4c2b0d9f7e5c3a1b8d6f4e2c0a9b7d5f3e1c8a6b",
            },
          ],
        });
      }
      throw new Error(`unexpected command ${cmd}`);
    });

    const { container } = renderWithProviders(<SettingsPage />);

    const skipped = await screen.findByRole("region", { name: "Skipped versions" });
    expect(within(skipped).getByText("qwen3:8b")).toBeInTheDocument();
    expect(within(skipped).getByText("A newer build")).toBeInTheDocument();
    expect(
      within(skipped).getByRole("button", { name: "Stop skipping the newer build of qwen3:8b" }),
    ).toBeInTheDocument();
    expect(container.textContent).not.toContain("sha256");
  });

  it("keeps listing a skipped version its source no longer offers, and keeps it through a save of anything else", async () => {
    // Say glib 2.89.0 was skipped and its source has since moved on to
    // 2.90.0: the skip hides nothing any more (`hidingRule` matches only the
    // version a row offers). This page does not read the snapshot -- the
    // mock below answers nothing else -- so what a source offers now cannot
    // make it drop an entry. The skip is still listed, and a save of
    // another setting writes it back unchanged: nothing is pruned behind
    // the user's back.
    vi.mocked(invoke).mockImplementation(async (cmd: string, _args?: unknown) => {
      if (cmd === "get_settings") {
        return baseSettings({ skipped_versions: [{ key: glibKey, version: "2.89.0" }] });
      }
      if (cmd === "set_settings") return undefined;
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const skipped = await screen.findByRole("region", { name: "Skipped versions" });
    expect(within(skipped).getByText("2.89.0")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("switch", { name: "Show technical details" }));
    await waitFor(() => expect(lastSaved().show_technical_details).toBe(true));
    expect(lastSaved().skipped_versions).toEqual([{ key: glibKey, version: "2.89.0" }]);
  });

  it("says what Show technical details shows, and nothing it does not", async () => {
    // Every reader of `show_technical_details`, and nothing else (T2 of the
    // copy table): a tool's own error text behind a row's "Can't check"
    // (updateDetails.tsx), a tool's location in its drawer and where an
    // Unknown-page link points, the command a chip talks about and Copy
    // command, and a confirmation's command, open from the start
    // (CommandPreview.tsx). Version numbers are on every row, on or off.
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return baseSettings();
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    expect(await screen.findByRole("switch", { name: "Show technical details" })).toHaveAccessibleDescription(
      "Shows tools' own error messages, file locations and the commands to run, and opens a confirmation's command from the start.",
    );
    expect(zhCN.settings.showTechnicalDetails.description).toBe(
      "显示工具自己的报错、文件位置和要运行的命令，确认时直接展开命令。",
    );
    expect(zhCN.settings.showTechnicalDetails.description).not.toMatch(/版本号/);
  });

  it("calls the two lists 已跳过的版本 and 不再提醒的软件 in Chinese, as they were asked for", () => {
    expect(zhCN.settings.skippedVersions.title).toBe("已跳过的版本");
    expect(zhCN.settings.ignoredUpdates.title).toBe("不再提醒的软件");
  });

  it("round-trips the include-self-updating toggle through set_settings and re-checks for updates", async () => {
    // The backend reads include_self_updating fresh on each refresh, but
    // nothing was triggering one: the save only wrote to the query cache, so
    // flipping the switch changed nothing the user could see until the app
    // was restarted.
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return baseSettings();
      if (cmd === "set_settings") return undefined;
      if (cmd === "refresh") return null;
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const toggle = await screen.findByRole("switch", { name: "Show self-updating apps" });
    expect(toggle).not.toBeChecked();
    fireEvent.click(toggle);

    await waitFor(() =>
      expect(vi.mocked(invoke)).toHaveBeenCalledWith("set_settings", {
        settings: expect.objectContaining({ include_self_updating: true }),
      }),
    );
    await waitFor(() =>
      expect(vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "refresh")).toHaveLength(1),
    );
  });

  it("does not re-scan every source for a save that cannot change what a refresh finds", async () => {
    // Only include_self_updating changes the backend's answer. Refreshing on
    // every save would put a full scan of every source behind each Skip this
    // version or Never remind me click on the Updates page, which shares
    // this mutation.
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return baseSettings();
      if (cmd === "set_settings") return undefined;
      if (cmd === "refresh") return null;
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    fireEvent.click(await screen.findByRole("switch", { name: "Show technical details" }));

    await waitFor(() =>
      expect(vi.mocked(invoke)).toHaveBeenCalledWith("set_settings", {
        settings: expect.objectContaining({ show_technical_details: true }),
      }),
    );
    expect(vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "refresh")).toHaveLength(0);
  });
  it("marks the chosen language visibly, not only through aria-checked", async () => {
    // Under Tailwind's preflight a class-less <button> has no background, no
    // border and no padding, so the three languages rendered as three bare
    // words and the only marker of the current one was aria-checked, which a
    // sighted user cannot see.
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return baseSettings({ language: "En" });
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const english = await screen.findByRole("radio", { name: "English" });
    const system = screen.getByRole("radio", { name: "System" });

    expect(english.className).not.toBe("");
    expect(system.className).not.toBe("");
    expect(english.className).not.toBe(system.className);
  });

  it("gives the language group one tab stop and moves the choice with the arrow keys", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return baseSettings({ language: "System" });
      if (cmd === "set_settings") return undefined;
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const system = await screen.findByRole("radio", { name: "System" });
    const english = screen.getByRole("radio", { name: "English" });
    const chinese = screen.getByRole("radio", { name: "简体中文" });

    // Roving tabindex: Tab reaches the group once and lands on the current
    // choice, rather than stopping at all three buttons in turn.
    expect(system).toHaveAttribute("tabindex", "0");
    expect(english).toHaveAttribute("tabindex", "-1");
    expect(chinese).toHaveAttribute("tabindex", "-1");

    fireEvent.keyDown(system, { key: "ArrowRight" });

    await waitFor(() => expect(english).toHaveAttribute("aria-checked", "true"));
    expect(english).toHaveAttribute("tabindex", "0");
    expect(english).toHaveFocus();
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("set_settings", {
      settings: expect.objectContaining({ language: "En" }),
    });

    // The group wraps, so the arrow keys never dead-end.
    fireEvent.keyDown(english, { key: "ArrowLeft" });
    await waitFor(() => expect(system).toHaveAttribute("aria-checked", "true"));
    fireEvent.keyDown(system, { key: "ArrowLeft" });
    await waitFor(() => expect(chinese).toHaveAttribute("aria-checked", "true"));
  });

  it("styles the Remind me again and Stop skipping buttons so they read as controls", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") {
        return baseSettings({
          ignored_updates: [jqKey],
          skipped_versions: [{ key: glibKey, version: "2.90.0" }],
        });
      }
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const remind = await screen.findByRole("button", { name: "Remind me again about jq" });
    const unskip = screen.getByRole("button", { name: "Stop skipping 2.90.0 of glib" });
    expect(remind.className).not.toBe("");
    expect(unskip.className).toBe(remind.className);
  });

  it("groups the settings in three cards: General, Updates and Hidden updates", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") {
        return baseSettings({ ignored_updates: [jqKey], skipped_versions: [{ key: glibKey, version: "2.90.0" }] });
      }
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const general = await screen.findByRole("region", { name: "General" });
    const updates = screen.getByRole("region", { name: "Updates" });
    const hidden = screen.getByRole("region", { name: "Hidden updates" });
    expect(within(general).getByRole("radiogroup", { name: "Language" })).toBeInTheDocument();
    expect(within(general).getByRole("switch", { name: "Show technical details" })).toBeInTheDocument();
    expect(within(updates).getByRole("switch", { name: "Show self-updating apps" })).toHaveAccessibleDescription(
      "Also list Homebrew apps that update themselves, like Chrome, under Updates.",
    );
    expect(within(hidden).getByRole("region", { name: "Skipped versions" })).toBeInTheDocument();
    expect(within(hidden).getByRole("region", { name: "Never remind me about" })).toBeInTheDocument();
    // The groups' titles in the section style; nothing else is on the switches' cards.
    for (const name of ["General", "Updates", "Hidden updates"]) {
      expect(screen.getByRole("heading", { level: 2, name }).className).toContain("text-section");
    }
    expect(within(general).queryByRole("button", { name: "Remind me again about jq" })).toBeNull();
  });

  it("calls the groups and the self-updating switch what the copy table has them in Chinese", () => {
    expect(zhCN.settings.groups).toEqual({ general: "通用", updates: "更新", hidden: "已隐藏的更新" });
    expect(zhCN.settings.includeSelfUpdating.label).toBe("显示自更新 App");
    // The switch adds Homebrew's self-updating apps and nothing else, so
    // its line names Homebrew.
    expect(zhCN.settings.includeSelfUpdating.description).toContain("Homebrew");
    expect(zhCN.settings.skippedVersions.empty).toBe("没有跳过的版本");
    expect(zhCN.settings.ignoredUpdates.empty).toBe("没有设为不再提醒的软件");
  });

  it("puts a skipped version back with its own undo, from its card, and leaves the other list alone", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string, _args?: unknown) => {
      if (cmd === "get_settings") {
        return baseSettings({ ignored_updates: [jqKey], skipped_versions: [{ key: glibKey, version: "2.90.0" }] });
      }
      if (cmd === "set_settings") return undefined;
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const hidden = await screen.findByRole("region", { name: "Hidden updates" });
    fireEvent.click(within(hidden).getByRole("button", { name: "Stop skipping 2.90.0 of glib" }));

    const skipped = within(hidden).getByRole("region", { name: "Skipped versions" });
    await waitFor(() => expect(within(skipped).getByText("No skipped versions")).toBeInTheDocument());
    expect(lastSaved().skipped_versions).toEqual([]);
    expect(lastSaved().ignored_updates).toEqual([jqKey]);
    expect(within(hidden).getByRole("button", { name: "Remind me again about jq" })).toBeInTheDocument();
  });
});
