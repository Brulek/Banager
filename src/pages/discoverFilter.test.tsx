import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, screen, waitFor } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { WithToolbarSlot } from "../test/toolbarSlot";
import { InstalledPage } from "./InstalledPage";
import { useUiStore } from "../store/ui";
import i18n from "../i18n";
import type { ArtifactKey, CommandFact, HomebrewFacts, InstalledArtifact, Settings, Snapshot } from "../lib/types";
import { NO_FACTS } from "../lib/types";

/**
 * The Installed page's 「显示」 popup's discovery choices: 「终端里找不到」,
 * the tools with a command Terminal does not find, and 「Homebrew已停用或
 * 弃用」 -- each with how many it shows -- and the notice lines over 所有工具
 * that point at them, so the facts a tool's inspector shows can be found
 * without opening sixty inspectors.
 */

const mockInvoke = vi.mocked(invoke);

const BREW = "brew:/opt/homebrew";
const PIPX = "pipx:/opt/homebrew/bin";
const GROK = "standalone-grok";

function instance(adapterId: string, id: string): Snapshot["instances"][number] {
  return {
    id,
    adapter_id: adapterId,
    exe_path: `/opt/homebrew/bin/${adapterId}`,
    prefix: "/opt/homebrew",
    scope: "User",
    version: "1.0.0",
    status: { unavailable: null, notes: [] },
    unverified_version: null,
    read_only_reason: null,
  };
}

const EMPTY_HOMEBREW: HomebrewFacts = { deprecated: null, disabled: null, caveats: null, other_versions: [] };

function artifact(
  key: ArtifactKey,
  commands: CommandFact[],
  homebrew: Partial<HomebrewFacts> | null = null,
  displayName = key.name,
): InstalledArtifact {
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
    facts: { ...NO_FACTS, commands, homebrew: homebrew === null ? null : { ...EMPTY_HOMEBREW, ...homebrew } },
  };
}

const wget: ArtifactKey = { instance_id: BREW, kind: "Formula", name: "wget" };
const youtubeDl: ArtifactKey = { instance_id: BREW, kind: "Formula", name: "youtube-dl" };
const quickjot: ArtifactKey = { instance_id: BREW, kind: "Cask", name: "quickjot" };
const httpie: ArtifactKey = { instance_id: PIPX, kind: "Tool", name: "httpie" };
const grok: ArtifactKey = { instance_id: GROK, kind: "Binary", name: "grok" };

const notFound = (dir: string): CommandFact["state"] => ({ NotOnPath: { dir } });

let artifacts: InstalledArtifact[];

const settings: Settings = {
  language: "System",
  show_technical_details: false,
  ignored_updates: [],
  skipped_versions: [],
  include_self_updating: false,
  auto_check: false,
  notify_updates: false,
};

function fullWorld(): InstalledArtifact[] {
  return [
    artifact(wget, [{ name: "wget", state: "Runs" }], { caveats: "a note, not a mark" }),
    artifact(youtubeDl, [{ name: "youtube-dl", state: "Runs" }], {
      deprecated: { date: "2025-11-01", reason: "unmaintained", replacement: "yt-dlp" },
    }),
    artifact(
      quickjot,
      [],
      { disabled: { date: "2026-09-01", reason: "fails_gatekeeper_check", replacement: null } },
      "QuickJot",
    ),
    artifact(httpie, [
      { name: "http", state: notFound("~/.local/bin") },
      { name: "https", state: notFound("~/.local/bin") },
    ]),
    artifact(
      grok,
      [
        { name: "agent", state: notFound("~/.grok/bin") },
        { name: "grok", state: notFound("~/.grok/bin") },
      ],
      null,
      "Grok Build",
    ),
  ];
}

