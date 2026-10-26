import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { command } from "../test/command";
import { DiscoverPage } from "./DiscoverPage";
import { useUiStore } from "../store/ui";
import i18n from "../i18n";
import type { CatalogEntry, IssuedPlan, OpSummary, Outcome, Snapshot } from "../lib/types";

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
  { id: "homebrew", category: "Basics", install: "HomebrewItself", state: "Installed", installs_here: false },
  {
    id: "claude-code",
    category: "AiAssistants",
    install: { Homebrew: { kind: "Cask", name: "claude-code" } },
    state: "Installed",
    installs_here: true,
  },
  {
    id: "codex",
    category: "AiAssistants",
    install: { Homebrew: { kind: "Cask", name: "codex" } },
    state: "NotInstalled",
    installs_here: true,
  },
  {
    id: "gemini-cli",
    category: "AiAssistants",
    install: { Homebrew: { kind: "Formula", name: "gemini-cli" } },
    state: "NotInstalled",
    installs_here: true,
  },
  {
    id: "ollama",
    category: "LocalModels",
    install: { Homebrew: { kind: "Cask", name: "ollama-app" } },
    state: { NeedsMacOS: { major: 14 } },
    installs_here: false,
  },
  {
    id: "node",
    category: "CommonTools",
    install: { Homebrew: { kind: "Formula", name: "node" } },
    state: "NotInstalled",
    installs_here: true,
  },
  {
    id: "uv",
    category: "CommonTools",
    install: { Homebrew: { kind: "Formula", name: "uv" } },
    state: "NeedsHomebrew",
    installs_here: true,
  },
  {
    id: "ffmpeg",
    category: "CommonTools",
    install: { Homebrew: { kind: "Formula", name: "ffmpeg" } },
    state: "NotInstalled",
    installs_here: true,
  },
];

/** The plan `plan_install` answers for an entry: Homebrew's own install of its formula or cask. */
function installPlan(kind: "Formula" | "Cask", name: string): IssuedPlan {
  const flag = kind === "Cask" ? "--cask" : "--formula";
  return {
    id: `plan-${name}`,
    plan: {
      request: { kind: "Install", instance_id: "brew:/opt/homebrew", artifact_kind: kind, name },
      action: { Command: { program: "/opt/homebrew/bin/brew", args: ["install", flag, name], env: [] } },
      needs_password: kind === "Cask",
      locks: ["brew:/opt/homebrew"],
      cancel_policy: "KillThenReconcile",
      warnings: [],
      affected: [],
      timeout_secs: 1800,
    },
    issued_at: 1_789_700_000,
  };
}

/** An install of `name` from Discover, as `list_operations` lists it. */
function installOp(
  id: number,
  name: string,
  status: OpSummary["status"],
  outcome: Outcome | null = null,
  kind: OpSummary["kind"] = "Install",
): OpSummary {
  return {
    id,
    kind,
    instance_id: "brew:/opt/homebrew",
    artifact_kind: "Formula",
    name,
    status,
    outcome,
    argv_preview: [],
    cancel_policy: "KillThenReconcile",
  };
}

let snapshot: Snapshot;
let entries: CatalogEntry[];
let operations: OpSummary[];
let planInstall: (catalogId: string) => Promise<IssuedPlan>;

beforeEach(() => {
  snapshot = snapshotAt(1_789_700_000);
  entries = catalog;
  operations = [];
  planInstall = async (catalogId) => {
    const entry = catalog.find((candidate) => candidate.id === catalogId)!;
    if (entry.install === "HomebrewItself") throw JSON.stringify({ kind: "refused" });
    return installPlan(entry.install.Homebrew.kind, entry.install.Homebrew.name);
  };
  useUiStore.setState({ opFinishedAt: {} });
  mockInvoke.mockReset();
  mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
    if (cmd === "get_snapshot") return snapshot;
    if (cmd === "get_catalog") return entries;
    if (cmd === "list_operations") return operations;
    if (cmd === "plan_install") return planInstall((args as { catalogId: string }).catalogId);
    if (cmd === "submit_operation") return 7;
    return null;
  });
});

afterEach(async () => {
  await i18n.changeLanguage("en");
});

function calls(name: string) {
  return mockInvoke.mock.calls.filter(([cmd]) => cmd === name);
}

/** Each row's name and what it says of the entry on its right. */
function rows(): [string, string][] {
  return [...document.querySelectorAll("[data-catalog-entry]")].map((row) => [
    row.querySelector(".truncate")?.textContent ?? "",
    row.querySelector("[data-catalog-state]")?.textContent ?? "",
  ]);
}

