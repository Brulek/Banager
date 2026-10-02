import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { UpdatesToolbar } from "../test/updatesToolbar";
import { WithToolbarSlot } from "../test/toolbarSlot";
import { InstalledPage } from "./InstalledPage";
import { UpdatesPage } from "./UpdatesPage";
import { useUiStore } from "../store/ui";
import { queryKeys } from "../lib/queryKeys";
import i18n from "../i18n";
import { shownBy } from "../lib/families";
import { twinsByArtifact } from "../lib/commands";
import type { ArtifactKey, CommandFact, InstalledArtifact, Settings, Snapshot, UpdateCandidate } from "../lib/types";
import { NO_FACTS } from "../lib/types";

/**
 * The Installed page's 「显示」 popup's third choice, 「装了不止一份」: the
 * rows that carry the 「装了两份」 word (`twinsByArtifact`), and nothing
 * else -- not two programs that merely share a command's name. The
 * Updates page's popup keeps its two.
 */

const mockInvoke = vi.mocked(invoke);

const BREW = "brew:/opt/homebrew";
const NPM = "npm:/opt/homebrew";
const CLAUDE = "standalone-claude";
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
    answered_at: null,
    unverified_version: null,
    read_only_reason: null,
  };
}

function artifact(
  key: ArtifactKey,
  family: string | null,
  commands: CommandFact[],
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
    facts: { ...NO_FACTS, family, commands },
  };
}

const wget: ArtifactKey = { instance_id: BREW, kind: "Formula", name: "wget" };
// Homebrew's `grok` is a regular-expression tool: same command name as
// Grok Build's, no family -- another program with the name, not a copy.
const brewGrok: ArtifactKey = { instance_id: BREW, kind: "Formula", name: "grok" };
const grokBuild: ArtifactKey = { instance_id: GROK, kind: "Binary", name: "grok" };
const npmClaude: ArtifactKey = { instance_id: NPM, kind: "Package", name: "@anthropic-ai/claude-code" };
const nativeClaude: ArtifactKey = { instance_id: CLAUDE, kind: "Binary", name: "claude" };
// An AI tool installed once: in 「AI工具」, not in 「装了不止一份」.
const codex: ArtifactKey = { instance_id: NPM, kind: "Package", name: "@openai/codex" };

let artifacts: InstalledArtifact[];
let updates: UpdateCandidate[];

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
    artifact(wget, null, [{ name: "wget", state: "Runs" }]),
    artifact(brewGrok, null, [{ name: "grok", state: "Runs" }]),
    artifact(grokBuild, "grok-build", [{ name: "grok", state: { ShadowedBy: { by: brewGrok } } }], "Grok Build"),
    artifact(npmClaude, "claude-code", [{ name: "claude", state: "Runs" }]),
    artifact(
      nativeClaude,
      "claude-code",
      [{ name: "claude", state: { ShadowedBy: { by: npmClaude } } }],
      "Claude Code",
    ),
    artifact(codex, "codex", [{ name: "codex", state: "Runs" }]),
  ];
}

