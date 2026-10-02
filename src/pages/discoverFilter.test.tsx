import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { WithToolbarSlot } from "../test/toolbarSlot";
import { InstalledPage } from "./InstalledPage";
import { useUiStore } from "../store/ui";
import { writeInventoryPreview } from "../lib/events";
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

let grokNotes: Snapshot["instances"][number]["status"]["notes"];

function instance(adapterId: string, id: string): Snapshot["instances"][number] {
  return {
    id,
    adapter_id: adapterId,
    exe_path: `/opt/homebrew/bin/${adapterId}`,
    prefix: "/opt/homebrew",
    scope: "User",
    version: "1.0.0",
    status: { unavailable: null, notes: id === GROK ? grokNotes : [] },
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
  grokNotes = [];
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
  it("offers all three after the others, each with how many it shows", async () => {
    renderInstalled();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    expect(options()).toEqual([
      ["all", "All Tools"],
      ["ai", "AI Tools"],
      ["twins", "Installed More Than Once"],
      ["notOnPath", "Not Found in Terminal (2)"],
      ["brewRetired", "Disabled or Deprecated by Homebrew (2)"],
      ["otherVersions", "Keeping Other Versions"],
    ]);

    await i18n.changeLanguage("zh-CN");
    await waitFor(() =>
      expect(options().slice(3).map(([, label]) => label)).toEqual([
        "终端里找不到（2）",
        "Homebrew已停用或弃用（2）",
        "保留了其他版本",
      ]),
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

  it("lists a component Homebrew deprecated, unfolded, so the count is the rows shown", async () => {
    const libfoo: ArtifactKey = { instance_id: BREW, kind: "Formula", name: "libfoo" };
    artifacts = [
      artifact(wget, [{ name: "wget", state: "Runs" }]),
      {
        ...artifact(libfoo, [], { deprecated: { date: "2026-01-01", reason: "unmaintained", replacement: null } }),
        reason: "Dependency",
      },
    ];
    renderInstalled();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    // Over 所有工具 it stays behind its source's fold, as every component does.
    expect(rowNames()).toEqual(["wget"]);
    expect(screen.getByText("1 more component came with other software")).toBeInTheDocument();
    expect(screen.getByText("1 tool was disabled or deprecated by Homebrew")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Show Tool" }));
    await waitFor(() => expect(rowNames()).toEqual(["libfoo"]));
    expect(screen.queryByText(/more component/)).not.toBeInTheDocument();
  });

  it("lists the formulae Homebrew keeps another version of under Keeping Other Versions, components unfolded", async () => {
    const readline: ArtifactKey = { instance_id: BREW, kind: "Formula", name: "readline" };
    const node: ArtifactKey = { instance_id: BREW, kind: "Formula", name: "node@22" };
    artifacts = [
      ...fullWorld(),
      artifact(node, [{ name: "node", state: "Runs" }], { other_versions: ["22.22.0"] }),
      { ...artifact(readline, [], { other_versions: ["8.3.3"] }), reason: "Dependency" },
    ];
    renderInstalled();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    expect(options()[5]).toEqual(["otherVersions", "Keeping Other Versions (2)"]);
    // No line over 所有工具 points at it: keeping one is common, and the tools work.
    expect(screen.queryByText(/keep other versions/)).not.toBeInTheDocument();
    show("otherVersions");
    await waitFor(() => expect(rowNames()).toEqual(["node@22", "readline"]));
    expect(useUiStore.getState().installedShow).toBe("otherVersions");

    await i18n.changeLanguage("zh-CN");
    await waitFor(() => expect(options()[5]).toEqual(["otherVersions", "保留了其他版本（2）"]));
  });

  it("says none keeps other versions in a source, by name, in Chinese too", async () => {
    await i18n.changeLanguage("zh-CN");
    useUiStore.getState().openInstalled(PIPX);
    useUiStore.getState().setInstalledShow("otherVersions");
    renderInstalled();
    expect(await screen.findByText(/^pipx中没有发现保留了其他版本的工具/)).toBeInTheDocument();
  });

  it("counts only the source in view, and keeps a choice with none, without a number", async () => {
    useUiStore.getState().openInstalled(PIPX);
    renderInstalled();
    await screen.findByText("httpie", { selector: "[data-tool-row] p" });
    expect(options().slice(3)).toEqual([
      ["notOnPath", "Not Found in Terminal (1)"],
      ["brewRetired", "Disabled or Deprecated by Homebrew"],
      ["otherVersions", "Keeping Other Versions"],
    ]);
  });

  it("says none was found when nothing matches, in either language", async () => {
    artifacts = [artifact(wget, [{ name: "wget", state: "Runs" }])];
    renderInstalled();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    expect(options().slice(3)).toEqual([
      ["notOnPath", "Not Found in Terminal"],
      ["brewRetired", "Disabled or Deprecated by Homebrew"],
      ["otherVersions", "Keeping Other Versions"],
    ]);
    show("otherVersions");
    expect(await screen.findByText("No tools keeping other versions were found")).toBeInTheDocument();
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

  it("says the commands weren't looked at, not that none was found, when no command has a verdict", async () => {
    // The login shell's PATH was not restored (names, no verdicts), or the
    // folder read ran past its budget (no commands at all): 「没有发现…」
    // would say it had been looked for.
    artifacts = [artifact(wget, [{ name: "wget", state: null }]), artifact(httpie, [])];
    renderInstalled();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    show("notOnPath");
    expect(await screen.findByText("This check didn't look at the commands in Terminal")).toBeInTheDocument();
    expect(screen.queryByText("No tools missing from Terminal were found")).not.toBeInTheDocument();
    await i18n.changeLanguage("zh-CN");
    expect(await screen.findByText("这次检查没有判断终端里的命令")).toBeInTheDocument();
    // Homebrew's own choice goes by Homebrew's facts, which are there.
    show("brewRetired");
    expect(await screen.findByText("没有发现Homebrew已停用或弃用的工具")).toBeInTheDocument();
  });

  it("says those appear when the check finishes while the first check's list is on screen", async () => {
    // The preview's rows carry no commands: they are judged when the round
    // ends. Over a startup snapshot, the page lists the preview.
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === "get_snapshot")
        return Promise.resolve({
          generation: 0,
          round: 0,
          detect: "Missing",
          instances: [],
          artifacts: [],
          updates: [],
          refreshed_at: null,
          stale: false,
          errors: [],
        } satisfies Snapshot);
      if (cmd === "get_settings") return Promise.resolve(settings);
      if (cmd === "list_operations") return Promise.resolve([]);
      return Promise.resolve(undefined);
    });
    const { queryClient } = renderInstalled();
    act(() =>
      writeInventoryPreview(queryClient, {
        round: 1,
        instances: [instance("brew", BREW)],
        artifacts: [artifact(wget, [])],
      }),
    );
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    show("notOnPath");
    expect(await screen.findByText("These appear here when the check finishes")).toBeInTheDocument();
    show("twins");
    expect(await screen.findByText("These appear here when the check finishes")).toBeInTheDocument();
    // Homebrew's two as well: the preview's rows carry no Homebrew facts.
    show("brewRetired");
    expect(await screen.findByText("These appear here when the check finishes")).toBeInTheDocument();
    expect(screen.queryByText(/No tools disabled or deprecated/)).toBeNull();
    show("otherVersions");
    expect(await screen.findByText("These appear here when the check finishes")).toBeInTheDocument();
    await i18n.changeLanguage("zh-CN");
    expect(await screen.findByText("检查完成后会显示在这里")).toBeInTheDocument();
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
    const more = await screen.findByRole("button", { name: /more note/ });
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

  it("tells each Show apart by its line's title, its name staying the word it shows", async () => {
    renderInstalled();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    await unfold();
    const lines = [...document.querySelectorAll<HTMLElement>("[data-notice-line]")];
    // Named for what it shows, as many as the line counts (walk-3 W3-5).
    const shows = lines.map((line) => within(line).getByRole("button", { name: "Show Tools" }));
    expect(shows[0]).toHaveAccessibleDescription("2 tools can't be found in Terminal");
    expect(shows[1]).toHaveAccessibleDescription("2 tools were disabled or deprecated by Homebrew");
  });

  it("shows the tools Terminal can't find when its Show is pressed, and the line goes", async () => {
    artifacts = fullWorld().filter((a) => a.facts.homebrew?.deprecated == null && a.facts.homebrew?.disabled == null);
    renderInstalled();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    expect(noticeLines()).toEqual([expect.stringContaining("2 tools can't be found in Terminal")]);
    fireEvent.click(screen.getByRole("button", { name: "Show Tools" }));
    await waitFor(() => expect(rowNames()).toEqual(["Grok Build", "httpie"]));
    expect(useUiStore.getState().installedShow).toBe("notOnPath");
    expect(screen.getByRole("combobox", { name: "Show" })).toHaveValue("notOnPath");
    expect(screen.queryByText("2 tools can't be found in Terminal")).not.toBeInTheDocument();
  });

  it("lets the search go when its Show is pressed, as the count was search aside", async () => {
    artifacts = fullWorld().filter((a) => a.facts.homebrew?.deprecated == null && a.facts.homebrew?.disabled == null);
    renderInstalled();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "wget" } });
    await waitFor(() => expect(rowNames()).toEqual(["wget"]));
    expect(noticeLines()).toEqual([expect.stringContaining("2 tools can't be found in Terminal")]);
    fireEvent.click(screen.getByRole("button", { name: "Show Tools" }));
    await waitFor(() => expect(rowNames()).toEqual(["Grok Build", "httpie"]));
    expect(useUiStore.getState().query).toBe("");
  });

  it("shows what Homebrew disabled or deprecated when that one's Show is pressed", async () => {
    artifacts = fullWorld().filter((a) => a.key.instance_id === BREW);
    renderInstalled();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    expect(noticeLines()).toEqual([expect.stringContaining("2 tools were disabled or deprecated by Homebrew")]);
    fireEvent.click(screen.getByRole("button", { name: "Show Tools" }));
    await waitFor(() => expect(rowNames()).toEqual(["QuickJot", "youtube-dl"]));
  });

  it("doesn't say again what a source's own notice says, but still offers the choice", async () => {
    grokNotes = ["NotOnPath"];
    artifacts = fullWorld().filter((a) => a.key.instance_id !== PIPX && a.key.instance_id !== BREW);
    artifacts.push(artifact(wget, [{ name: "wget", state: "Runs" }]));
    renderInstalled();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    expect(noticeLines()).toEqual([expect.stringContaining("Grok Build is installed")]);
    expect(options()[3]).toEqual(["notOnPath", "Not Found in Terminal (1)"]);
  });

  it("keeps the line, with the whole count, when a source's notice names only some", async () => {
    grokNotes = ["NotOnPath"];
    artifacts = fullWorld().filter((a) => a.key.instance_id !== BREW);
    artifacts.push(artifact(wget, [{ name: "wget", state: "Runs" }]));
    renderInstalled();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    await unfold();
    expect(screen.getByText("2 tools can't be found in Terminal")).toBeInTheDocument();
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

describe("the details and the focus when a choice hides the selected tool", () => {
  async function openDetails(name: string): Promise<HTMLElement> {
    const label = await screen.findByText(name, { selector: "[data-tool-row] p" });
    const row = label.closest("[data-tool-row]") as HTMLElement;
    fireEvent.click(within(row).getByRole("button", { name: i18n.t("common.detailsLabel", { title: name }) }));
    return screen.findByRole("complementary", { name });
  }

  it("closes the details of a tool the choice hides, the focus staying on the popup", async () => {
    renderInstalled();
    await openDetails("wget");
    const popup = screen.getByRole("combobox", { name: "Show" });
    popup.focus();
    show("notOnPath");
    await waitFor(() => expect(rowNames()).toEqual(["Grok Build", "httpie"]));
    await waitFor(() => expect(screen.queryByRole("complementary")).toBeNull());
    expect(popup).toHaveFocus();
    // Every tool again: wget is not selected again behind the user's back.
    show("all");
    await waitFor(() => expect(rowNames()).toContain("wget"));
    expect(screen.queryByRole("complementary")).toBeNull();
    expect(document.querySelector("[data-tool-row][data-selected]")).toBeNull();
  });

  it("keeps the details of a tool the choice still shows", async () => {
    renderInstalled();
    await openDetails("httpie");
    show("notOnPath");
    await waitFor(() => expect(rowNames()).toEqual(["Grok Build", "httpie"]));
    expect(screen.getByRole("complementary", { name: "httpie" })).toBeInTheDocument();
  });

  it("puts the focus on the list's first row once 查看 has gone with its line", async () => {
    artifacts = fullWorld().filter((a) => a.facts.homebrew?.deprecated == null && a.facts.homebrew?.disabled == null);
    renderInstalled();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    const view = screen.getByRole("button", { name: "Show Tools" });
    view.focus();
    fireEvent.click(view);
    await waitFor(() => expect(rowNames()).toEqual(["Grok Build", "httpie"]));
    // The row's name, with its word: Terminal does not find its commands.
    await waitFor(() =>
      expect(document.activeElement?.getAttribute("aria-label")).toBe("Grok Build, Not Found in Terminal"),
    );
  });

  it("puts it on the page's title when the focus was lost and the choice shows nothing", async () => {
    artifacts = [artifact(wget, [{ name: "wget", state: "Runs" }])];
    renderWithProviders(
      <WithToolbarSlot>
        <h1 tabIndex={-1} data-focus-fallback="">
          Installed
        </h1>
        <InstalledPage />
      </WithToolbarSlot>,
    );
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    (document.activeElement as HTMLElement | null)?.blur();
    act(() => useUiStore.getState().setInstalledShow("brewRetired"));
    await screen.findByText("No tools disabled or deprecated by Homebrew were found");
    await waitFor(() => expect(screen.getByRole("heading", { name: "Installed" })).toHaveFocus());
  });
});
