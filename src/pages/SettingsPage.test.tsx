import { afterEach, describe, expect, it, vi, beforeEach } from "vitest";
import { act, screen, waitFor, fireEvent, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { SettingsPage } from "./SettingsPage";
import { useUiStore } from "../store/ui";
import i18n from "../i18n";
import zhCN from "../i18n/zh-CN.json";
import { loadToolIcons, type ToolIconPack } from "../lib/toolIcons";
import type { ArtifactKey, InstalledArtifact, Settings, Snapshot } from "../lib/types";
import { NO_FACTS } from "../lib/types";
import { BUTTON } from "../components/ui/controls";
import { GROUP_ROW } from "../components/ui/group";
import { creditSource } from "../components/IconCreditsDrawer";
import tauriConfig from "../../src-tauri/tauri.conf.json";

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

function artifact(key: ArtifactKey, displayName: string): InstalledArtifact {
  return {
    key,
    display_name: displayName,
    version: "1.0.0",
    reason: "Requested",
    description: null,
    homepage: null,
    size_bytes: null,
    installed_at: null,
    path: null,
    auto_updates: false,
    uninstall_blocked: null,
    facts: NO_FACTS,
  };
}

// A snapshot with these installed, and nothing else the page reads.
function snapshotOf(artifacts: InstalledArtifact[], updates: Snapshot["updates"] = []): Snapshot {
  return {
    generation: 3,
    round: 3,
    detect: "Found",
    instances: [],
    artifacts,
    updates,
    refreshed_at: 1790586000,
    stale: false,
    errors: [],
  };
}

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
    expect(screen.getByRole("combobox", { name: "Language" })).toHaveValue("System");
    expect(screen.getByText("jq")).toBeInTheDocument();
  });

  it("optimistically applies a toggle and rolls it back when the save fails", async () => {
    // Technical details on, and staying on: the save that was to turn them
    // off is the one that fails.
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return baseSettings({ show_technical_details: true });
      if (cmd === "set_settings") throw new Error("disk full");
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const toggle = await screen.findByRole("switch", { name: "Show technical details" });
    await waitFor(() => expect(toggle).toBeChecked());

    fireEvent.click(toggle);
    expect(toggle).not.toBeChecked();

    // One sentence: what went wrong, and that the old setting is back.
    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent("Couldn't save: disk full. Your previous setting is back."),
    );
    await waitFor(() => expect(toggle).toBeChecked());
  });

  it("says a save failed without the system's words while technical details are off, the old setting back", async () => {
    // The save that was to turn them on is the one that fails: they stay off.
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

    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent("Couldn't save. Your previous setting is back."),
    );
    expect(screen.queryByText(/disk full/)).toBeNull();
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

    const never = screen.getByRole("region", { name: "Tools with reminders off" });
    await waitFor(() => expect(within(never).getByText("None")).toBeInTheDocument());
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
    const never = screen.getByRole("region", { name: "Tools with reminders off" });
    // A skip names the version it hides; that version is what the entry
    // is, beside its source.
    expect(within(skipped).getByText("glib")).toBeInTheDocument();
    expect(within(skipped).getByText("Homebrew · 2.90.0")).toBeInTheDocument();
    expect(within(skipped).getByRole("button", { name: "Stop skipping 2.90.0 of glib" })).toHaveTextContent(
      "Stop Skipping",
    );
    expect(within(skipped).queryByText("jq")).toBeNull();
    expect(within(never).getByText("jq")).toBeInTheDocument();
    expect(within(never).getByRole("button", { name: "Remind me again about jq" })).toHaveTextContent(
      "Remind Me Again",
    );
    expect(within(never).queryByText("glib")).toBeNull();
  });

  it("says each list is empty on its own, in one row of muted text", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return baseSettings({ ignored_updates: [jqKey] });
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const skipped = await screen.findByRole("region", { name: "Skipped versions" });
    const never = screen.getByRole("region", { name: "Tools with reminders off" });
    const none = within(skipped).getByText("None");
    // One row, 36 high, in the group's container; no list, and no button.
    expect(none.className).toContain("min-h-9");
    expect(none.parentElement?.className).toContain("bg-group");
    expect(none.parentElement?.children).toHaveLength(1);
    expect(within(skipped).queryByRole("list")).toBeNull();
    expect(within(skipped).queryByRole("button")).toBeNull();
    // What it says is information: muted, never the tertiary of disabled text.
    expect(none.className).toContain("text-muted");
    expect(none.className).not.toContain("text-tertiary");
    // The other group is not empty, and says nothing of the kind.
    expect(within(never).queryByText("None")).toBeNull();
    expect(within(never).getByRole("list")).toBeInTheDocument();
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
    expect(within(skipped).getByText("Ollama · New version")).toBeInTheDocument();
    expect(
      within(skipped).getByRole("button", { name: "Stop skipping the new version of qwen3:8b" }),
    ).toBeInTheDocument();
    expect(container.textContent).not.toContain("sha256");
  });

  it("keeps listing a skipped version its source no longer offers, and keeps it through a save of anything else", async () => {
    // Say glib 2.89.0 was skipped and its source has since moved on to
    // 2.90.0: the skip hides nothing any more (`hidingRule` matches only the
    // version a row offers). This page reads the snapshot for names only,
    // so what a source offers now -- 2.90.0, below -- cannot make it drop
    // an entry. The skip is still listed, and a save of another setting
    // writes it back unchanged: nothing is pruned behind the user's back.
    vi.mocked(invoke).mockImplementation(async (cmd: string, _args?: unknown) => {
      if (cmd === "get_settings") {
        return baseSettings({ skipped_versions: [{ key: glibKey, version: "2.89.0" }] });
      }
      if (cmd === "get_snapshot") {
        return snapshotOf(
          [artifact(glibKey, "glib")],
          [
            {
              key: glibKey,
              current: "2.88.3",
              target: "2.90.0",
              channel: "Native",
              checkable: true,
              warnings: [],
              blocked: null,
            },
          ],
        );
      }
      if (cmd === "set_settings") return undefined;
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const skipped = await screen.findByRole("region", { name: "Skipped versions" });
    expect(within(skipped).getByText("Homebrew · 2.89.0")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("switch", { name: "Show technical details" }));
    await waitFor(() => expect(lastSaved().show_technical_details).toBe(true));
    expect(lastSaved().skipped_versions).toEqual([{ key: glibKey, version: "2.89.0" }]);
  });

  it("names each hidden update the way the Updates page does, with its source beside it", async () => {
    const claudeKey: ArtifactKey = { instance_id: "standalone-claude", kind: "Binary", name: "claude" };
    const agyKey: ArtifactKey = { instance_id: "standalone-agy", kind: "Binary", name: "agy" };
    const grokKey: ArtifactKey = { instance_id: "standalone-grok", kind: "Binary", name: "grok" };
    const codeKey: ArtifactKey = { instance_id: "brew:/opt/homebrew", kind: "Cask", name: "visual-studio-code" };
    const brewTsKey: ArtifactKey = { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "typescript" };
    const npmTsKey: ArtifactKey = { instance_id: "npm:/opt/homebrew", kind: "Package", name: "typescript" };
    const ffmpegKey: ArtifactKey = { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "ffmpeg" };
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") {
        return baseSettings({
          skipped_versions: [
            { key: claudeKey, version: "2.1.3" },
            { key: codeKey, version: "1.105.0" },
          ],
          // grok and ffmpeg are no longer installed: the snapshot has no
          // name for them.
          ignored_updates: [agyKey, grokKey, brewTsKey, npmTsKey, ffmpegKey],
        });
      }
      if (cmd === "get_snapshot") {
        return snapshotOf([
          artifact(claudeKey, "Claude Code"),
          artifact(agyKey, "Antigravity CLI"),
          artifact(codeKey, "Microsoft Visual Studio Code"),
          artifact(brewTsKey, "typescript"),
          artifact(npmTsKey, "typescript"),
        ]);
      }
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const skipped = await screen.findByRole("region", { name: "Skipped versions" });
    const never = screen.getByRole("region", { name: "Tools with reminders off" });
    const lines = (list: HTMLElement) =>
      within(list)
        .getAllByRole("listitem")
        .map((item) => [...item.querySelectorAll("span > span")].map((part) => part.textContent));

    // A tool with its own installer is its own source: its name, once.
    await waitFor(() =>
      expect(lines(skipped)).toEqual([
        ["Claude Code", "2.1.3"],
        ["Microsoft Visual Studio Code", "Homebrew · 1.105.0"],
      ]),
    );
    expect(lines(never)).toEqual([
      ["Antigravity CLI"],
      // Not in the snapshot: a tool with its own installer by its product
      // name all the same, anything else by its package's.
      ["Grok Build"],
      // Two installs of one name, told apart by their source.
      ["typescript", "Homebrew"],
      ["typescript", "npm"],
      ["ffmpeg", "Homebrew"],
    ]);
    expect(screen.queryByText("claude")).toBeNull();
    expect(screen.queryByText("agy")).toBeNull();
    // Their buttons are named by the same names.
    expect(within(skipped).getByRole("button", { name: "Stop skipping 2.1.3 of Claude Code" })).toBeInTheDocument();
    expect(
      within(skipped).getByRole("button", { name: "Stop skipping 1.105.0 of Microsoft Visual Studio Code" }),
    ).toBeInTheDocument();
    expect(within(never).getByRole("button", { name: "Remind me again about Antigravity CLI" })).toBeInTheDocument();
    expect(within(never).getByRole("button", { name: "Remind me again about Grok Build" })).toBeInTheDocument();
    expect(within(never).getByRole("button", { name: "Remind me again about ffmpeg" })).toBeInTheDocument();
  });

  it("names the source of a hidden update as the sidebar does where this Mac has two Homebrews", async () => {
    // A Mac migrated from Intel: jq from each Homebrew, each hidden.
    // 「Homebrew」 beside both would not say which is which.
    const intelJqKey: ArtifactKey = { ...jqKey, instance_id: "brew:/usr/local" };
    const brew = {
      id: "brew:/opt/homebrew",
      adapter_id: "brew",
      exe_path: "/opt/homebrew/bin/brew",
      prefix: "/opt/homebrew",
      scope: "User" as const,
      version: "7.0.3",
      status: { unavailable: null, notes: [] },
      unverified_version: null,
      read_only_reason: null,
    };
    const intel = { ...brew, id: "brew:/usr/local", exe_path: "/usr/local/bin/brew", prefix: "/usr/local" };
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return baseSettings({ ignored_updates: [jqKey, intelJqKey] });
      if (cmd === "get_snapshot") {
        return { ...snapshotOf([artifact(jqKey, "jq"), artifact(intelJqKey, "jq")]), instances: [brew, intel] };
      }
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const never = await screen.findByRole("region", { name: "Tools with reminders off" });
    await waitFor(() =>
      expect(
        within(never)
          .getAllByRole("listitem")
          .map((item) => [...item.querySelectorAll("span > span")].map((part) => part.textContent)),
      ).toEqual([
        ["jq", "Homebrew (Apple silicon)"],
        ["jq", "Homebrew (Intel)"],
      ]),
    );
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

    // In plain words, on one line.
    expect(await screen.findByRole("switch", { name: "Show technical details" })).toHaveAccessibleDescription(
      "Show error messages, file locations and commands from the tools themselves, with commands expanded in confirmations.",
    );
    // 「原始错误信息」, not the colloquial 「工具自己的报错」; 「在确认窗口中」
    // says where the command is expanded, which 「确认时」 left unclear.
    expect(zhCN.settings.showTechnicalDetails.description).toBe("显示原始错误信息和文件位置，并在确认窗口中展开命令。");
    expect(zhCN.settings.showTechnicalDetails.description).not.toMatch(/版本号/);
  });

  it("calls the two lists 已跳过的版本 and 不再提醒的工具 in Chinese", () => {
    expect(zhCN.settings.skippedVersions.title).toBe("已跳过的版本");
    expect(zhCN.settings.ignoredUpdates.title).toBe("不再提醒的工具");
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

    const toggle = await screen.findByRole("switch", { name: "Show apps that update themselves" });
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

  it("offers the daily check in the Updates card, off, saying what it does, with Notify me under it, off until it is on", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return baseSettings();
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const updates = await screen.findByRole("region", { name: "Updates" });
    const daily = within(updates).getByRole("switch", { name: "Check for updates every day" });
    expect(daily).not.toBeChecked();
    expect(daily).toHaveAccessibleDescription(
      "Banager checks for updates once a day while it's running, and doesn't install the updates it finds.",
    );
    const notify = within(updates).getByRole("switch", { name: "Notify me when there are updates" });
    expect(notify).not.toBeChecked();
    expect(notify).toBeDisabled();
    // Why it does not move, and what turns it on.
    expect(notify).toHaveAccessibleDescription("Requires “Check for updates every day”.");
    // Under the daily check, the row it depends on.
    const switches = within(updates).getAllByRole("switch");
    expect(switches.indexOf(notify)).toBe(switches.indexOf(daily) + 1);
  });

  it("says when the next automatic check is due under the daily check while it is on, and nothing while it is off", async () => {
    // Two hours from now: today, or -- just before midnight -- tomorrow.
    const due = Math.floor(Date.now() / 1000) + 2 * 60 * 60;
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return baseSettings({ auto_check: true });
      if (cmd === "get_snapshot") return { ...snapshotOf([]), next_auto_check_at: due };
      if (cmd === "set_settings") return undefined;
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const line = await screen.findByText(
      /^Next automatic check: about .+ (today|tomorrow)$/,
    );
    expect(line).toHaveAttribute("data-next-auto-check");
    // Under the switch's own description, in its row.
    const daily = screen.getByRole("switch", { name: "Check for updates every day" });
    expect(daily.parentElement).toContainElement(line);

    fireEvent.click(daily);
    await waitFor(() => expect(lastSaved()).toEqual(baseSettings({ auto_check: false })));
    expect(document.querySelector("[data-next-auto-check]")).toBeNull();
  });

  it("says nothing about the next automatic check before any check has counted", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return baseSettings({ auto_check: true });
      if (cmd === "get_snapshot") return { ...snapshotOf([]), next_auto_check_at: null };
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    await screen.findByRole("switch", { name: "Check for updates every day" });
    await waitFor(() => expect(vi.mocked(invoke).mock.calls.some(([cmd]) => cmd === "get_snapshot")).toBe(true));
    expect(document.querySelector("[data-next-auto-check]")).toBeNull();
  });

  it("turns the daily check on and saves it, which makes Notify me available, and re-scans nothing", async () => {
    // The daily check runs at its next look, a day after the last check
    // ended; turning it on is no reason to check now.
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return baseSettings();
      if (cmd === "set_settings") return undefined;
      if (cmd === "refresh") return null;
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    fireEvent.click(await screen.findByRole("switch", { name: "Check for updates every day" }));

    await waitFor(() => expect(lastSaved()).toEqual(baseSettings({ auto_check: true })));
    expect(screen.getByRole("switch", { name: "Check for updates every day" })).toBeChecked();
    const notify = screen.getByRole("switch", { name: "Notify me when there are updates" });
    expect(notify).toBeEnabled();
    expect(notify).not.toBeChecked();
    // Nothing left to say about why it would not move.
    expect(notify).not.toHaveAttribute("aria-describedby");
    expect(screen.queryByText("Requires “Check for updates every day”.")).toBeNull();
    expect(vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "refresh")).toHaveLength(0);
  });

  it("turns Notify me on while the daily check is on, once permission is granted, and off with the daily check", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return baseSettings({ auto_check: true });
      if (cmd === "set_settings") return undefined;
      if (cmd === "request_notification_permission") return true;
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const notify = await screen.findByRole("switch", { name: "Notify me when there are updates" });
    expect(notify).toBeEnabled();
    fireEvent.click(notify);
    await waitFor(() =>
      expect(lastSaved()).toEqual(baseSettings({ auto_check: true, notify_updates: true })),
    );
    expect(notify).toBeChecked();
    expect(notify).toBeEnabled();
    expect(vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "request_notification_permission")).toHaveLength(1);

    fireEvent.click(screen.getByRole("switch", { name: "Check for updates every day" }));
    await waitFor(() =>
      expect(lastSaved()).toEqual(baseSettings({ auto_check: false, notify_updates: false })),
    );
    expect(notify).not.toBeChecked();
    expect(notify).toBeDisabled();
  });

  it("asks for permission before saving Notify me on, the switch on and still while it waits", async () => {
    let answer: (granted: boolean) => void = () => {};
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return baseSettings({ auto_check: true });
      if (cmd === "set_settings") return undefined;
      if (cmd === "request_notification_permission") {
        return new Promise<boolean>((resolve) => {
          answer = resolve;
        });
      }
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const notify = await screen.findByRole("switch", { name: "Notify me when there are updates" });
    fireEvent.click(notify);
    await waitFor(() => expect(notify).toBeChecked());
    expect(notify).toBeDisabled();
    expect(vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "set_settings")).toHaveLength(0);

    act(() => answer(true));

    await waitFor(() =>
      expect(lastSaved()).toEqual(baseSettings({ auto_check: true, notify_updates: true })),
    );
    expect(notify).toBeChecked();
    expect(notify).toBeEnabled();
  });

  it("turns Notify me back off when permission is refused, says where to allow it, and saves nothing", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return baseSettings({ auto_check: true });
      if (cmd === "set_settings") return undefined;
      if (cmd === "request_notification_permission") return false;
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const notify = await screen.findByRole("switch", { name: "Notify me when there are updates" });
    fireEvent.click(notify);

    await waitFor(() =>
      expect(notify).toHaveAccessibleDescription("Allow Banager to send notifications in System Settings > Notifications."),
    );
    expect(notify).not.toBeChecked();
    expect(notify).toBeEnabled();
    expect(screen.getByRole("status")).toHaveTextContent("Allow Banager to send notifications in System Settings > Notifications.");
    expect(vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "set_settings")).toHaveLength(0);
  });

  it("takes a permission request that failed as refused", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return baseSettings({ auto_check: true });
      if (cmd === "request_notification_permission") throw "notification plugin not ready";
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const notify = await screen.findByRole("switch", { name: "Notify me when there are updates" });
    fireEvent.click(notify);

    await waitFor(() =>
      expect(notify).toHaveAccessibleDescription("Allow Banager to send notifications in System Settings > Notifications."),
    );
    expect(notify).not.toBeChecked();
  });

  it("drops the line once permission is granted on another try, or the daily check is turned off", async () => {
    let grant = false;
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return baseSettings({ auto_check: true });
      if (cmd === "set_settings") return undefined;
      if (cmd === "request_notification_permission") return grant;
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const notify = await screen.findByRole("switch", { name: "Notify me when there are updates" });
    fireEvent.click(notify);
    await waitFor(() => expect(screen.getByRole("status")).toBeInTheDocument());

    // Allowed in System Settings, then tried again.
    grant = true;
    fireEvent.click(notify);
    await waitFor(() =>
      expect(lastSaved()).toEqual(baseSettings({ auto_check: true, notify_updates: true })),
    );
    expect(screen.queryByRole("status")).toBeNull();
    expect(notify).not.toHaveAttribute("aria-describedby");

    // Refused again after turning it off; then the daily check goes off.
    grant = false;
    fireEvent.click(notify);
    await waitFor(() =>
      expect(lastSaved()).toEqual(baseSettings({ auto_check: true, notify_updates: false })),
    );
    fireEvent.click(notify);
    await waitFor(() => expect(screen.getByRole("status")).toBeInTheDocument());
    fireEvent.click(screen.getByRole("switch", { name: "Check for updates every day" }));
    await waitFor(() => expect(screen.queryByRole("status")).toBeNull());
  });

  it("does not save Notify me on when the daily check was turned off while permission was asked for", async () => {
    let answer: (granted: boolean) => void = () => {};
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return baseSettings({ auto_check: true });
      if (cmd === "set_settings") return undefined;
      if (cmd === "request_notification_permission") {
        return new Promise<boolean>((resolve) => {
          answer = resolve;
        });
      }
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const notify = await screen.findByRole("switch", { name: "Notify me when there are updates" });
    fireEvent.click(notify);
    await waitFor(() => expect(notify).toBeChecked());
    fireEvent.click(screen.getByRole("switch", { name: "Check for updates every day" }));
    await waitFor(() =>
      expect(lastSaved()).toEqual(baseSettings({ auto_check: false, notify_updates: false })),
    );
    const saves = () => vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "set_settings").length;
    const before = saves();

    await act(async () => answer(true));

    expect(saves()).toBe(before);
    expect(notify).not.toBeChecked();
    expect(notify).toBeDisabled();
  });

  it("shows Notify me off while the daily check is off, even where a file saved it on", async () => {
    // What the switch shows is what can happen: nothing is checked, so
    // nothing can be notified.
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return baseSettings({ auto_check: false, notify_updates: true });
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const notify = await screen.findByRole("switch", { name: "Notify me when there are updates" });
    expect(notify).not.toBeChecked();
    expect(notify).toBeDisabled();
  });

  it("names the daily check and its notification in Chinese as the spec does, and says the check installs none of the updates it finds", () => {
    // Not the spec's 只检查不安装: every check's `brew update` can install
    // a package Homebrew moved between a formula and a cask
    // (docs/what-we-run.md, Homebrew), so the subtitle claims only that no
    // update the check finds is installed. The same holds against the
    // polish-3 copy table's 「不会自动安装」, which claims more than that.
    expect(zhCN.settings.autoCheck.label).toBe("每天自动检查");
    expect(zhCN.settings.autoCheck.description).toBe("Banager运行时每天检查一次更新，查到的更新不会自动安装。");
    expect(zhCN.settings.notifyUpdates.label).toBe("有更新时通知我");
    expect(zhCN.settings.notifyUpdates.refused).toBe("请在“系统设置”>“通知”中允许Banager发送通知。");
  });

  it("says whose the logos are in the icon credits in few words, in Chinese as the review asked", () => {
    expect(zhCN.settings.iconCredits.owners).toBe("各标志归其权利人所有，仅用于识别工具。");
    // Not 「另有自己的许可」, which read as a translation.
    expect(zhCN.settings.iconCredits.ownLicense).toBe("以下标志采用其他许可协议：");
  });

  it("offers the language as a popup button: the chosen one's name, then ⌃⌄ in a grey capsule, no border", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return baseSettings({ language: "En" });
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const popup = await screen.findByRole("combobox", { name: "Language" });
    // A native select, so that WebKit opens the Mac's own menu, with the
    // three choices in it and the current one chosen.
    expect(popup.tagName).toBe("SELECT");
    expect(popup).toHaveValue("En");
    expect(within(popup).getAllByRole("option").map((option) => option.textContent)).toEqual([
      "System",
      "English",
      "简体中文",
    ]);
    // What shows is drawn beside it: the value in the body size, and the
    // 20-wide grey capsule with its chevrons; the select itself is laid
    // over them, unseen.
    const control = popup.parentElement as HTMLElement;
    const [value, capsule] = [...control.children] as HTMLElement[];
    expect(value).toHaveTextContent("English");
    expect(value.className).toContain("text-body");
    expect(capsule.className.split(" ")).toEqual(expect.arrayContaining(["w-5", "rounded-full", "bg-fill"]));
    // ⌃ over ⌄ in a light stroke, 2 apart so they do not meet in a diamond.
    const chevrons = capsule.querySelector("svg");
    expect(chevrons).toHaveAttribute("height", "12");
    expect(chevrons).toHaveAttribute("stroke-width", "1.25");
    expect(chevrons?.querySelector("path")).toHaveAttribute("d", "M1.25 4.5L4 1.75L6.75 4.5M1.25 7.5L4 10.25L6.75 7.5");
    expect(popup.className.split(" ")).toEqual(expect.arrayContaining(["absolute", "inset-0", "opacity-0"]));
    // No border and no fill behind the value.
    expect(control.className).not.toMatch(/\bborder\b|\bbg-/);
    // The keyboard's ring goes round the whole, since the select is not seen.
    expect(control.className).toContain("has-[:focus-visible]:outline-3");
    // Not a segmented control any more.
    expect(screen.queryByRole("radiogroup")).toBeNull();
    expect(screen.queryByRole("radio")).toBeNull();
  });

  it("saves the language chosen from the popup, and shows it", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return baseSettings({ language: "System" });
      if (cmd === "set_settings") return undefined;
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const popup = await screen.findByRole("combobox", { name: "Language" });
    expect(popup.parentElement?.firstElementChild).toHaveTextContent("System");

    fireEvent.change(popup, { target: { value: "ZhCn" } });

    await waitFor(() =>
      expect(vi.mocked(invoke)).toHaveBeenCalledWith("set_settings", {
        settings: expect.objectContaining({ language: "ZhCn" }),
      }),
    );
    expect(popup).toHaveValue("ZhCn");
    expect(popup.parentElement?.firstElementChild).toHaveTextContent("简体中文");
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

  it("groups the settings in five groups: General, Updates, the two kinds of hidden update, and About", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") {
        return baseSettings({ ignored_updates: [jqKey], skipped_versions: [{ key: glibKey, version: "2.90.0" }] });
      }
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const general = await screen.findByRole("region", { name: "General" });
    const updates = screen.getByRole("region", { name: "Updates" });
    const skipped = screen.getByRole("region", { name: "Skipped versions" });
    const never = screen.getByRole("region", { name: "Tools with reminders off" });
    expect(screen.getAllByRole("heading", { level: 2 }).map((heading) => heading.textContent)).toEqual([
      "General",
      "Updates",
      "Skipped versions",
      "Tools with reminders off",
      "About",
    ]);
    expect(within(general).getByRole("combobox", { name: "Language" })).toBeInTheDocument();
    expect(within(general).getByRole("switch", { name: "Show technical details" })).toBeInTheDocument();
    expect(within(updates).getByRole("switch", { name: "Show apps that update themselves" })).toHaveAccessibleDescription(
      "Also list apps installed with Homebrew that update themselves, like Chrome, under Updates.",
    );
    expect(within(skipped).getByRole("button", { name: "Stop skipping 2.90.0 of glib" })).toBeInTheDocument();
    expect(within(never).getByRole("button", { name: "Remind me again about jq" })).toBeInTheDocument();
    expect(within(general).queryByRole("button", { name: "Remind me again about jq" })).toBeNull();
    // The old single group of hidden updates, with two small titles in it, is gone.
    expect(screen.queryByRole("region", { name: "Hidden updates" })).toBeNull();
    expect(screen.queryByRole("heading", { level: 3 })).toBeNull();
  });

  it("draws each group as System Settings does: a 13 bold title over the rows' text, a grey container with no edge", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return baseSettings({ ignored_updates: [jqKey] });
      throw new Error(`unexpected command ${cmd}`);
    });

    const { container } = renderWithProviders(<SettingsPage />);

    await screen.findByRole("region", { name: "General" });
    // The column: as wide as 560 or the page less 40, centred, 20 under the toolbar.
    const column = container.firstElementChild as HTMLElement;
    expect(column.className.split(" ")).toEqual(
      expect.arrayContaining(["mx-auto", "w-[min(560px,calc(100%-40px))]", "pt-5", "gap-6"]),
    );
    for (const name of ["General", "Updates", "Skipped versions", "Tools with reminders off", "About"]) {
      const heading = screen.getByRole("heading", { level: 2, name });
      // 13/16 bold, 10 in (in line with the rows' words), 8 over the container.
      expect(heading.className.split(" ")).toEqual(expect.arrayContaining(["text-title", "px-2.5", "mb-2"]));
      const group = heading.nextElementSibling as HTMLElement;
      const classes = group.className.split(" ");
      expect(classes).toEqual(expect.arrayContaining(["bg-group", "rounded-group"]));
      expect(classes).not.toContain("border");
      // A hairline between each two rows, 10 in from either side.
      expect(classes).toEqual(
        expect.arrayContaining([
          "[&>*+*]:before:left-2.5",
          "[&>*+*]:before:right-2.5",
          "[&>*+*]:before:h-px",
          "[&>*+*]:before:bg-group-separator",
        ]),
      );
    }
  });

  it("makes a row 36 high on one line and 46 with a second, and gives a group one standing second line at most", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return baseSettings({ auto_check: true });
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const rowOf = (control: HTMLElement) => control.closest(".px-2\\.5") as HTMLElement;
    const language = rowOf(await screen.findByRole("combobox", { name: "Language" }));
    expect(language.className.split(" ")).toEqual(expect.arrayContaining(["min-h-9", "px-2.5"]));
    const technical = rowOf(screen.getByRole("switch", { name: "Show technical details" }));
    expect(technical.className).toContain("min-h-11.5");
    // Labels in the regular weight.
    expect(within(language).getByText("Language").className).not.toMatch(/font-(medium|semibold|bold)/);

    // With the daily check on, nothing passing to say: one row with a
    // second line in each group that has any.
    const twoLines = (region: HTMLElement) =>
      within(region)
        .getAllByRole("switch")
        .map(rowOf)
        .filter((row) => row.className.includes("min-h-11.5"));
    expect(twoLines(screen.getByRole("region", { name: "General" }))).toHaveLength(1);
    expect(twoLines(screen.getByRole("region", { name: "Updates" }))).toEqual([
      rowOf(screen.getByRole("switch", { name: "Check for updates every day" })),
    ]);
    for (const name of ["Notify me when there are updates", "Show apps that update themselves"]) {
      expect(rowOf(screen.getByRole("switch", { name })).className).toContain("min-h-9");
    }
    // A second line is 11 with its lines 16 apart, should it wrap, and
    // still 14 high on one line: 1 short at either end.
    const subtitle = within(technical).getByText("Show error messages, file locations and commands from the tools themselves, with commands expanded in confirmations.");
    expect(subtitle.className.split(" ")).toEqual(
      expect.arrayContaining(["text-small", "leading-4", "-my-px", "text-muted"]),
    );
  });

  it("says what the daily check does under its switch, and which apps the last switch adds in the group's footnote", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return baseSettings({ auto_check: true });
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const updates = await screen.findByRole("region", { name: "Updates" });
    const daily = within(updates).getByRole("switch", { name: "Check for updates every day" });
    const what = within(updates).getByText(
      "Banager checks for updates once a day while it's running, and doesn't install the updates it finds.",
    );
    // In the daily check's own row, under its label.
    expect(daily.closest(".px-2\\.5")?.contains(what)).toBe(true);
    expect(daily).toHaveAccessibleDescription(what.textContent ?? "");
    // After the container, not in it: the footnote, in small muted text,
    // saying which apps the switch above it adds -- the switch's
    // description, so its name need not be said again.
    const group = screen.getByRole("heading", { level: 2, name: "Updates" }).nextElementSibling as HTMLElement;
    const footnote = group.nextElementSibling as HTMLElement;
    expect(footnote).toHaveTextContent(
      "Also list apps installed with Homebrew that update themselves, like Chrome, under Updates.",
    );
    expect(footnote.className.split(" ")).toEqual(
      expect.arrayContaining(["mt-1.5", "px-2.5", "text-small", "leading-4", "text-muted"]),
    );
    expect(within(updates).getByRole("switch", { name: "Show apps that update themselves" })).toHaveAccessibleDescription(
      footnote.textContent ?? "",
    );
  });

  it("says in Chinese what turns the notification on while the daily check is off", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return baseSettings();
      throw new Error(`unexpected command ${cmd}`);
    });
    await i18n.changeLanguage("zh-CN");
    try {
      renderWithProviders(<SettingsPage />);

      const notify = await screen.findByRole("switch", { name: "有更新时通知我" });
      expect(notify).toBeDisabled();
      expect(notify).toHaveAccessibleDescription("请先打开上方的“每天自动检查”。");
    } finally {
      await i18n.changeLanguage("en");
    }
  });

  it("calls the groups and the self-updating switch what the copy table has them in Chinese", () => {
    expect(zhCN.settings.groups).toEqual({ general: "通用", updates: "更新", about: "关于" });
    expect(zhCN.settings.includeSelfUpdating.label).toBe("显示会自行更新的App");
    // The switch adds Homebrew's self-updating apps and nothing else, so
    // its line names Homebrew; it is the switch's description, so it
    // does not say the switch's name over again.
    expect(zhCN.settings.includeSelfUpdating.description).toBe(
      "在“更新”中也显示通过Homebrew安装、会自行更新的App，例如Chrome。",
    );
    expect(zhCN.settings.includeSelfUpdating.description).toContain("Homebrew");
    expect(zhCN.settings.includeSelfUpdating.description).not.toContain(zhCN.settings.includeSelfUpdating.label);
    // An empty group of hidden updates says so in one word, as System
    // Settings' lists do.
    expect(zhCN.settings.hiddenNone).toBe("无");
    expect(zhCN.settings.hiddenEntryMeta).toBe("{{source}} · {{version}}");
  });

  it("puts a skipped version back with its own undo, from its group, and leaves the other list alone", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string, _args?: unknown) => {
      if (cmd === "get_settings") {
        return baseSettings({ ignored_updates: [jqKey], skipped_versions: [{ key: glibKey, version: "2.90.0" }] });
      }
      if (cmd === "set_settings") return undefined;
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const skipped = await screen.findByRole("region", { name: "Skipped versions" });
    fireEvent.click(within(skipped).getByRole("button", { name: "Stop skipping 2.90.0 of glib" }));

    await waitFor(() => expect(within(skipped).getByText("None")).toBeInTheDocument());
    expect(lastSaved().skipped_versions).toEqual([]);
    expect(lastSaved().ignored_updates).toEqual([jqKey]);
    const never = screen.getByRole("region", { name: "Tools with reminders off" });
    expect(within(never).getByRole("button", { name: "Remind me again about jq" })).toBeInTheDocument();
  });
});