function row(id: string): HTMLElement {
  return document.querySelector(`[data-catalog-entry="${id}"]`) as HTMLElement;
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
    // Install in the state's place where the catalog installs it from
    // here and this Mac can; the state otherwise.
    expect(rows()).toEqual([
      ["Homebrew", "Installed"],
      ["Claude Code", "Installed"],
      ["Codex", "Install"],
      ["Gemini CLI", "Install"],
      ["Ollama", "Needs macOS 14"],
      ["Node.js", "Install"],
      ["uv", "Needs Homebrew"],
      ["ffmpeg", "Install"],
    ]);
    const codex = row("codex");
    expect(codex).toHaveTextContent("OpenAI's AI coding assistant for Terminal");
    expect(within(codex).getByRole("button", { name: "Install Codex" })).toBeInTheDocument();
    // Who needs it and how it is used, behind the line's ⓘ.
    fireEvent.click(within(codex).getByRole("button", { name: "Details: Codex" }));
    expect(codex).toHaveTextContent("Once installed, type codex in a new Terminal window.");
    expect(screen.getByText("Homebrew and Ollama can't be installed from here yet.")).toBeInTheDocument();
    // Nothing is planned until Install is pressed.
    expect(calls("plan_install")).toEqual([]);
    expect(calls("plan_operation")).toEqual([]);
  });

  it("says nothing of any entry until a check has finished", async () => {
    snapshot = snapshotAt(null);
    renderWithProviders(<DiscoverPage />);
    await waitFor(() => expect(rows()).toHaveLength(8));
    expect(rows().map(([, state]) => state)).toEqual(Array(8).fill(""));
    expect(screen.queryByRole("button", { name: /^Install / })).toBeNull();
  });

  it("previews the install of the entry by its id, and runs that plan only once Install is pressed", async () => {
    renderWithProviders(<DiscoverPage />);
    await waitFor(() => expect(rows()).toHaveLength(8));
    fireEvent.click(within(row("gemini-cli")).getByRole("button", { name: "Install Gemini CLI" }));

    const dialog = await screen.findByRole("alertdialog", { name: "Install “Gemini CLI”?" });
    // The window names the entry, never a package.
    await waitFor(() => expect(calls("plan_install")).toHaveLength(1));
    expect(calls("plan_install")[0][1]).toEqual({ catalogId: "gemini-cli" });
    expect(dialog).toHaveTextContent("Once installed, type gemini in a new Terminal window.");
    expect(dialog).toHaveTextContent("Homebrew downloads it and checks the download before installing it.");
    expect(dialog).toHaveTextContent("Homebrew also installs what it needs, and may update some of what's already installed.");
    // A formula asks for no password.
    expect(dialog).not.toHaveTextContent("password");
    const install = within(dialog).getByRole("button", { name: "Install" });
    await waitFor(() => expect(install).toBeEnabled());
    fireEvent.click(within(dialog).getByRole("button", { name: "Show Command" }));
    expect(await within(dialog).findByText(command("/opt/homebrew/bin/brew install --formula gemini-cli"))).toBeInTheDocument();
    expect(calls("submit_operation")).toHaveLength(0);
    fireEvent.click(install);
    await waitFor(() => expect(calls("submit_operation")).toHaveLength(1));
    expect(calls("submit_operation")[0][1]).toEqual({ planId: "plan-gemini-cli" });
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
  });

  it("says a cask may ask for the Mac's password", async () => {
    renderWithProviders(<DiscoverPage />);
    await waitFor(() => expect(rows()).toHaveLength(8));
    fireEvent.click(within(row("codex")).getByRole("button", { name: "Install Codex" }));
    const dialog = await screen.findByRole("alertdialog", { name: "Install “Codex”?" });
    await waitFor(() => expect(within(dialog).getByRole("button", { name: "Install" })).toBeEnabled());
    expect(dialog).toHaveTextContent(/password/);
  });

  it("says why it cannot install an entry this Mac no longer can, in the row's terms", async () => {
    planInstall = async () => {
      throw JSON.stringify({ kind: "catalog_state", state: "Installed" });
    };
    renderWithProviders(<DiscoverPage />);
    await waitFor(() => expect(rows()).toHaveLength(8));
    fireEvent.click(within(row("node")).getByRole("button", { name: "Install Node.js" }));
    const dialog = await screen.findByRole("alertdialog", { name: "Install “Node.js”?" });
    expect(await within(dialog).findByRole("alert")).toHaveTextContent("It's already installed.");
    expect(within(dialog).getByRole("button", { name: "Install" })).toBeDisabled();
  });

  it("shows an install under way in its row, and how to use the tool once it is there", async () => {
    operations = [installOp(3, "ffmpeg", "Running")];
    const { unmount } = renderWithProviders(<DiscoverPage />);
    await waitFor(() => expect(rows()[7]).toEqual(["ffmpeg", "Installing…"]));
    expect(within(row("ffmpeg")).queryByRole("button", { name: "Install ffmpeg" })).toBeNull();
    unmount();

    // Done: installed, before the check after it says so too, and the
    // line says how to use it, with Copy for what to type.
    operations = [installOp(3, "ffmpeg", "Done", "Succeeded")];
    useUiStore.getState().rememberOpFinished(3, (snapshot.refreshed_at! + 5) * 1000);
    const { unmount: again } = renderWithProviders(<DiscoverPage />);
    await waitFor(() => expect(rows()[7]).toEqual(["ffmpeg", "Installed"]));
    const ffmpeg = row("ffmpeg");
    expect(ffmpeg.querySelector("[data-catalog-use]")).toHaveTextContent("Type ffmpeg in a new Terminal window.");
    expect(within(ffmpeg).getByRole("button", { name: "Copy “ffmpeg”" })).toBeInTheDocument();
    again();

    // The check after it says Installed too: still how to use it.
    entries = catalog.map((entry) => (entry.id === "ffmpeg" ? { ...entry, state: "Installed" } : entry));
    snapshot = snapshotAt(snapshot.refreshed_at! + 10);
    renderWithProviders(<DiscoverPage />);
    await waitFor(() => expect(rows()[7]).toEqual(["ffmpeg", "Installed"]));
    expect(row("ffmpeg").querySelector("[data-catalog-use]")).not.toBeNull();
  });

  it("goes by the catalog once a check after the install says it is not there", async () => {
    // Installed from here, and the check since says it is gone (removed in
    // Terminal): Install again, and no word of the old install.
    operations = [installOp(3, "ffmpeg", "Done", "Succeeded")];
    useUiStore.getState().rememberOpFinished(3, (snapshot.refreshed_at! - 60) * 1000);
    const { unmount } = renderWithProviders(<DiscoverPage />);
    await waitFor(() => expect(rows()[7]).toEqual(["ffmpeg", "Install"]));
    expect(row("ffmpeg").querySelector("[data-catalog-use]")).toBeNull();
    unmount();
    // Nor an install this window did not hear end (before a reload).
    useUiStore.setState({ opFinishedAt: {} });
    const { unmount: second } = renderWithProviders(<DiscoverPage />);
    await waitFor(() => expect(rows()[7]).toEqual(["ffmpeg", "Install"]));
    second();
    // And an uninstall of it since, before any check: its install is over.
    operations = [
      installOp(3, "ffmpeg", "Done", "Succeeded"),
      installOp(4, "ffmpeg", "Done", "Succeeded", "Uninstall"),
    ];
    useUiStore.getState().rememberOpFinished(3, (snapshot.refreshed_at! + 5) * 1000);
    renderWithProviders(<DiscoverPage />);
    await waitFor(() => expect(rows()[7]).toEqual(["ffmpeg", "Install"]));
  });

  it("says an install that ended without the tool, opening its log, with Install to try again", async () => {
    operations = [
      installOp(4, "node", "Done", { Failed: { exit_code: 1, summary: "Error: node: no bottle", cause: null } }),
      installOp(5, "gemini-cli", "Done", { NeedsAttention: "NotInstalledAfterInstall" }),
    ];
    renderWithProviders(<DiscoverPage />);
    await waitFor(() => expect(rows()[5]).toEqual(["Node.js", "Couldn't installInstall"]));
    expect(within(row("node")).getByRole("button", { name: "View log: Node.js" })).toBeInTheDocument();
    expect(within(row("node")).getByRole("button", { name: "Install Node.js" })).toBeInTheDocument();
    expect(rows()[3]).toEqual(["Gemini CLI", "Needs attentionInstall"]);
  });

  it("says why it cannot install again beside an install that failed", async () => {
    // uv's install failed, and the check after it found no Homebrew that
    // can install any more.
    operations = [installOp(4, "uv", "Done", { Failed: { exit_code: 1, summary: "", cause: null } })];
    renderWithProviders(<DiscoverPage />);
    await waitFor(() => expect(rows()[6]).toEqual(["uv", "Couldn't installNeeds Homebrew"]));
    expect(within(row("uv")).queryByRole("button", { name: "Install uv" })).toBeNull();
  });

  it("says it in Chinese and in Taiwan's words", async () => {
    await i18n.changeLanguage("zh-CN");
    const { unmount } = renderWithProviders(<DiscoverPage />);
    await waitFor(() => expect(rows()).toHaveLength(8));
    expect(screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent)).toEqual(["基础", "AI助手", "本地模型", "常用工具"]);
    expect(rows().slice(4, 7)).toEqual([
      ["Ollama", "需要macOS 14"],
      ["Node.js", "安装"],
      ["uv", "需先装Homebrew"],
    ]);
    unmount();
    await i18n.changeLanguage("zh-Hant");
    renderWithProviders(<DiscoverPage />);
    await waitFor(() => expect(rows()).toHaveLength(8));
    expect(rows()[0]).toEqual(["Homebrew", "已安裝"]);
    expect(rows()[5]).toEqual(["Node.js", "安裝"]);
    expect(rows()[6]).toEqual(["uv", "需先安裝Homebrew"]);
  });
});
