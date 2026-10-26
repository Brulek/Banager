import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { command } from "../test/command";
import { DiscoverPage } from "./DiscoverPage";
import { useUiStore } from "../store/ui";
import i18n from "../i18n";
import type { CatalogEntry, IssuedPlan, ManagerInstance, OpSummary, Outcome, Snapshot } from "../lib/types";
import { queryKeys } from "../lib/queryKeys";

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
    if (entry.install === "HomebrewItself" || !("Homebrew" in entry.install)) throw JSON.stringify({ kind: "refused" });
    return installPlan(entry.install.Homebrew.kind, entry.install.Homebrew.name);
  };
  useUiStore.setState({ opFinishedAt: {} });
  mockInvoke.mockReset();
  mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
    if (cmd === "get_snapshot") return snapshot;
    if (cmd === "get_catalog") return entries;
    if (cmd === "list_operations") return operations;
    if (cmd === "plan_install") return planInstall((args as { catalogId: string }).catalogId);
    if (cmd === "submit_operation") {
      // Listed from then on, as the backend lists what it was given.
      operations = [...operations, installOp(7, "gemini-cli", "Queued")];
      return 7;
    }
    if (cmd === "look_for_homebrew") return lookForHomebrew();
    return null;
  });
});

let lookForHomebrew: () => { there: boolean; updating: boolean } = () => ({ there: false, updating: false });