describe("SettingsPage's version", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return baseSettings();
      throw new Error(`unexpected command ${cmd}`);
    });
  });

  it("is the version in tauri.conf.json, the one Tauri gives the bundle", () => {
    expect(tauriConfig.version).toMatch(/^\d+\.\d+\.\d+/);
    expect(__APP_VERSION__).toBe(tauriConfig.version);
  });

  it("heads the About group as System Settings shows it: a plain row, the version on the right, muted and selectable", async () => {
    renderWithProviders(<SettingsPage />);

    const about = await screen.findByRole("region", { name: "About" });
    const value = within(about).getByText(tauriConfig.version);
    const row = value.parentElement as HTMLElement;
    // The group's first row, 36 high, its label on the left.
    expect(row).toBe(about.querySelector("h2 + div")?.firstElementChild);
    expect(row.className).toBe(GROUP_ROW);
    expect(row.firstElementChild).toHaveTextContent(/^Version$/);
    // The value: in the muted colour, and text a user can copy.
    expect(value.className.split(" ")).toEqual(
      expect.arrayContaining(["text-body", "text-muted", "select-text"]),
    );
    // Nothing to press in it: the group's buttons are the credits' and
    // Copy Diagnostic Info's.
    expect(within(row).queryByRole("button")).toBeNull();
    expect(
      within(about)
        .getAllByRole("button")
        .map((button) => button.getAttribute("aria-label") ?? button.textContent),
    ).toEqual(["View icon credits", "Copy Diagnostic Info"]);
  });

  it("is called 「版本」 in Chinese, with the same version beside it", async () => {
    expect(zhCN.settings.version).toBe("版本");
    await i18n.changeLanguage("zh-CN");
    try {
      renderWithProviders(<SettingsPage />);

      const about = await screen.findByRole("region", { name: "关于" });
      expect(within(about).getAllByText(/./).map((node) => node.textContent).slice(0, 3)).toEqual([
        "关于",
        "版本",
        tauriConfig.version,
      ]);
    } finally {
      await i18n.changeLanguage("en");
    }
  });
});

