import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, screen, waitFor, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { renderWithProviders } from "../test/setup";
import { command } from "../test/command";
import { DiscoverPage } from "../pages/DiscoverPage";
import { searchable } from "../lib/catalog";
import i18n from "../i18n";
import { useUiStore } from "../store/ui";
import { NO_FACTS, type CatalogEntry, type Found, type IssuedPlan, type ManagerInstance, type Snapshot } from "../lib/types";

const mockInvoke = vi.mocked(invoke);

const brew: ManagerInstance = {
  id: "brew:/opt/homebrew",
  adapter_id: "brew",
  exe_path: "/opt/homebrew/bin/brew",
  prefix: "/opt/homebrew",
  scope: "User",
  version: "7.0.10",
  answered_at: null,
  unverified_version: null,
  read_only_reason: null,
  status: { unavailable: null, notes: [] },
};

const snapshot: Snapshot = {
  generation: 1,
  round: 1,
  detect: "Found",
  instances: [brew],
  // jq is installed: the results say so as the last check does.
  artifacts: [
    {
      key: { instance_id: brew.id, kind: "Formula", name: "jq" },
      display_name: "jq",
      version: "1.8.1",
      reason: "Requested",
      description: null,
      homepage: null,
      size_bytes: null,
      installed_at: null,
      path: null,
      auto_updates: false,
      uninstall_blocked: null,
      facts: NO_FACTS,
    },
  ],
  updates: [],
  refreshed_at: 1_789_700_000,
  stale: false,
  errors: [],
};

const catalog: CatalogEntry[] = [
  { id: "homebrew", category: "Basics", install: "HomebrewItself", state: "Installed", installs_here: false },
];

const results: Found[] = [
  { id: "a".repeat(32), kind: "Formula", name: "jq", description: "Lightweight and flexible command-line JSON processor", installed: true, catalog_id: null },
  { id: "b".repeat(32), kind: "Formula", name: "jq-lsp", description: "Jq language server", installed: false, catalog_id: null },
  { id: "c".repeat(32), kind: "Cask", name: "claude-code", description: null, installed: false, catalog_id: "claude-code" },
];

function plan(found: Found): IssuedPlan {
  const flag = found.kind === "Cask" ? "--cask" : "--formula";
  return {
    id: `plan-${found.name}`,
    plan: {
      request: { kind: "Install", instance_id: brew.id, artifact_kind: found.kind, name: found.name },
      action: { Command: { program: "/opt/homebrew/bin/brew", args: ["install", flag, found.name], env: [] } },
      needs_password: found.kind === "Cask",
      locks: [brew.id],
      cancel_policy: "KillThenReconcile",
      warnings: [],
      affected: [],
      timeout_secs: 1800,
    },
    issued_at: 1_789_700_000,
  };
}

let search: (query: string) => Promise<Found[]>;