beforeEach(() => {
  artifacts = fullWorld();
  mockInvoke.mockReset();
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockImplementation(function (this: HTMLElement) {
    return this.getAttribute("data-index") === null ? 600 : 56;
  });
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
  mockInvoke.mockImplementation((cmd: string) => {
    if (cmd === "get_snapshot")
      return Promise.resolve({
        generation: 1,
        round: 1,
        detect: "Found",
        instances: [instance("brew", BREW), instance("pipx", PIPX), instance("standalone-grok", GROK)],
        artifacts,
        updates: [],
        refreshed_at: 1789700000,
        stale: false,
        errors: [],
      } satisfies Snapshot);
    if (cmd === "get_settings") return Promise.resolve(settings);
    if (cmd === "list_operations") return Promise.resolve([]);
    return Promise.resolve(undefined);
  });
});

afterEach(() => {
  vi.restoreAllMocks();
  void i18n.changeLanguage("en");
});

function rowNames(): string[] {
  return [...document.querySelectorAll("[data-tool-row]")].map((row) => row.querySelector("p")?.textContent ?? "");
}

function renderInstalled() {
  return renderWithProviders(
    <WithToolbarSlot>
      <InstalledPage />
    </WithToolbarSlot>,
  );
}

function show(value: string) {
  fireEvent.change(screen.getByRole("combobox", { name: /^(Show|显示)$/ }), { target: { value } });
}

function options(): [string, string | null][] {
  const popup = screen.getByRole("combobox", { name: /^(Show|显示)$/ });
  return [...popup.querySelectorAll("option")].map((option) => [option.value, option.textContent]);
}

describe("the Installed page's discovery choices", () => {
  it("offers both after the others, each with how many it shows", async () => {
    renderInstalled();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    expect(options()).toEqual([
      ["all", "All Tools"],
      ["ai", "AI Tools"],
      ["twins", "Installed More Than Once"],
      ["notOnPath", "Not Found in Terminal (2)"],
      ["brewRetired", "Disabled or Deprecated by Homebrew (2)"],
    ]);

    await i18n.changeLanguage("zh-CN");
    await waitFor(() =>
      expect(options().slice(3).map(([, label]) => label)).toEqual(["终端里找不到（2）", "Homebrew已停用或弃用（2）"]),
    );
  });

  it("lists the tools with a command Terminal doesn't find under Not Found in Terminal", async () => {
    renderInstalled();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    show("notOnPath");
    await waitFor(() => expect(rowNames()).toEqual(["Grok Build", "httpie"]));
    expect(useUiStore.getState().installedShow).toBe("notOnPath");
  });

  it("lists what Homebrew disabled or deprecated, not what it only has notes on", async () => {
    renderInstalled();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    show("brewRetired");
    await waitFor(() => expect(rowNames()).toEqual(["QuickJot", "youtube-dl"]));
  });

  it("counts only the source in view, and keeps a choice with none, without a number", async () => {
    useUiStore.getState().openInstalled(PIPX);
    renderInstalled();
    await screen.findByText("httpie", { selector: "[data-tool-row] p" });
    expect(options().slice(3)).toEqual([
      ["notOnPath", "Not Found in Terminal (1)"],
      ["brewRetired", "Disabled or Deprecated by Homebrew"],
    ]);
  });

  it("says none was found when nothing matches, in either language", async () => {
    artifacts = [artifact(wget, [{ name: "wget", state: "Runs" }])];
    renderInstalled();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    expect(options().slice(3)).toEqual([
      ["notOnPath", "Not Found in Terminal"],
      ["brewRetired", "Disabled or Deprecated by Homebrew"],
    ]);
    show("notOnPath");
    expect(await screen.findByText("No tools missing from Terminal were found")).toBeInTheDocument();
    show("brewRetired");
    expect(await screen.findByText("No tools disabled or deprecated by Homebrew were found")).toBeInTheDocument();
    expect(screen.queryByText("Nothing installed")).not.toBeInTheDocument();

    await i18n.changeLanguage("zh-CN");
    expect(await screen.findByText("没有发现Homebrew已停用或弃用的工具")).toBeInTheDocument();
    show("notOnPath");
    expect(await screen.findByText("没有发现终端里找不到的工具")).toBeInTheDocument();
  });

  it("names the source when the page shows one source that has none", async () => {
    useUiStore.getState().openInstalled(PIPX);
    useUiStore.getState().setInstalledShow("brewRetired");
    renderInstalled();
    expect(await screen.findByText(/^No tools disabled or deprecated by Homebrew were found in pipx/)).toBeInTheDocument();
  });

  it("shows every tool again wherever the choice is reset", () => {
    useUiStore.getState().setInstalledShow("notOnPath");
    useUiStore.getState().showInstalledTool(wget);
    expect(useUiStore.getState().installedShow).toBe("all");

    useUiStore.getState().setInstalledShow("brewRetired");
    useUiStore.getState().openPage("installed");
    expect(useUiStore.getState().installedShow).toBe("all");
  });
});

