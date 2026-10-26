import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { DiscoverPage } from "./DiscoverPage";
import i18n from "../i18n";
import type { CatalogEntry, Snapshot } from "../lib/types";

const mockInvoke = vi.mocked(invoke);

function snapshotAt(refreshed_at: number | null): Snapshot {
  return {
    generation: 1,
    round: 1,
    detect: "Found",
    instances: [],
    artifacts: [],
    updates: [],
    refreshed_at,
    stale: false,
    errors: [],
  };
}

/** The catalog as Rust answers it on a Mac with Homebrew and Claude Code, and macOS 13. */
const catalog: CatalogEntry[] = [
  { id: "homebrew", category: "Basics", install: "HomebrewItself", state: "Installed" },
  { id: "claude-code", category: "AiAssistants", install: { Homebrew: { kind: "Cask", name: "claude-code" } }, state: "Installed" },
  { id: "codex", category: "AiAssistants", install: { Homebrew: { kind: "Cask", name: "codex" } }, state: "NotInstalled" },
  { id: "gemini-cli", category: "AiAssistants", install: { Homebrew: { kind: "Formula", name: "gemini-cli" } }, state: "NotInstalled" },
  { id: "ollama", category: "LocalModels", install: { Homebrew: { kind: "Cask", name: "ollama-app" } }, state: { NeedsMacOS: { major: 14 } } },
  { id: "node", category: "CommonTools", install: { Homebrew: { kind: "Formula", name: "node" } }, state: "NotInstalled" },
  { id: "uv", category: "CommonTools", install: { Homebrew: { kind: "Formula", name: "uv" } }, state: "NeedsHomebrew" },
  { id: "ffmpeg", category: "CommonTools", install: { Homebrew: { kind: "Formula", name: "ffmpeg" } }, state: "NotInstalled" },
];

let snapshot: Snapshot;

beforeEach(() => {
  snapshot = snapshotAt(1_789_700_000);
  mockInvoke.mockReset();
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "get_snapshot") return Promise.resolve(snapshot);
    if (cmd === "get_catalog") return Promise.resolve(catalog);
    return Promise.resolve(null);
  });
});

afterEach(async () => {
  await i18n.changeLanguage("en");
});

/** Each row's name and what it says of the entry on its right. */
function rows(): [string, string][] {
  return [...document.querySelectorAll("[data-catalog-entry]")].map((row) => [
    row.querySelector(".truncate")?.textContent ?? "",
    row.querySelector("[data-catalog-state]")?.textContent ?? "",
  ]);
}

describe("the Discover page", () => {
  it("lists the catalog by group, each entry with what it is and whether this Mac has it", async () => {
    renderWithProviders(<DiscoverPage />);
    await waitFor(() => expect(rows()).toHaveLength(8));
    expect(screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent)).toEqual([
      "Basics",
      "AI Assistants",
      "Local Models",
      "Common Tools",
    ]);
    expect(rows()).toEqual([
      ["Homebrew", "Installed"],
      ["Claude Code", "Installed"],
      ["Codex", "Not installed"],
      ["Gemini CLI", "Not installed"],
      ["Ollama", "Needs macOS 14"],
      ["Node.js", "Not installed"],
      ["uv", "Needs Homebrew"],
      ["ffmpeg", "Not installed"],
    ]);
    const codex = document.querySelector('[data-catalog-entry="codex"]') as HTMLElement;
    expect(codex).toHaveTextContent("OpenAI's AI coding assistant for Terminal");
    // Who needs it and how it is used, behind the line's ⓘ.
    fireEvent.click(within(codex).getByRole("button", { name: "Details: Codex" }));
    expect(codex).toHaveTextContent("Once installed, type codex in a new Terminal window.");
    // Nothing on the page installs anything yet, and the page says so.
    expect(screen.queryByRole("button", { name: /install/i })).toBeNull();
    expect(screen.getByText("This page lists these tools; installing them from here isn't available yet.")).toBeInTheDocument();
    expect(mockInvoke.mock.calls.filter(([cmd]) => cmd === "plan_operation")).toEqual([]);
  });

  it("says nothing of any entry until a check has finished", async () => {
    snapshot = snapshotAt(null);
    renderWithProviders(<DiscoverPage />);
    await waitFor(() => expect(rows()).toHaveLength(8));
    expect(rows().map(([, state]) => state)).toEqual(Array(8).fill(""));
  });

  it("says it in Chinese and in Taiwan's words", async () => {
    await i18n.changeLanguage("zh-CN");
    const { unmount } = renderWithProviders(<DiscoverPage />);
    await waitFor(() => expect(rows()).toHaveLength(8));
    expect(screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent)).toEqual(["基础", "AI助手", "本地模型", "常用工具"]);
    expect(rows().slice(4, 7)).toEqual([
      ["Ollama", "需要macOS 14"],
      ["Node.js", "未安装"],
      ["uv", "需先装Homebrew"],
    ]);
    unmount();
    await i18n.changeLanguage("zh-Hant");
    renderWithProviders(<DiscoverPage />);
    await waitFor(() => expect(rows()).toHaveLength(8));
    expect(rows()[0]).toEqual(["Homebrew", "已安裝"]);
    expect(rows()[6]).toEqual(["uv", "需先安裝Homebrew"]);
  });
});
