import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke, type InvokeArgs } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import i18n from "../i18n";
import App from "../App";
import { queryKeys } from "../lib/queryKeys";
import type { Settings, Snapshot } from "../lib/types";
import { openWelcomeSheet, useWelcomeAgain } from "../lib/welcome";
import { fakeMenuBar } from "../test/menuBar";
import { WelcomeSheet, welcomeDue } from "./WelcomeSheet";
import { BUTTON } from "./ui/controls";

const mockInvoke = vi.mocked(invoke);

const base: Settings = {
  language: "System",
  show_technical_details: true,
  ignored_updates: [],
  skipped_versions: [],
  include_self_updating: false,
  auto_check: true,
  notify_updates: false,
  auto_check_every: "Week",
  notify_operations: false,
  snoozed_updates: [],
};

const snapshot: Snapshot = {
  generation: 1,
  round: 1,
  detect: "Found",
  instances: [],
  artifacts: [],
  updates: [],
  refreshed_at: 1789700000,
  stale: false,
  errors: [],
};

// What `get_settings` answers, and every `set_settings` the page sent.
let served: Settings;
let saved: Settings[];
// What `set_settings` answers: at once, unless a test fails it.
let saveReply: () => Promise<void>;

beforeEach(() => {
  served = { ...base, welcome_seen: false };
  saved = [];
  saveReply = () => Promise.resolve();
  useWelcomeAgain.setState({ open: false });
  mockInvoke.mockReset();
  mockInvoke.mockImplementation((cmd: string, args?: InvokeArgs) => {
    if (cmd === "get_settings") return Promise.resolve(served);
    if (cmd === "set_settings") {
      saved.push((args as { settings: Settings }).settings);
      return saveReply();
    }
    if (cmd === "get_snapshot" || cmd === "refresh") return Promise.resolve(snapshot);
    if (cmd === "list_operations") return Promise.resolve([]);
    return Promise.resolve(undefined);
  });
});

afterEach(async () => {
  await i18n.changeLanguage("en");
});

async function findSheet(): Promise<HTMLElement> {
  return screen.findByRole("dialog", { name: "Welcome to Banager" });
}

/** Lets every pending query answer, so a sheet that was going to open has. */
async function settle(): Promise<void> {
  await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("get_settings"));
  await new Promise((resolve) => setTimeout(resolve, 0));
}

describe("welcomeDue", () => {
  it("asks for the sheet only when the settings say, in so many words, that it has not been shown", () => {
    expect(welcomeDue({ ...base, welcome_seen: false })).toBe(true);
    expect(welcomeDue({ ...base, welcome_seen: true })).toBe(false);
    // Built by hand, by a test or the preview: not asked.
    expect(welcomeDue(base)).toBe(false);
    expect(welcomeDue(undefined)).toBe(false);
  });
});