describe("the lines over 所有工具 that point at them", () => {
  function noticeLines(): string[] {
    return [...document.querySelectorAll("[data-notice-line]")].map((line) => line.textContent ?? "");
  }

  async function unfold() {
    const more = await screen.findByRole("button", { name: /more issue/ });
    fireEvent.click(more);
  }

  it("says how many of each there are, each with a Show of its own", async () => {
    renderInstalled();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    await unfold();
    expect(screen.getByText("2 tools can't be found in Terminal")).toBeInTheDocument();
    expect(screen.getByText("2 tools were disabled or deprecated by Homebrew")).toBeInTheDocument();
    expect(noticeLines()).toHaveLength(2);
  });

  it("shows the tools Terminal can't find when its Show is pressed, and the line goes", async () => {
    artifacts = fullWorld().filter((a) => a.facts.homebrew?.deprecated == null && a.facts.homebrew?.disabled == null);
    renderInstalled();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    expect(noticeLines()).toEqual([expect.stringContaining("2 tools can't be found in Terminal")]);
    fireEvent.click(screen.getByRole("button", { name: "Show" }));
    await waitFor(() => expect(rowNames()).toEqual(["Grok Build", "httpie"]));
    expect(useUiStore.getState().installedShow).toBe("notOnPath");
    expect(screen.getByRole("combobox", { name: "Show" })).toHaveValue("notOnPath");
    expect(screen.queryByText("2 tools can't be found in Terminal")).not.toBeInTheDocument();
  });

  it("shows what Homebrew disabled or deprecated when that one's Show is pressed", async () => {
    artifacts = fullWorld().filter((a) => a.key.instance_id === BREW);
    renderInstalled();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    expect(noticeLines()).toEqual([expect.stringContaining("2 tools were disabled or deprecated by Homebrew")]);
    fireEvent.click(screen.getByRole("button", { name: "Show" }));
    await waitFor(() => expect(rowNames()).toEqual(["QuickJot", "youtube-dl"]));
  });

  it("counts only the source in view, and says one in the singular", async () => {
    useUiStore.getState().openInstalled(PIPX);
    renderInstalled();
    await screen.findByText("httpie", { selector: "[data-tool-row] p" });
    expect(noticeLines()).toEqual([expect.stringContaining("1 tool can't be found in Terminal")]);
  });

  it("says nothing while another choice is shown, or when there is none of either", async () => {
    useUiStore.getState().setInstalledShow("ai");
    const { unmount } = renderInstalled();
    await screen.findByText("No common AI coding tools were found on this Mac");
    expect(noticeLines()).toEqual([]);
    unmount();

    useUiStore.getState().setInstalledShow("all");
    artifacts = [artifact(wget, [{ name: "wget", state: "Runs" }])];
    renderInstalled();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    expect(noticeLines()).toEqual([]);
  });

  it("says it in Chinese", async () => {
    await i18n.changeLanguage("zh-CN");
    artifacts = fullWorld().filter((a) => a.key.instance_id !== BREW);
    renderInstalled();
    expect(await screen.findByText("2个工具在终端里找不到")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "查看" })).toBeInTheDocument();
  });
});