/** The catalog of a Mac with no Homebrew on Apple silicon. */
const noHomebrew: CatalogEntry[] = catalog.map((entry) => ({
  ...entry,
  state: entry.id === "homebrew" ? "NotInstalled" : "NeedsHomebrew",
}));

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
    expect(
      screen.getByText("Tools here install with Homebrew, and the model with Ollama; each install shows its command before it runs."),
    ).toBeInTheDocument();
    expect(screen.getByText("Tools to start with, and which of them this Mac already has.")).toBeInTheDocument();
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

  it("says nothing either from a catalog made before the first check, while the first check's is asked for", async () => {
    // Discover open as Banager starts: its catalog is the empty snapshot's,
    // with no Homebrew (from the independent review of the branch).
    snapshot = snapshotAt(null);
    entries = noHomebrew;
    const { queryClient } = renderWithProviders(<DiscoverPage />);
    await waitFor(() => expect(rows()).toHaveLength(8));
    // The first check commits; its catalog is slow to answer.
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_snapshot") return snapshot;
      if (cmd === "get_catalog") return new Promise(() => {});
      if (cmd === "list_operations") return operations;
      return null;
    });
    act(() => {
      queryClient.setQueryData<Snapshot>(queryKeys.snapshot, { ...snapshotAt(1_789_700_000), generation: 2, round: 2 });
    });
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(rows().map(([, state]) => state)).toEqual(Array(8).fill(""));
    expect(screen.queryByText(/doesn't have Homebrew yet/)).toBeNull();
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
    // The operation bar names it as the row does.
    await waitFor(() => expect(useUiStore.getState().opNames[7]).toBe("Gemini CLI"));
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

  it("keeps an install that ended while a check was under way, one that read Homebrew before it (from the review of 44674745)", async () => {
    // The snapshot was made after the install ended, by a check whose
    // Homebrew task asked before it: the catalog does not have it yet.
    const made = snapshot.refreshed_at!;
    snapshot = {
      ...snapshot,
      instances: [{ id: "brew:/opt/homebrew", answered_at: made - 30 } as ManagerInstance],
    };
    operations = [installOp(3, "ffmpeg", "Done", "Succeeded")];
    useUiStore.getState().rememberOpFinished(3, (made - 10) * 1000);
    const { unmount } = renderWithProviders(<DiscoverPage />);
    await waitFor(() => expect(rows()[7]).toEqual(["ffmpeg", "Installed"]));
    unmount();
    // Once Homebrew has answered after it, the catalog says what there is.
    snapshot = { ...snapshot, instances: [{ id: "brew:/opt/homebrew", answered_at: made } as ManagerInstance] };
    renderWithProviders(<DiscoverPage />);
    await waitFor(() => expect(rows()[7]).toEqual(["ffmpeg", "Install"]));
  });

  it("measures an install against the snapshot the catalog shown was made from, while the next one's is asked for", async () => {
    // ffmpeg installed from here, then uninstalled in Terminal: a check
    // that read Homebrew after the install says Install.
    const made = snapshot.refreshed_at!;
    const brewAt = (answered_at: number) => [{ id: "brew:/opt/homebrew", answered_at } as ManagerInstance];
    snapshot = { ...snapshot, instances: brewAt(made) };
    operations = [installOp(3, "ffmpeg", "Done", "Succeeded")];
    useUiStore.getState().rememberOpFinished(3, (made - 10) * 1000);
    const { queryClient } = renderWithProviders(<DiscoverPage />);
    await waitFor(() => expect(rows()[7]).toEqual(["ffmpeg", "Install"]));
    // The next snapshot comes, and its catalog is slow to answer: the last
    // catalog stands, and it is still newer than the install (from the
    // review of the fix to 44674745).
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_snapshot") return snapshot;
      if (cmd === "get_catalog") return new Promise(() => {});
      if (cmd === "list_operations") return operations;
      return null;
    });
    act(() => {
      queryClient.setQueryData<Snapshot>(queryKeys.snapshot, {
        ...snapshot,
        generation: 2,
        round: 2,
        refreshed_at: made + 60,
        instances: brewAt(made + 50),
      });
    });
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(rows()[7]).toEqual(["ffmpeg", "Install"]);
  });

  it("measures against the catalog's own snapshot where the install ended between its read and the next one's", async () => {
    // From the independent review of the branch: the test above passes on
    // the live snapshot too. Here the catalog shown (the last one's) read
    // Homebrew before the install ended, and the next snapshot after it:
    // only the catalog's own snapshot says the install is newer.
    const made = snapshot.refreshed_at!;
    const brewAt = (answered_at: number) => [{ id: "brew:/opt/homebrew", answered_at } as ManagerInstance];
    snapshot = { ...snapshot, instances: brewAt(made - 30) };
    operations = [installOp(3, "ffmpeg", "Done", "Succeeded")];
    useUiStore.getState().rememberOpFinished(3, (made - 10) * 1000);
    const { queryClient } = renderWithProviders(<DiscoverPage />);
    await waitFor(() => expect(rows()[7]).toEqual(["ffmpeg", "Installed"]));
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_snapshot") return snapshot;
      if (cmd === "get_catalog") return new Promise(() => {});
      if (cmd === "list_operations") return operations;
      return null;
    });
    act(() => {
      queryClient.setQueryData<Snapshot>(queryKeys.snapshot, {
        ...snapshot,
        generation: 2,
        round: 2,
        refreshed_at: made + 60,
        instances: brewAt(made + 50),
      });
    });
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(rows()[7]).toEqual(["ffmpeg", "Installed"]);
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

  it("starts with Homebrew where there is none, installed in Terminal by the person, with the command copied", async () => {
    entries = noHomebrew;
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    let looks = 0;
    lookForHomebrew = () => {
      looks += 1;
      return { there: looks > 1, updating: looks === 2 };
    };
    try {
      renderWithProviders(<DiscoverPage />);
      await waitFor(() => expect(rows()).toHaveLength(8));
      expect(
        screen.getByText("This Mac doesn't have Homebrew yet. Install it first, and then the tools below install here in one click."),
      ).toBeInTheDocument();
      expect(rows()[0]).toEqual(["Homebrew", "Install"]);
      expect(rows()[3]).toEqual(["Gemini CLI", "Needs Homebrew"]);
      fireEvent.click(within(row("homebrew")).getByRole("button", { name: "Install Homebrew" }));

      const sheet = await screen.findByRole("dialog", { name: "Install Homebrew" });
      expect(sheet).toHaveTextContent("Homebrew is installed in Terminal, where you run its installer and type your password.");
      expect(sheet).toHaveTextContent("When it asks for your “Password”, type this Mac's login password and press Return.");
      // Nothing is planned or run for it: no plan, no operation.
      expect(calls("plan_install")).toEqual([]);
      fireEvent.click(within(sheet).getByRole("button", { name: "Show Command" }));
      expect(within(sheet).getByLabelText("Homebrew's install command")).toHaveTextContent(
        '/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"',
      );
      fireEvent.click(within(sheet).getByRole("button", { name: "Open Terminal" }));
      await waitFor(() => expect(calls("open_terminal")).toHaveLength(1));
      expect(writeText).toHaveBeenCalledWith(
        '/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"',
      );
      expect(calls("open_terminal")[0][1]).toBeUndefined();
      // Two looks in a row find it there and not updating: installed, and
      // checked again.
      expect(await within(sheet).findByText("Homebrew is installed.", {}, { timeout: 8000 })).toBeInTheDocument();
      await waitFor(() => expect(calls("refresh")).toHaveLength(1));
      expect(looks).toBeGreaterThanOrEqual(4);
      fireEvent.click(within(sheet).getByRole("button", { name: "Continue" }));
      await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    } finally {
      lookForHomebrew = () => ({ there: false, updating: false });
      Reflect.deleteProperty(navigator, "clipboard");
    }
  }, 15_000);

  it("says where to see why where Homebrew is there and cannot install", async () => {
    snapshot = {
      ...snapshotAt(1_789_700_000),
      instances: [
        {
          id: "brew:/opt/homebrew",
          adapter_id: "brew",
          exe_path: "/opt/homebrew/bin/brew",
          prefix: "/opt/homebrew",
          scope: "User",
          version: "7.0.10",
          answered_at: null,
          unverified_version: null,
          read_only_reason: "PrefixNotWritable",
          status: { unavailable: null, notes: [] },
        },
      ],
    };
    entries = catalog.map((entry) =>
      entry.id === "homebrew" ? entry : { ...entry, state: entry.state === "NotInstalled" ? "HomebrewNotReady" : entry.state },
    );
    renderWithProviders(<DiscoverPage />);
    await waitFor(() => expect(rows()).toHaveLength(8));
    expect(
      screen.getByText("Homebrew is on this Mac but can't install anything right now: its page under Sources says why."),
    ).toBeInTheDocument();
    expect(rows()[2]).toEqual(["Codex", "Homebrew can't install now"]);
    // No search: there is nothing to install it with.
    expect(screen.queryByRole("searchbox")).toBeNull();
  });

  it("says Homebrew no longer installs on an Intel Mac, with nothing to press", async () => {
    entries = noHomebrew.map((entry) => (entry.id === "homebrew" ? { ...entry, state: "HomebrewUnsupported" } : entry));
    renderWithProviders(<DiscoverPage />);
    await waitFor(() => expect(rows()).toHaveLength(8));
    expect(rows()[0]).toEqual(["Homebrew", "Not supported on this Mac"]);
    expect(screen.queryByRole("button", { name: "Install Homebrew" })).toBeNull();
    // The rows each need Homebrew; the line over them says why none can be
    // had here (from the independent review of the branch).
    expect(
      screen.getByText("Homebrew's installer no longer supports Intel Macs, so the tools below can't be installed from here."),
    ).toBeInTheDocument();
  });

  it("offers the model this Mac's memory suits through Ollama, and Open Ollama where Ollama is not running", async () => {
    const ollama = {
      id: "ollama:http://127.0.0.1:11434",
      adapter_id: "ollama",
      exe_path: "/opt/homebrew/bin/ollama",
      prefix: "/Users/you/.ollama",
      scope: "User" as const,
      version: "0.40.1",
      answered_at: null,
      unverified_version: null,
      read_only_reason: null,
      status: { unavailable: null, notes: [] },
    };
    const model: CatalogEntry = {
      id: "model",
      category: "LocalModels",
      install: { OllamaModel: { name: "gemma3:4b", download_bytes: 3_350_272_403 } },
      state: "NotInstalled",
      installs_here: true,
      // Less room than the download needs.
      free_bytes: 2_000_000_000,
    };
    snapshot = { ...snapshotAt(1_789_700_000), instances: [ollama] };
    entries = [model];
    planInstall = async () => ({
      id: "plan-gemma3:4b",
      plan: {
        request: { kind: "Install", instance_id: ollama.id, artifact_kind: "Model", name: "gemma3:4b" },
        action: {
          Command: {
            program: "/opt/homebrew/bin/ollama",
            args: ["pull", "gemma3:4b"],
            env: [["OLLAMA_HOST", "http://127.0.0.1:11434"]],
          },
        },
        needs_password: false,
        locks: [ollama.id],
        cancel_policy: "KillThenReconcile",
        warnings: [],
        affected: [],
        timeout_secs: 7200,
      },
      issued_at: 1_789_700_000,
    });
    const { unmount } = renderWithProviders(<DiscoverPage />);
    await waitFor(() => expect(rows()).toEqual([["Gemma 3", "Install"]]));
    expect(row("model")).toHaveTextContent("An openly released AI model from Google, chosen for this Mac's memory: gemma3:4b, about 3.4 GB");
    fireEvent.click(within(row("model")).getByRole("button", { name: "Install Gemma 3" }));
    const sheet = await screen.findByRole("alertdialog", { name: "Install “Gemma 3”?" });
    await waitFor(() => expect(calls("plan_install")[0][1]).toEqual({ catalogId: "model" }));
    expect(sheet).toHaveTextContent("Ollama downloads gemma3:4b from its library, about 3.4 GB.");
    expect(sheet).toHaveTextContent("This Mac has only 2 GB free, less than the download needs. Free up some space first.");
    expect(sheet).not.toHaveTextContent("Homebrew");
    await waitFor(() => expect(within(sheet).getByRole("button", { name: "Install" })).toBeEnabled());
    fireEvent.click(within(sheet).getByRole("button", { name: "Show Command" }));
    expect(within(sheet).getByText(command(/\/opt\/homebrew\/bin\/ollama pull gemma3:4b$/))).toBeInTheDocument();
    unmount();

    // Room for the download, without a tenth to spare: not "less than it
    // needs" (from the review of 9ba6981e); and plenty, said plainly.
    for (const [free_bytes, said] of [
      [3_600_000_000, "This Mac has only 3.6 GB free, which leaves little room once it's downloaded. Free up some space first."],
      [50_000_000_000, "This Mac has 50 GB free."],
    ] as const) {
      entries = [{ ...model, free_bytes }];
      const { unmount: room } = renderWithProviders(<DiscoverPage />);
      await waitFor(() => expect(rows()).toEqual([["Gemma 3", "Install"]]));
      fireEvent.click(within(row("model")).getByRole("button", { name: "Install Gemma 3" }));
      expect(await screen.findByRole("alertdialog", { name: "Install “Gemma 3”?" })).toHaveTextContent(said);
      room();
    }

    // Ollama is here and not running: Open Ollama, which opens it.
    snapshot = { ...snapshot, instances: [{ ...ollama, status: { unavailable: "NotRunning", notes: [] } }] };
    entries = [{ ...model, state: "OllamaNotReady" }];
    const { unmount: second } = renderWithProviders(<DiscoverPage />);
    await waitFor(() => expect(rows()).toEqual([["Gemma 3", "Open Ollama"]]));
    fireEvent.click(within(row("model")).getByRole("button", { name: "Open Ollama" }));
    await waitFor(() => expect(calls("open_ollama_app")).toHaveLength(1));
    second();

    // Where opening it fails, the row says why, as Ollama's notice does.
    mockInvoke.mockImplementation(async (cmd: string) => {
      if (cmd === "get_snapshot") return snapshot;
      if (cmd === "get_catalog") return entries;
      if (cmd === "list_operations") return [];
      if (cmd === "open_ollama_app") throw JSON.stringify({ kind: "ollama_open_failed", reason: "not_installed" });
      return null;
    });
    const { unmount: third } = renderWithProviders(<DiscoverPage />);
    await waitFor(() => expect(rows()).toEqual([["Gemma 3", "Open Ollama"]]));
    fireEvent.click(within(row("model")).getByRole("button", { name: "Open Ollama" }));
    expect(await within(row("model")).findByRole("alert")).toHaveTextContent("There's no Ollama app in Applications.");
    third();
    mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "get_snapshot") return snapshot;
      if (cmd === "get_catalog") return entries;
      if (cmd === "list_operations") return operations;
      if (cmd === "plan_install") return planInstall((args as { catalogId: string }).catalogId);
      return null;
    });

    // Pulled from here: how to use it, with Copy for what to type.
    snapshot = { ...snapshotAt(1_789_700_000), instances: [ollama] };
    entries = [{ ...model, state: "Installed" }];
    operations = [{ ...installOp(9, "gemma3:4b", "Done", "Succeeded"), instance_id: ollama.id, artifact_kind: "Model" }];
    renderWithProviders(<DiscoverPage />);
    await waitFor(() => expect(rows()).toEqual([["Gemma 3", "Installed"]]));
    expect(row("model").querySelector("[data-catalog-use]")).toHaveTextContent(
      "Type ollama run gemma3:4b in a new Terminal window.",
    );
    expect(within(row("model")).getByRole("button", { name: "Copy “ollama run gemma3:4b”" })).toBeInTheDocument();
  });

  it("offers Open Ollama where Ollama.app is there with no ollama command yet", async () => {
    // No Ollama source: the app was dragged in and never opened. The
    // catalog says Ollama is there and the model waits for it.
    entries = [
      {
        id: "model",
        category: "LocalModels",
        install: { OllamaModel: { name: "gemma3:4b", download_bytes: 3_350_272_403 } },
        state: "OllamaNotReady",
        installs_here: true,
      },
    ];
    renderWithProviders(<DiscoverPage />);
    await waitFor(() => expect(rows()).toEqual([["Gemma 3", "Open Ollama"]]));
    // Opened, and the checks after it still find no `ollama` command (the
    // app was running, or its offer of the command was turned down): the
    // row says so, not a press that did nothing (from the review of
    // 8f7a680b).
    fireEvent.click(within(row("model")).getByRole("button", { name: "Open Ollama" }));
    await waitFor(() => expect(calls("open_ollama_app")).toHaveLength(1));
    expect(
      await within(row("model")).findByText(
        "No ollama command was found after Ollama opened, and downloading the model needs one. If Ollama offers to install it, accept, then click Check Again.",
        {},
        { timeout: 8000 },
      ),
    ).toHaveAttribute("role", "status");
  }, 15_000);

  it("says it in Chinese and in Taiwan's words", async () => {
    await i18n.changeLanguage("zh-CN");
    const { unmount } = renderWithProviders(<DiscoverPage />);
    await waitFor(() => expect(rows()).toHaveLength(8));
    expect(screen.getAllByRole("heading", { level: 2 }).map((h) => h.textContent)).toEqual(["基础", "AI助手", "本地模型", "常用工具"]);
    expect(rows().slice(4, 7)).toEqual([
      ["Ollama", "需要macOS 14"],
      ["Node.js", "安装"],
      ["uv", "需先安装Homebrew"],
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