beforeEach(() => {
  artifacts = fullWorld();
  updates = [];
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
        instances: [
          instance("brew", BREW),
          instance("npm", NPM),
          instance("standalone-claude", CLAUDE),
          instance("standalone-grok", GROK),
        ],
        artifacts,
        updates,
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

function rowsMarkedTwice(): string[] {
  return [...document.querySelectorAll("[data-tool-row]")]
    .filter((row) => /^Installed (twice|\d+ times)$/.test(row.querySelector("[data-status]")?.textContent ?? ""))
    .map((row) => row.querySelector("p")?.textContent ?? "");
}

function renderInstalled() {
  return renderWithProviders(
    <WithToolbarSlot>
      <InstalledPage />
    </WithToolbarSlot>,
  );
}

function show(value: string) {
  fireEvent.change(screen.getByRole("combobox", { name: "Show" }), { target: { value } });
}

describe("shownBy with 「装了不止一份」", () => {
  it("shows exactly the artifacts that have another copy, and nothing it is not given twins for", () => {
    const all = fullWorld();
    const twins = twinsByArtifact(all);
    expect(all.filter((a) => shownBy("twins", a, twins)).map((a) => a.key.name)).toEqual([
      "@anthropic-ai/claude-code",
      "claude",
    ]);
    expect(shownBy("twins", all[0], undefined)).toBe(false);
    expect(shownBy("twins", undefined, twins)).toBe(false);
  });
});

describe("the Installed page with 「装了不止一份」 shown", () => {
  it("offers it third in the Installed page's popup, after every tool and the AI tools", async () => {
    renderInstalled();
    const popup = await screen.findByRole("combobox", { name: "Show" });
    expect(
      [...popup.querySelectorAll("option")].slice(0, 3).map((option) => [option.value, option.textContent]),
    ).toEqual([
      ["all", "All Tools"],
      ["ai", "AI Tools"],
      // With how many rows it shows, as the choices after it have theirs.
      ["twins", "Installed More Than Once (2)"],
    ]);
  });

  it("lists exactly the rows marked Installed twice, not a namesake, and keeps the choice in the store", async () => {
    renderInstalled();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    const marked = rowsMarkedTwice();
    expect(marked).toEqual(["@anthropic-ai/claude-code", "Claude Code"]);

    show("twins");
    await waitFor(() => expect(rowNames()).toEqual(marked));
    expect(useUiStore.getState().installedShow).toBe("twins");
    expect(useUiStore.getState().updatesShow).toBe("all");
  });

  it("searches within them", async () => {
    renderInstalled();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    show("twins");
    // Both copies of Claude Code: npm's by its name, the native one by
    // its line, "Anthropic's AI coding assistant"; not Grok's two.
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "anthropic" } });
    await waitFor(() => expect(rowNames()).toEqual(["@anthropic-ai/claude-code", "Claude Code"]));
    fireEvent.change(screen.getByRole("searchbox"), { target: { value: "@anthropic" } });
    await waitFor(() => expect(rowNames()).toEqual(["@anthropic-ai/claude-code"]));
  });

  it("says none was found when nothing is installed more than once, in either language", async () => {
    artifacts = fullWorld().filter((a) => a.key.instance_id !== CLAUDE);
    renderInstalled();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    // No number while there are none.
    const popup = screen.getByRole("combobox", { name: "Show" });
    expect(popup.querySelector('option[value="twins"]')?.textContent).toBe("Installed More Than Once");
    show("twins");
    expect(await screen.findByText("Nothing is installed more than once")).toBeInTheDocument();
    expect(screen.queryByText("Nothing installed")).not.toBeInTheDocument();

    await i18n.changeLanguage("zh-CN");
    expect(await screen.findByText("没有发现装了不止一份的工具")).toBeInTheDocument();
  });

  it("says the commands weren't looked at when no row has any, rather than that none is installed twice", async () => {
    // The folder read ran past its budget: no row has a command, and the
    // copies can't be told apart.
    artifacts = fullWorld().map((a) => ({ ...a, facts: { ...a.facts, commands: [] } }));
    renderInstalled();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    show("twins");
    expect(await screen.findByText("This check didn't look at the commands in Terminal")).toBeInTheDocument();
    expect(screen.queryByText("Nothing is installed more than once")).not.toBeInTheDocument();
  });

  it("closes the details of a tool a new check takes out of the choice, and keeps those it doesn't", async () => {
    // Under 装了不止一份, npm's Claude Code is selected; its other copy is
    // uninstalled and the next check lists it alone. Its details close
    // with its row, as when the choice itself hides it.
    const { queryClient } = renderInstalled();
    await screen.findByText("wget", { selector: "[data-tool-row] p" });
    show("twins");
    const label = await screen.findByText("@anthropic-ai/claude-code", { selector: "[data-tool-row] p" });
    const row = label.closest("[data-tool-row]") as HTMLElement;
    fireEvent.click(within(row).getByRole("button", { name: "Details: @anthropic-ai/claude-code" }));
    await screen.findByRole("complementary", { name: "@anthropic-ai/claude-code" });
    const served = queryClient.getQueryData<Snapshot>(queryKeys.snapshot)!;
    // A check that changes nothing the choice goes by keeps it open.
    act(() => queryClient.setQueryData(queryKeys.snapshot, { ...served, generation: 2, artifacts: [...served.artifacts] }));
    expect(screen.getByRole("complementary", { name: "@anthropic-ai/claude-code" })).toBeInTheDocument();
    act(() =>
      queryClient.setQueryData(queryKeys.snapshot, {
        ...served,
        generation: 3,
        artifacts: served.artifacts.filter((a) => a.key.instance_id !== CLAUDE),
      }),
    );
    await waitFor(() => expect(screen.queryByRole("complementary")).toBeNull());
    expect(await screen.findByText("Nothing is installed more than once")).toBeInTheDocument();
  });

  it("names the source when the page shows one source that has none", async () => {
    useUiStore.getState().openInstalled(BREW);
    useUiStore.getState().setInstalledShow("twins");
    renderInstalled();
    expect(await screen.findByText(/^Nothing in Homebrew/)).toBeInTheDocument();
  });

  it("shows every tool again wherever the AI tools' choice is reset", () => {
    useUiStore.getState().setInstalledShow("twins");
    useUiStore.getState().showInstalledTool(wget);
    expect(useUiStore.getState().installedShow).toBe("all");

    useUiStore.getState().setInstalledShow("twins");
    useUiStore.getState().openInstalled(BREW);
    expect(useUiStore.getState().installedShow).toBe("all");

    useUiStore.getState().setInstalledShow("twins");
    useUiStore.getState().openPage("installed");
    expect(useUiStore.getState().installedShow).toBe("all");
  });
});

describe("the Updates page's Show popup", () => {
  it("keeps every tool and the AI tools only", async () => {
    updates = [
      {
        key: codex,
        current: "1.0.0",
        target: "1.1.0",
        channel: "Native",
        checkable: true,
        warnings: [],
        blocked: null,
      },
    ];
    renderWithProviders(
      <UpdatesToolbar>
        <UpdatesPage />
      </UpdatesToolbar>,
    );
    const popup = await screen.findByRole("combobox", { name: "Show" });
    expect([...popup.querySelectorAll("option")].map((option) => option.value)).toEqual(["all", "ai"]);
  });
});