describe("WelcomeSheet", () => {
  it("shows on the first launch: its title, three points and Get Started, which has the focus", async () => {
    renderWithProviders(<WelcomeSheet />);
    const sheet = await findSheet();
    const points = within(sheet).getAllByRole("listitem");
    // Titles alone, nothing under them (the author, 2026-10-07).
    expect(points.map((point) => point.textContent)).toEqual([
      "See What's Installed",
      "You Confirm Every Update and Uninstall",
      "No Terminal Changes, No Data Collected",
    ]);
    // Each point's symbol is decoration: its title says it.
    for (const point of points) {
      expect(point.querySelector("svg")).toHaveAttribute("aria-hidden", "true");
    }
    const start = within(sheet).getByRole("button", { name: "Get Started" });
    expect(start).toHaveClass(...BUTTON.large.default.split(" "));
    expect(within(sheet).getAllByRole("button")).toHaveLength(1);
    await waitFor(() => expect(start).toHaveFocus());
  });

  it("says it in Chinese", async () => {
    await i18n.changeLanguage("zh-CN");
    renderWithProviders(<WelcomeSheet />);
    const sheet = await screen.findByRole("dialog", { name: "欢迎使用Banager" });
    expect(within(sheet).getByText("看清装了什么")).toBeInTheDocument();
    expect(within(sheet).getByText("更新、卸载都由你确认")).toBeInTheDocument();
    expect(within(sheet).getByText("不改终端配置，不收集数据")).toBeInTheDocument();
    expect(within(sheet).getAllByRole("listitem").map((point) => point.textContent)).toEqual([
      "看清装了什么",
      "更新、卸载都由你确认",
      "不改终端配置，不收集数据",
    ]);
    expect(within(sheet).getByRole("button", { name: "开始使用" })).toBeInTheDocument();
  });

  it("has no sentence under its titles, in any language", async () => {
    // The author asked for the titles alone (2026-10-07): each point is
    // one line, its symbol and its title.
    for (const language of ["en", "zh-CN", "zh-Hant"] as const) {
      await i18n.changeLanguage(language);
      const { unmount } = renderWithProviders(<WelcomeSheet />);
      const sheet = await screen.findByRole("dialog");
      for (const point of within(sheet).getAllByRole("listitem")) {
        expect(point.querySelectorAll("p"), language).toHaveLength(1);
      }
      unmount();
    }
    for (const key of ["welcome.listText", "welcome.confirmText", "welcome.settingsText"]) {
      expect(i18n.exists(key), key).toBe(false);
    }
  });

  it("never shows once it has been seen", async () => {
    served = { ...base, welcome_seen: true };
    renderWithProviders(<WelcomeSheet />);
    await settle();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("never shows for settings that do not mention it", async () => {
    served = base;
    renderWithProviders(<WelcomeSheet />);
    await settle();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("never shows when the settings cannot be read", async () => {
    mockInvoke.mockImplementation((cmd: string) =>
      cmd === "get_settings" ? Promise.reject(new Error("unreadable")) : Promise.resolve(undefined),
    );
    renderWithProviders(<WelcomeSheet />);
    await settle();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it.each([
    ["Get Started", async (user: ReturnType<typeof userEvent.setup>) => {
      await user.click(screen.getByRole("button", { name: "Get Started" }));
    }],
    ["Return", async (user: ReturnType<typeof userEvent.setup>) => {
      await waitFor(() => expect(screen.getByRole("button", { name: "Get Started" })).toHaveFocus());
      await user.keyboard("{Enter}");
    }],
    ["Escape", async (user: ReturnType<typeof userEvent.setup>) => {
      await user.keyboard("{Escape}");
    }],
  ])("closes with %s, and saves that it was seen over the settings as they are, once", async (_how, close) => {
    const user = userEvent.setup();
    renderWithProviders(<WelcomeSheet />);
    await findSheet();
    await close(user);
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    await waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0]).toEqual({ ...base, welcome_seen: true });
  });

  it("saves over a change made while it was up, not over the settings it opened with", async () => {
    const user = userEvent.setup();
    const { queryClient } = renderWithProviders(<WelcomeSheet />);
    await findSheet();
    queryClient.setQueryData<Settings>(queryKeys.settings, { ...base, welcome_seen: false, language: "ZhCn" });
    await user.click(screen.getByRole("button", { name: /Get Started|开始使用/ }));
    await waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0]).toEqual({ ...base, language: "ZhCn", welcome_seen: true });
  });

  it("does not come back in the same launch, even when saving fails", async () => {
    const user = userEvent.setup();
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    saveReply = () => Promise.reject(new Error("disk full"));
    const { queryClient } = renderWithProviders(<WelcomeSheet />);
    await findSheet();
    await user.click(screen.getByRole("button", { name: "Get Started" }));
    await waitFor(() => expect(error).toHaveBeenCalledWith("saving welcome_seen failed", expect.anything()));
    // The settings are read again -- still saying it was not seen.
    await queryClient.invalidateQueries({ queryKey: queryKeys.settings });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    error.mockRestore();
  });

  it("shows nothing at the next launch, once closed", async () => {
    const user = userEvent.setup();
    const first = renderWithProviders(<WelcomeSheet />);
    await findSheet();
    await user.click(screen.getByRole("button", { name: "Get Started" }));
    await waitFor(() => expect(saved).toHaveLength(1));
    first.unmount();
    // The next launch reads what was saved.
    served = saved[0];
    renderWithProviders(<WelcomeSheet />);
    await settle();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});

describe("the welcome sheet in the window", () => {
  it("lets the first check start behind it", async () => {
    renderWithProviders(<App />);
    await findSheet();
    await waitFor(() => expect(mockInvoke).toHaveBeenCalledWith("refresh"));
    expect(screen.getByRole("dialog", { name: "Welcome to Banager" })).toBeInTheDocument();
  });

  it("is not in the window for settings that have seen it", async () => {
    served = { ...base, welcome_seen: true };
    renderWithProviders(<App />);
    await screen.findByRole("heading", { level: 1, name: "Overview" });
    await settle();
    expect(screen.queryByRole("dialog", { name: "Welcome to Banager" })).not.toBeInTheDocument();
  });
});

describe("Help's Welcome to Banager", () => {
  it("shows the sheet again once it has been seen, and closing it saves nothing", async () => {
    served = { ...base, welcome_seen: true };
    const user = userEvent.setup();
    renderWithProviders(<WelcomeSheet />);
    await settle();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();

    act(() => openWelcomeSheet());
    const sheet = await findSheet();
    expect(within(sheet).getAllByRole("listitem")).toHaveLength(3);
    await waitFor(() => expect(within(sheet).getByRole("button", { name: "Get Started" })).toHaveFocus());
    await user.click(within(sheet).getByRole("button", { name: "Get Started" }));

    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(useWelcomeAgain.getState().open).toBe(false);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(saved).toEqual([]);
  });

  it("shows it again as often as it is chosen, and Escape closes it", async () => {
    served = { ...base, welcome_seen: true };
    const user = userEvent.setup();
    renderWithProviders(<WelcomeSheet />);
    await settle();
    for (let time = 0; time < 2; time += 1) {
      act(() => openWelcomeSheet());
      await findSheet();
      await user.keyboard("{Escape}");
      await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    }
    expect(saved).toEqual([]);
  });

  it("is the same one sheet when chosen while the first launch's is up, which saves once on closing", async () => {
    const user = userEvent.setup();
    renderWithProviders(<WelcomeSheet />);
    await findSheet();
    act(() => openWelcomeSheet());
    expect(screen.getAllByRole("dialog")).toHaveLength(1);
    await user.click(screen.getByRole("button", { name: "Get Started" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    await waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0]).toEqual({ ...base, welcome_seen: true });
    expect(useWelcomeAgain.getState().open).toBe(false);
  });

  it("opens from the menu bar over the page that is showing", async () => {
    served = { ...base, welcome_seen: true };
    const menu = fakeMenuBar();
    renderWithProviders(<App />);
    await screen.findByRole("heading", { level: 1, name: "Overview" });
    await settle();
    expect(screen.queryByRole("dialog", { name: "Welcome to Banager" })).not.toBeInTheDocument();

    menu.choose("welcome");

    expect(await findSheet()).toBeInTheDocument();
    // Still behind it, hidden from VoiceOver while the sheet is up.
    expect(screen.getByRole("heading", { level: 1, name: "Overview", hidden: true })).toBeInTheDocument();
  });
});