beforeEach(() => {
  search = async () => results;
  mockInvoke.mockReset();
  mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
    if (cmd === "get_snapshot") return snapshot;
    if (cmd === "get_catalog") return catalog;
    if (cmd === "list_operations") return [];
    if (cmd === "search_homebrew") return search((args as { query: string }).query);
    if (cmd === "plan_install_found") {
      const found = results.find((r) => r.id === (args as { foundId: string }).foundId)!;
      return plan(found);
    }
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

function results_(): [string, string, string][] {
  return [...document.querySelectorAll("[data-found]")].map((row) => [
    row.getAttribute("data-found") ?? "",
    row.querySelector("[data-found-mark]")?.textContent ?? "",
    row.querySelector("[data-found-state]")?.textContent ?? "",
  ]);
}

async function typeSearch(text: string) {
  const field = await screen.findByRole("searchbox", { name: "Search Homebrew for a tool" });
  fireEvent.change(field, { target: { value: text } });
}

describe("Discover's search of Homebrew", () => {
  it("takes the texts brew search may be given, as Rust does", () => {
    expect(searchable("jq")).toBe(true);
    expect(searchable("video editor")).toBe(true);
    expect(searchable("node@22")).toBe(true);
    expect(searchable("视频 ffmpeg")).toBe(true);
    expect(searchable("j")).toBe(false);
    expect(searchable("视频")).toBe(false);
    expect(searchable("视a")).toBe(false);
    expect(searchable("--macports")).toBe(false);
    expect(searchable("user/tap/jq")).toBe(false);
  });

  it("goes by a suggestion's own state: installed another way, or one this Mac cannot install", async () => {
    const entries: CatalogEntry[] = [
      ...catalog,
      {
        id: "claude-code",
        category: "AiAssistants",
        install: { Homebrew: { kind: "Cask", name: "claude-code" } },
        state: "Installed",
        installs_here: true,
      },
    ];
    mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "get_snapshot") return snapshot;
      if (cmd === "get_catalog") return entries;
      if (cmd === "list_operations") return [];
      if (cmd === "search_homebrew") return search((args as { query: string }).query);
      return null;
    });
    const { unmount } = renderWithProviders(<DiscoverPage />);
    await typeSearch("claude");
    await waitFor(() => expect(results_()).toHaveLength(3), { timeout: 3000 });
    // Claude Code from npm, say: the catalog says Installed, and so does
    // its search row -- no second copy.
    expect(results_()[2]).toEqual(["claude-code", "Suggested", "Installed"]);
    unmount();
    entries[entries.length - 1] = { ...entries[entries.length - 1], state: { NeedsMacOS: { major: 14 } } };
    renderWithProviders(<DiscoverPage />);
    await typeSearch("claude");
    await waitFor(() => expect(results_()).toHaveLength(3), { timeout: 3000 });
    expect(results_()[2]).toEqual(["claude-code", "Suggested", "Needs macOS 14"]);
    expect(screen.queryByRole("button", { name: "Install claude-code" })).toBeNull();
  });

  it("says a suggestion found by searching in its own words", async () => {
    renderWithProviders(<DiscoverPage />);
    await typeSearch("claude");
    await waitFor(() => expect(results_()).toHaveLength(3), { timeout: 3000 });
    fireEvent.click(screen.getByRole("button", { name: "Install claude-code" }));
    const sheet = await screen.findByRole("alertdialog", { name: "Install “claude-code”?" });
    expect(sheet).toHaveTextContent("Once installed, type claude in a new Terminal window.");
    expect(sheet).not.toHaveTextContent("isn't one of the tools suggested here");
    // Installed, the operation bar keeps the suggestion's own name.
    useUiStore.setState({ opNames: {} });
    await waitFor(() => expect(within(sheet).getByRole("button", { name: "Install" })).toBeEnabled());
    fireEvent.click(within(sheet).getByRole("button", { name: "Install" }));
    await waitFor(() => expect(useUiStore.getState().opNames[7]).toBe("Claude Code"));
  });

  it("lists what Homebrew has in the list's place, marked suggested or not, and installs a result by its id", async () => {
    renderWithProviders(<DiscoverPage />);
    await typeSearch("jq");
    await waitFor(() => expect(results_()).toHaveLength(3), { timeout: 3000 });
    expect(calls("search_homebrew")).toEqual([["search_homebrew", { query: "jq" }]]);
    // The suggestions are out of the way while searching.
    expect(screen.queryByRole("heading", { name: "Basics" })).toBeNull();
    expect(results_()).toEqual([
      ["jq", "Not suggested", "Installed"],
      ["jq-lsp", "Not suggested", "Install"],
      ["claude-code", "Suggested", "Install"],
    ]);
    expect(document.querySelector('[data-found="jq-lsp"]')).toHaveTextContent("Jq language server");
    // Said to a screen reader as they come.
    expect(screen.getByRole("status")).toHaveTextContent("3 results");

    fireEvent.click(screen.getByRole("button", { name: "Install jq-lsp" }));
    const sheet = await screen.findByRole("alertdialog", { name: "Install “jq-lsp”?" });
    await waitFor(() => expect(calls("plan_install_found")).toEqual([["plan_install_found", { foundId: "b".repeat(32) }]]));
    expect(sheet).toHaveTextContent("It isn't one of the tools suggested here, so check that it's the one you want.");
    // Homebrew's description a line of its own: it ends in no stop (from
    // the independent review of the branch).
    expect(within(sheet).getByText("Jq language server").tagName).toBe("P");
    // And the alert reads both, the warning not lost to a screen reader
    // (from the review of 02041f8a).
    expect(sheet).toHaveAccessibleDescription(/Jq language server.*isn't one of the tools suggested here/);
    // A formula's download is checked.
    expect(sheet).toHaveTextContent("checks the download before installing it");
    await waitFor(() => expect(within(sheet).getByRole("button", { name: "Install" })).toBeEnabled());
    fireEvent.click(within(sheet).getByRole("button", { name: "Show Command" }));
    expect(within(sheet).getByText(command("/opt/homebrew/bin/brew install --formula jq-lsp"))).toBeInTheDocument();
    useUiStore.setState({ opNames: {} });
    fireEvent.click(within(sheet).getByRole("button", { name: "Install" }));
    await waitFor(() => expect(calls("submit_operation")[0][1]).toEqual({ planId: "plan-jq-lsp" }));
    // Not one of the suggestions: no name kept for it, so the bar goes by
    // Homebrew's, as for any other operation.
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    expect(useUiStore.getState().opNames).toEqual({});
    // Never a package's name by any other way.
    expect(calls("plan_install")).toEqual([]);
    expect(calls("plan_operation")).toEqual([]);

    // Cleared: the suggestions again.
    await typeSearch("");
    expect(await screen.findByRole("heading", { name: "Basics" })).toBeInTheDocument();
  });

  it("says how to use a suggestion installed from here, with Copy, as its row on the list does", async () => {
    // Claude Code's cask installed from here, before the check after it.
    const installed = {
      id: 7,
      kind: "Install" as const,
      instance_id: brew.id,
      artifact_kind: "Cask" as const,
      name: "claude-code",
      status: "Done" as const,
      outcome: "Succeeded" as const,
      argv_preview: [],
      cancel_policy: "KillThenReconcile" as const,
    };
    const claude: CatalogEntry = {
      id: "claude-code",
      category: "AiAssistants",
      install: { Homebrew: { kind: "Cask", name: "claude-code" } },
      state: "NotInstalled",
      installs_here: true,
    };
    // And Ollama's app, which has nothing to type: no line, as its row on
    // the list has Open Ollama instead (from the review of b5f7fca5).
    const ollamaApp = { ...installed, id: 8, name: "ollama-app" };
    const ollama: CatalogEntry = {
      id: "ollama",
      category: "LocalModels",
      install: { Homebrew: { kind: "Cask", name: "ollama-app" } },
      state: "NotInstalled",
      installs_here: true,
    };
    search = async () => [
      ...results,
      { id: "d".repeat(32), kind: "Cask", name: "ollama-app", description: "Get up and running with large language models locally", installed: false, catalog_id: "ollama" },
    ];
    mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "get_snapshot") return snapshot;
      if (cmd === "get_catalog") return [...catalog, claude, ollama];
      if (cmd === "list_operations") return [installed, ollamaApp];
      if (cmd === "search_homebrew") return search((args as { query: string }).query);
      return null;
    });
    useUiStore.getState().rememberOpFinished(7, (snapshot.refreshed_at! + 5) * 1000);
    useUiStore.getState().rememberOpFinished(8, (snapshot.refreshed_at! + 5) * 1000);
    try {
      renderWithProviders(<DiscoverPage />);
      await typeSearch("claude");
      await waitFor(() => expect(results_()).toHaveLength(4), { timeout: 3000 });
      const row = document.querySelector('[data-found="claude-code"]') as HTMLElement;
      await waitFor(() => expect(row.querySelector("[data-found-use]")).toHaveTextContent("Type claude in a new Terminal window."));
      expect(within(row).getByRole("button", { name: "Copy “claude”" })).toBeInTheDocument();
      // Not one of the suggestions, or not installed from here: what it is.
      expect(document.querySelector('[data-found="jq"] [data-found-use]')).toBeNull();
      const app = document.querySelector('[data-found="ollama-app"]') as HTMLElement;
      expect(results_()[3]).toEqual(["ollama-app", "Suggested", "Installed"]);
      expect(app.querySelector("[data-found-use]")).toBeNull();
      expect(app).toHaveTextContent("Get up and running with large language models locally");
    } finally {
      useUiStore.setState({ opFinishedAt: {} });
    }
  });

  it("promises no check of the download for an app found by searching, which may have no checksum", async () => {
    // From the independent review of the branch: google-chrome, and a
    // third of the casks, are `sha256 :no_check`.
    search = async () => [
      { id: "e".repeat(32), kind: "Cask", name: "iina", description: "Free and open-source media player", installed: false, catalog_id: null },
    ];
    mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "get_snapshot") return snapshot;
      if (cmd === "get_catalog") return catalog;
      if (cmd === "list_operations") return [];
      if (cmd === "search_homebrew") return search((args as { query: string }).query);
      if (cmd === "plan_install_found")
        return plan({ id: "e".repeat(32), kind: "Cask", name: "iina", description: null, installed: false, catalog_id: null });
      return null;
    });
    renderWithProviders(<DiscoverPage />);
    await typeSearch("iina");
    fireEvent.click(await screen.findByRole("button", { name: "Install iina" }, { timeout: 3000 }));
    const sheet = await screen.findByRole("alertdialog", { name: "Install “iina”?" });
    expect(sheet).toHaveTextContent("Some apps give Homebrew no checksum to check the download against.");
    expect(sheet).not.toHaveTextContent("checks the download before installing it");
  });

  it("searches again where a result's id is out of date, and says so in the sheet", async () => {
    // From the independent review of the branch: no test reached this.
    let expired = true;
    mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "get_snapshot") return snapshot;
      if (cmd === "get_catalog") return catalog;
      if (cmd === "list_operations") return [];
      if (cmd === "search_homebrew") return search((args as { query: string }).query);
      if (cmd === "plan_install_found") {
        if (expired) {
          expired = false;
          throw JSON.stringify({ kind: "search_expired" });
        }
        return plan(results.find((r) => r.id === (args as { foundId: string }).foundId)!);
      }
      return null;
    });
    renderWithProviders(<DiscoverPage />);
    await typeSearch("jq");
    await waitFor(() => expect(results_()).toHaveLength(3), { timeout: 3000 });
    fireEvent.click(screen.getByRole("button", { name: "Install jq-lsp" }));
    const sheet = await screen.findByRole("alertdialog", { name: "Install “jq-lsp”?" });
    expect(await within(sheet).findByText("This result is out of date. Search again.")).toBeInTheDocument();
    // The list is searched again, for ids that are good.
    await waitFor(() => expect(calls("search_homebrew")).toHaveLength(2));
  });

  it("says a press that did not start the install, beside the fresh preview, not as an error", async () => {
    // From the independent review of the branch: the submit's error stayed
    // under a preview that worked.
    let refused = true;
    mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "get_snapshot") return snapshot;
      if (cmd === "get_catalog") return catalog;
      if (cmd === "list_operations") return [];
      if (cmd === "search_homebrew") return search((args as { query: string }).query);
      if (cmd === "plan_install_found") return plan(results.find((r) => r.id === (args as { foundId: string }).foundId)!);
      if (cmd === "submit_operation") {
        if (refused) {
          refused = false;
          throw JSON.stringify({ kind: "plan_expired" });
        }
        return 7;
      }
      return null;
    });
    renderWithProviders(<DiscoverPage />);
    await typeSearch("jq");
    await waitFor(() => expect(results_()).toHaveLength(3), { timeout: 3000 });
    fireEvent.click(screen.getByRole("button", { name: "Install jq-lsp" }));
    const sheet = await screen.findByRole("alertdialog", { name: "Install “jq-lsp”?" });
    await waitFor(() => expect(within(sheet).getByRole("button", { name: "Install" })).toBeEnabled());
    fireEvent.click(within(sheet).getByRole("button", { name: "Install" }));
    expect(await within(sheet).findByText("That didn't start, so it was checked again. Install once more.")).toBeInTheDocument();
    expect(calls("plan_install_found")).toHaveLength(2);
    expect(within(sheet).queryByText(/Couldn't start the install/)).toBeNull();
    await waitFor(() => expect(within(sheet).getByRole("button", { name: "Install" })).toBeEnabled());
  });

  it("keeps a suggestion's name for the bar where the sheet was closed before the install's answer came", async () => {
    // From the independent review of the branch: closing the sheet dropped
    // the submit's own callback, and the install still started.
    let answer: (opId: number) => void = () => {};
    mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "get_snapshot") return snapshot;
      if (cmd === "get_catalog") return catalog;
      if (cmd === "list_operations") return [];
      if (cmd === "search_homebrew") return search((args as { query: string }).query);
      if (cmd === "plan_install_found") return plan(results.find((r) => r.id === (args as { foundId: string }).foundId)!);
      if (cmd === "submit_operation") return new Promise<number>((resolve) => (answer = resolve));
      return null;
    });
    useUiStore.setState({ opNames: {} });
    renderWithProviders(<DiscoverPage />);
    await typeSearch("claude");
    await waitFor(() => expect(results_()).toHaveLength(3), { timeout: 3000 });
    fireEvent.click(screen.getByRole("button", { name: "Install claude-code" }));
    const sheet = await screen.findByRole("alertdialog", { name: "Install “claude-code”?" });
    await waitFor(() => expect(within(sheet).getByRole("button", { name: "Install" })).toBeEnabled());
    fireEvent.click(within(sheet).getByRole("button", { name: "Install" }));
    await waitFor(() => expect(calls("submit_operation")).toHaveLength(1));
    fireEvent.click(within(sheet).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("alertdialog")).toBeNull());
    answer(7);
    await waitFor(() => expect(useUiStore.getState().opNames[7]).toBe("Claude Code"));
  });

  it("says what it found nothing for, and what text it does not take, without asking", async () => {
    search = async () => [];
    renderWithProviders(<DiscoverPage />);
    await typeSearch("zzzz");
    expect(await screen.findByText("Homebrew has nothing matching “zzzz”.", {}, { timeout: 3000 })).toBeInTheDocument();
    const refused = "Type a tool's name in English letters, at least two, without “/” or a leading “-”.";
    await typeSearch("--macports");
    expect(await screen.findByText(refused)).toBeInTheDocument();
    // Homebrew matches names by letters and digits only: a text with none
    // would match every package.
    await typeSearch("视频");
    expect(await screen.findByText(refused)).toBeInTheDocument();
    await new Promise((resolve) => setTimeout(resolve, 800));
    expect(calls("search_homebrew").map(([, args]) => args)).toEqual([{ query: "zzzz" }]);
  });

  it("says why it could not search", async () => {
    search = async () => {
      throw JSON.stringify({ kind: "catalog_state", state: "NeedsHomebrew" });
    };
    renderWithProviders(<DiscoverPage />);
    await typeSearch("jq");
    expect(await screen.findByRole("alert", {}, { timeout: 3000 })).toHaveTextContent("Install Homebrew first to search it.");
  });

  it("keeps the list it has where a search asked again in the background fails", async () => {
    // From the review of the fix batch: the five-minute search again that
    // failed replaced every row with an alert.
    const { queryClient } = renderWithProviders(<DiscoverPage />);
    await typeSearch("jq");
    await waitFor(() => expect(results_()).toHaveLength(3), { timeout: 3000 });
    search = async () => {
      throw JSON.stringify({ kind: "not_answered" });
    };
    await act(async () => {
      await queryClient.refetchQueries({ queryKey: ["searchHomebrew"] });
    });
    expect(calls("search_homebrew")).toHaveLength(2);
    expect(results_()).toHaveLength(3);
    expect(screen.queryByRole("alert")).toBeNull();
    expect(screen.getByRole("button", { name: "Install jq-lsp" })).toBeInTheDocument();
    // The first failure settled, and the list still there.
    await new Promise((resolve) => setTimeout(resolve, 100));
    expect(results_()).toHaveLength(3);
    expect(screen.getByRole("status")).toHaveTextContent("3 results");
    // Failing again past its ids' ten minutes: not a list whose Install can
    // only say "out of date", but why it could not search -- by the
    // failure's own time, so it shows without anything else re-rendering
    // the page (from the review of e5e76aee).
    const now = Date.now();
    const later = vi.spyOn(Date, "now").mockReturnValue(now + 11 * 60 * 1000);
    try {
      await act(async () => {
        await queryClient.refetchQueries({ queryKey: ["searchHomebrew"] });
      });
      expect(await screen.findByRole("alert")).toHaveTextContent(/Couldn't search Homebrew/);
      expect(results_()).toHaveLength(0);
      // Nor does the status still say how many there were.
      expect(screen.getByRole("status")).toBeEmptyDOMElement();
    } finally {
      later.mockRestore();
    }
  });

  it("does not keep a list past its ten minutes for a failure that came a minute before them", async () => {
    // From the review of the fix batch: judged by the failure's time alone,
    // a failure at nine minutes kept the list up until the next search.
    const { queryClient } = renderWithProviders(<DiscoverPage />);
    await typeSearch("jq");
    await waitFor(() => expect(results_()).toHaveLength(3), { timeout: 3000 });
    search = async () => {
      throw JSON.stringify({ kind: "not_answered" });
    };
    const now = Date.now();
    const clock = vi.spyOn(Date, "now").mockReturnValue(now + 9 * 60 * 1000);
    try {
      await act(async () => {
        await queryClient.refetchQueries({ queryKey: ["searchHomebrew"] });
      });
      await new Promise((resolve) => setTimeout(resolve, 50));
      expect(results_()).toHaveLength(3);
      // Eleven minutes on, anything that draws the page again: why it
      // could not search, not the list.
      clock.mockReturnValue(now + 11 * 60 * 1000);
      act(() => {
        useUiStore.setState({ opFinishedAt: { 99: now } });
      });
      expect(await screen.findByRole("alert")).toHaveTextContent(/Couldn't search Homebrew/);
      expect(results_()).toHaveLength(0);
    } finally {
      clock.mockRestore();
      useUiStore.setState({ opFinishedAt: {} });
    }
  });

  it("looks at the clock again when the ten minutes are up, with nothing else drawing the page, and again where the clock was behind", async () => {
    // From the reviews of the fix batch: an idle page has nothing but this
    // timer to look again, and a timer that fires before the wall clock
    // agrees must be set again.
    const { queryClient } = renderWithProviders(<DiscoverPage />);
    await typeSearch("jq");
    await waitFor(() => expect(results_()).toHaveLength(3), { timeout: 3000 });
    const answeredAt = queryClient.getQueryState(["searchHomebrew", "jq"])!.dataUpdatedAt;
    search = async () => {
      throw JSON.stringify({ kind: "not_answered" });
    };
    // A failure 50 ms before the ten minutes are up: kept, and a timer set.
    const clock = vi.spyOn(Date, "now").mockReturnValue(answeredAt + 10 * 60 * 1000 - 50);
    try {
      await act(async () => {
        await queryClient.refetchQueries({ queryKey: ["searchHomebrew"] });
      });
      expect(results_()).toHaveLength(3);
      // The timer fires with the clock still behind: kept, and set again.
      await act(async () => {
        await new Promise((resolve) => setTimeout(resolve, 150));
      });
      expect(results_()).toHaveLength(3);
      // The clock catches up: the next firing shows why it could not search.
      clock.mockReturnValue(answeredAt + 10 * 60 * 1000 + 1000);
      expect(await screen.findByRole("alert", {}, { timeout: 2000 })).toHaveTextContent(/Couldn't search Homebrew/);
      expect(results_()).toHaveLength(0);
    } finally {
      clock.mockRestore();
    }
  });

  it("says nothing of how many it found while the text is one it does not take", async () => {
    // From the review of the fix batch: the status kept "3 results" for the
    // 0.6 s the text takes to settle.
    renderWithProviders(<DiscoverPage />);
    await typeSearch("jq");
    await waitFor(() => expect(results_()).toHaveLength(3), { timeout: 3000 });
    expect(screen.getByRole("status")).toHaveTextContent("3 results");
    fireEvent.change(screen.getByRole("searchbox", { name: "Search Homebrew for a tool" }), { target: { value: "jq/" } });
    expect(screen.getByRole("status")).toBeEmptyDOMElement();
  });

  it("says why it could not search in words with technical details on too, not as the refusal's JSON", async () => {
    // From the independent review of the branch.
    search = async () => {
      throw JSON.stringify({ kind: "invalid_query" });
    };
    mockInvoke.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "get_settings")
        return {
          language: "System",
          show_technical_details: true,
          ignored_updates: [],
          skipped_versions: [],
          include_self_updating: false,
          auto_check: false,
          notify_updates: false,
        };
      if (cmd === "get_snapshot") return snapshot;
      if (cmd === "get_catalog") return catalog;
      if (cmd === "list_operations") return [];
      if (cmd === "search_homebrew") return search((args as { query: string }).query);
      return null;
    });
    renderWithProviders(<DiscoverPage />);
    await typeSearch("jq");
    const alert = await screen.findByRole("alert", {}, { timeout: 3000 });
    expect(alert).not.toHaveTextContent("{");
    expect(alert).toHaveTextContent(/in English letters/);
  });

  it("has no search field before a check, nor without a Homebrew that can install", async () => {
    for (const instances of [[], [{ ...brew, read_only_reason: "PrefixNotWritable" as const }]]) {
      mockInvoke.mockImplementation(async (cmd: string) => {
        if (cmd === "get_snapshot") return { ...snapshot, instances };
        if (cmd === "get_catalog") return catalog;
        return null;
      });
      const { unmount } = renderWithProviders(<DiscoverPage />);
      await screen.findByText("Tools to start with, and which of them this Mac already has.");
      expect(screen.queryByRole("searchbox")).toBeNull();
      unmount();
    }
  });
});
