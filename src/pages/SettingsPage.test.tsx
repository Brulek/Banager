import { describe, expect, it, vi, beforeEach } from "vitest";
import { screen, waitFor, fireEvent } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { SettingsPage } from "./SettingsPage";
import type { Settings } from "../lib/types";

function baseSettings(overrides: Partial<Settings> = {}): Settings {
  return {
    language: "System",
    show_technical_details: false,
    ignored_updates: [],
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

    await waitFor(() => expect(screen.getByRole("alert")).toBeInTheDocument());
    await waitFor(() => expect(toggle).not.toBeChecked());
  });

  it("removes an item from the ignored list and saves the shorter list", async () => {
    // `args?: unknown`, as in Tasks 11/12/14: `invoke`'s second parameter is
    // `InvokeArgs` (a union that includes `ArrayBuffer`), and under
    // `strictFunctionTypes` a `Record<string, unknown>` parameter does not
    // accept it — vitest would pass, `pnpm build`'s tsc would not.
    vi.mocked(invoke).mockImplementation(async (cmd: string, _args?: unknown) => {
      if (cmd === "get_settings") {
        return baseSettings({
          ignored_updates: [
            { instance_id: "brew:/opt/homebrew", kind: "Formula", name: "jq" },
          ],
        });
      }
      if (cmd === "set_settings") return undefined;
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const unignoreButton = await screen.findByRole("button", { name: "Stop ignoring jq" });
    fireEvent.click(unignoreButton);

    await waitFor(() =>
      expect(screen.getByText("You haven't ignored any updates.")).toBeInTheDocument(),
    );
    // The optimistic draft shows the empty list before the save resolves, so
    // the line above alone cannot tell a correct payload from a wrong one.
    expect(vi.mocked(invoke)).toHaveBeenCalledWith("set_settings", {
      settings: expect.objectContaining({ ignored_updates: [] }),
    });
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

    const toggle = await screen.findByRole("switch", { name: "Include self-updating apps" });
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
    // every save would put a full seven-source scan behind each Ignore click
    // on the Updates page, which shares this mutation.
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

  it("styles the stop-ignoring button so it reads as a control", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "get_settings") {
        return baseSettings({
          ignored_updates: [{ instance_id: "brew:/opt/homebrew", kind: "Formula", name: "jq" }],
        });
      }
      throw new Error(`unexpected command ${cmd}`);
    });

    renderWithProviders(<SettingsPage />);

    const button = await screen.findByRole("button", { name: "Stop ignoring jq" });
    expect(button.className).not.toBe("");
  });
});