describe("SettingsPage's icon credits", () => {
  /**
   * A pack of this test's own: two logos under a license of their own,
   * listed out of order, one under Simple Icons' CC0 and one avatar.
   */
  const PACK: ToolIconPack = {
    version: 1,
    generated: "2026-09-28",
    glyphs: {
      "si-rust": {
        path: "M0 0h24v24H0z",
        hex: "000000",
        title: "Rust",
        license: { type: "CC-BY-SA-4.0", url: "https://spdx.org/licenses/CC-BY-SA-4.0" },
        source: "https://www.rust-lang.org",
      },
      "si-npm": { path: "M0 0h24v24H0z", hex: "CB3837", title: "npm" },
      "si-git": {
        path: "M0 0h24v24H0z",
        hex: "F03C2E",
        title: "Git",
        license: { type: "CC-BY-3.0", url: "https://spdx.org/licenses/CC-BY-3.0" },
        source: "https://git-scm.com/community/logos",
      },
    },
    rasters: { "gh-openai": { file: "gh-openai.webp", title: "openai" } },
    tools: { "brew:git": "si-git", "npm:@openai/codex": "gh-openai" },
    sources: { npm: "si-npm", cargo: "si-rust" },
  };
  const toolIcons = loadToolIcons(PACK, new Map());

  beforeEach(() => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") return baseSettings();
      throw new Error(`unexpected command ${cmd}`);
    });
  });

  it("opens from the About card and lists every logo with a license of its own, with its license and source", async () => {
    const user = userEvent.setup();
    renderWithProviders(<SettingsPage />, { toolIcons });

    const about = await screen.findByRole("region", { name: "About" });
    expect(screen.getByRole("heading", { level: 2, name: "About" }).className).toContain("text-title");
    // Its row under the version's: its name and its button, which opens
    // more (「查看…」), and no sentence explaining it.
    const open = within(about).getByRole("button", { name: "View icon credits" });
    expect(open).toHaveTextContent("View…");
    expect(within(about).getAllByText(/./).map((node) => node.textContent)).toEqual([
      "About",
      "Version",
      tauriConfig.version,
      "Icon credits",
      "View…",
      // Then Copy Diagnostic Info, its checkbox and what the text holds
      // (DiagnosticsRows.test.tsx).
      "Diagnostic info",
      "Copy Diagnostic Info",
      "Include the list of installed tools",
      "To paste to someone helping you. It has the macOS version and where each source is and how it's doing, with your home folder written as ~ and no values of environment variables.",
    ]);
    await user.click(open);

    const drawer = await screen.findByRole("dialog", { name: "Icon credits" });
    // A list: as wide as a dialog about several tools, not an alert's 360.
    expect(drawer).toHaveAttribute("data-dialog-width", "480");
    expect(drawer).toHaveAccessibleDescription(
      "Each logo belongs to its owner and is shown only to identify the tool.",
    );
    expect(
      within(drawer).getByText(
        "Most of the logos built into Banager come from Simple Icons, which is released under CC0.",
      ),
    ).toBeInTheDocument();
    const list = within(drawer).getByRole("list", { name: "The following logos use other licenses:" });
    const items = within(list).getAllByRole("listitem");
    // By title, each with its license's name and the site Simple Icons
    // took it from -- never a whole address, which breaks mid-word at this
    // width -- each address whole in its tooltip; npm's logo, under Simple
    // Icons' CC0, is not listed.
    expect(items.map((item) => item.firstElementChild?.textContent)).toEqual(["Git", "Rust"]);
    const facts = (item: HTMLElement) => [
      within(item).getAllByRole("term").map((term) => term.textContent),
      within(item).getAllByRole("definition").map((definition) => definition.textContent),
      within(item).getAllByRole("definition").map((definition) => definition.getAttribute("title")),
    ];
    expect(facts(items[0])).toEqual([
      ["License", "Source"],
      ["CC-BY-3.0", "git-scm.com"],
      ["https://spdx.org/licenses/CC-BY-3.0", "https://git-scm.com/community/logos"],
    ]);
    expect(facts(items[1])).toEqual([
      ["License", "Source"],
      ["CC-BY-SA-4.0", "rust-lang.org"],
      ["https://spdx.org/licenses/CC-BY-SA-4.0", "https://www.rust-lang.org"],
    ]);
    // Nothing that breaks a word in two to fit.
    for (const definition of within(list).getAllByRole("definition")) {
      expect(definition.className).not.toContain("break-all");
    }
    expect(within(drawer).queryByText(/npm/)).toBeNull();
    expect(
      within(drawer).getByText(
        "Built-in logos that do not come from Simple Icons are the avatars of the projects' GitHub organizations.",
      ),
    ).toBeInTheDocument();
  });

  it("names a logo's source by its site, and on GitHub by its organization too", () => {
    expect(creditSource("https://github.com/dotnet/brand/blob/c7d0f51b8ec5/logo/dotnet-logo.svg")).toBe("github.com/dotnet");
    expect(creditSource("https://simpleicons.org/?q=git")).toBe("simpleicons.org");
    expect(creditSource("https://partnermarketinghub.withgoogle.com/brands/android/visual-identity/logo-lock-ups")).toBe(
      "partnermarketinghub.withgoogle.com",
    );
    expect(creditSource("https://www.apache.org/foundation/press/kit")).toBe("apache.org");
    expect(creditSource("https://github.com")).toBe("github.com");
    // Not an address: as it is.
    expect(creditSource("the Rust Foundation")).toBe("the Rust Foundation");
  });

  it("lists nothing under a license of its own for a pack with none, and says the rest", async () => {
    const user = userEvent.setup();
    renderWithProviders(<SettingsPage />, { toolIcons: loadToolIcons({ ...PACK, glyphs: {} }, new Map()) });

    await user.click(await screen.findByRole("button", { name: "View icon credits" }));

    const drawer = await screen.findByRole("dialog", { name: "Icon credits" });
    expect(within(drawer).queryByRole("list")).toBeNull();
    expect(within(drawer).queryByText("The following logos use other licenses:")).toBeNull();
    expect(within(drawer).getByText(/^Most of the logos built into Banager/)).toBeInTheDocument();
    expect(within(drawer).getByText(/^Built-in logos that do not come from Simple Icons/)).toBeInTheDocument();
  });

  it("is reachable from the keyboard: Tab to it, Enter opens it, its credits take the focus to scroll, Escape gives the focus back", async () => {
    const user = userEvent.setup();
    renderWithProviders(<SettingsPage />, { toolIcons });

    const open = await screen.findByRole("button", { name: "View icon credits" });
    for (let tabs = 0; tabs < 10 && document.activeElement !== open; tabs++) await user.tab();
    expect(open).toHaveFocus();

    await user.keyboard("{Enter}");
    const drawer = await screen.findByRole("dialog", { name: "Icon credits" });
    // Done, its one button and the default one, as the log's.
    const done = within(drawer).getByRole("button", { name: "Done" });
    expect(done).toHaveFocus();
    expect(done.className).toBe(BUTTON.large.default);
    // Nothing in the credits takes the focus, so they do, as a whole.
    await user.tab();
    expect(within(drawer).getByRole("region", { name: "Icon credits" })).toHaveFocus();

    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await waitFor(() => expect(open).toHaveFocus());
  });

  it("calls the credits 图标来源 in Chinese, with a verb on the button", () => {
    expect(zhCN.settings.iconCredits.label).toBe("图标来源");
    expect(zhCN.settings.iconCredits.title).toBe("图标来源");
    expect(zhCN.settings.iconCredits.open).toBe("查看…");
    expect(zhCN.settings.iconCredits.openAriaLabel).toBe("查看图标来源");
  });
});

describe("SettingsPage, opened at its hidden updates", () => {
  // jsdom lays nothing out and has no `scrollIntoView`: each element
  // scrolled into view is noted, with how.
  let scrolledIntoView: Array<{ element: Element; options: boolean | ScrollIntoViewOptions | undefined }>;
  beforeEach(() => {
    scrolledIntoView = [];
    Element.prototype.scrollIntoView = function (this: Element, options?: boolean | ScrollIntoViewOptions) {
      scrolledIntoView.push({ element: this, options });
    };
  });
  afterEach(() => {
    delete (Element.prototype as Partial<Element>).scrollIntoView;
  });

  // What `get_settings` answers, with a snapshot that lists jq.
  function serve(settings: () => Promise<Settings>) {
    vi.mocked(invoke).mockImplementation((cmd: string) => {
      if (cmd === "get_settings") return settings();
      if (cmd === "get_snapshot") return Promise.resolve(snapshotOf([artifact(jqKey, "jq")]));
      return Promise.reject(new Error(`unexpected command ${cmd}`));
    });
  }

  it("brings them into view and puts the focus on their first title, from the Overview's count of them", async () => {
    serve(async () => baseSettings({ ignored_updates: [jqKey] }));
    useUiStore.getState().showHiddenUpdates();

    renderWithProviders(<SettingsPage />);

    const heading = await screen.findByRole("heading", { level: 2, name: "Skipped versions" });
    await waitFor(() => expect(heading).toHaveFocus());
    expect(heading).toHaveAttribute("tabindex", "-1");
    // Both groups, their titles and their rows, as little moved as will show them.
    const skipped = screen.getByRole("region", { name: "Skipped versions" });
    const never = screen.getByRole("region", { name: "Tools with reminders off" });
    expect(scrolledIntoView).toEqual([{ element: skipped.parentElement, options: { block: "nearest" } }]);
    expect(skipped.parentElement?.contains(never)).toBe(true);
    expect(skipped.parentElement?.contains(screen.getByRole("region", { name: "About" }))).toBe(false);
    expect(useUiStore.getState().hiddenUpdatesRequested).toBe(false);
  });

  it("does it once its settings have loaded, when they had not yet", async () => {
    let answer: (settings: Settings) => void = () => {};
    serve(() => new Promise<Settings>((resolve) => (answer = resolve)));
    useUiStore.getState().showHiddenUpdates();

    renderWithProviders(<SettingsPage />);

    expect(await screen.findByText("Loading…")).toBeInTheDocument();
    expect(useUiStore.getState().hiddenUpdatesRequested).toBe(true);
    await act(async () => answer(baseSettings()));
    const heading = await screen.findByRole("heading", { level: 2, name: "Skipped versions" });
    await waitFor(() => expect(heading).toHaveFocus());
    expect(useUiStore.getState().hiddenUpdatesRequested).toBe(false);
  });

  it("moves nothing and takes no focus, opened any other way", async () => {
    serve(async () => baseSettings({ ignored_updates: [jqKey] }));
    useUiStore.getState().setPage("settings");

    renderWithProviders(<SettingsPage />);

    const heading = await screen.findByRole("heading", { level: 2, name: "Skipped versions" });
    expect(await screen.findByRole("button", { name: "Remind me again about jq" })).toBeInTheDocument();
    expect(heading).not.toHaveFocus();
    expect(scrolledIntoView).toEqual([]);
  });
});
